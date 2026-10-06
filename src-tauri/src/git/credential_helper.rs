//! The app's git credential helper: answers with the connected account's
//! token when that token will work, and fronts Git Credential Manager for
//! everything else.
//!
//! # The two regressions this design carries the scars of
//!
//! **First** (fixed by the token path): connecting GitHub in Settings stored a
//! token only the API ever used. Push and fetch shell out to git, which asked
//! Git Credential Manager -- a separate process that had never heard of that
//! token -- so the app demanded a second sign-in, in a different window, for
//! the account the user had just connected.
//!
//! **Second** (fixed by the refusal marker): an earlier helper answered that
//! token unconditionally. On an organization with OAuth App access
//! restrictions the token is refused for git operations outright, and since
//! git uses the first complete credential a helper returns -- it does not fall
//! through to the next helper after a 401 -- every push to the org failed
//! before Credential Manager was ever asked. The cure at the time was to stop
//! supplying the token entirely, which reintroduced the first regression:
//! every push anywhere opened a Credential Manager window again.
//!
//! The resolution is to listen to git. After a 401 git runs `erase` with the
//! credential that failed. When that credential is ours, the helper records a
//! refusal marker for that repository and stops offering the token there until
//! the user reconnects the account (`auth.json` becoming newer than the marker
//! clears it). One failed push, then Credential Manager handles that repository
//! from then on -- instead of either failing forever or prompting forever.
//!
//! The marker was once per host. One refused repository -- often found by a
//! background fetch nobody saw -- then sent every push on the host to
//! Credential Manager, and the user got sign-in windows while Settings still
//! showed the account as connected (GITWYRM-FRONTEND-1E). Git names the
//! repository only with `credential.useHttpPath`, which `shell::credential_args`
//! sets.
//!
//! # Why Credential Manager is reached through us
//!
//! The helper list handed to git names only this binary; `manager` is not in
//! it. With both in the list, a push that succeeded on our token would make
//! git call `store` on every helper -- writing our OAuth token over the user's
//! own sign-in inside Credential Manager's store. Fronting GCM lets us drop
//! `store`/`erase` for credentials that are ours while forwarding the rest, so
//! the two stores never cross-contaminate.
//!
//! Which of GCM's operations are forwarded depends on who is asking:
//!
//! * **Attended** (the user pressed something): `get`, `store` and `erase` all
//!   forward. A login window is allowed, and erasing a credential the host
//!   genuinely revoked is correct -- the window replaces it immediately.
//! * **Unattended** (`--unattended`, the background sweep): only `get`
//!   forwards. Git erases from every helper after any 401, and GitHub answers
//!   401 for a renamed or deleted repository even when the sign-in is good, so
//!   a background fetch of one stale checkout used to delete the user's saved
//!   sign-in every fifteen minutes. Background work can read the stored
//!   credential; it must never be able to throw it away.
//!
//! # The protocol
//!
//! Git runs `<helper> <operation>` and writes `key=value` lines to stdin,
//! ending with a blank line. For `get`, the helper replies with its own lines.
//! Saying nothing is legal and means "I have nothing".
//!
//! # Diagnostics
//!
//! This process runs before Tauri initializes: no logger, no Sentry. Decisions
//! are appended to `credential-helper.log` next to `auth.json` instead --
//! operation, host and repository, and outcome only. No username, token, or reply ever
//! reaches the trace; the credential passes through this process on its way to
//! git and leaves no record in it. The app-side counterpart is the
//! `git push: ... helpers=[...]` line in `commands::remote`, which does reach
//! the log file and Sentry.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::hosting::registry::ProviderId;

/// The argv token that puts the binary in helper mode.
///
/// Deliberately not a bare word like `get`: this is the *application* binary,
/// and a repo folder or file argument must never be mistaken for a request to
/// read a credential.
pub const HELPER_FLAG: &str = "--credential-helper";

/// The argv token marking a run nobody is watching.
///
/// Added by `shell::credential_args` for background operations. It gates the
/// forwarding rules above: with it, `store` and `erase` stop here.
pub const UNATTENDED_FLAG: &str = "--unattended";

/// Environment variable naming the git program the app itself is using.
///
/// Set by the app on every network command it spawns, so the helper reaches
/// the same git -- and therefore the same Credential Manager -- rather than
/// whichever one is first on PATH. Falls back to `git` when absent, which is
/// what a helper launched outside the app (a test, say) gets.
pub const GIT_PROGRAM_ENV: &str = "GITWYRM_GIT";

/// Whether this process was launched by git to answer a credential request.
///
/// Checked before Tauri initializes: a helper invocation must not build a
/// window, register plugins, or trip the single-instance guard and hand its
/// arguments to the running app. It forwards one request and exits.
pub fn requested(args: &[String]) -> bool {
    args.iter().any(|a| a == HELPER_FLAG)
}

/// The credential operation git asked for: the first argument that is not one
/// of our own flags.
fn operation(args: &[String]) -> &str {
    args.iter()
        .skip(1)
        .find(|a| *a != HELPER_FLAG && *a != UNATTENDED_FLAG)
        .map(String::as_str)
        .unwrap_or("")
}

fn is_unattended(args: &[String]) -> bool {
    args.iter().any(|a| a == UNATTENDED_FLAG)
}

/// The `auth.json` provider whose token authenticates `host`.
///
/// Matched on the hostname git gives us rather than a remote URL, because that
/// is all the protocol supplies. Enterprise and self-hosted installs are not
/// matched: their host names are arbitrary, and guessing wrong would send a
/// GitHub token to an unrelated server. Those fall through to GCM as before.
fn provider_for_host(host: &str) -> Option<ProviderId> {
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    match host.as_str() {
        "github.com" | "www.github.com" => Some(ProviderId::Github),
        "gitlab.com" | "www.gitlab.com" => Some(ProviderId::Gitlab),
        "bitbucket.org" | "www.bitbucket.org" => Some(ProviderId::Bitbucket),
        "dev.azure.com" => Some(ProviderId::AzureDevops),
        _ => None,
    }
}

/// The username git should send alongside the token.
///
/// Every host here authenticates a token over HTTP Basic, but they disagree on
/// what goes in the username field, and sending the wrong one fails as if the
/// token were bad.
fn username_for(provider: ProviderId, stored_email: Option<&str>) -> String {
    match provider {
        // GitHub accepts any non-empty username with a token as the password;
        // this is the conventional one.
        ProviderId::Github => "x-access-token".to_string(),
        ProviderId::Gitlab => "oauth2".to_string(),
        // Bitbucket app passwords authenticate against the account email, which
        // is why `StoredCredential` carries one for this host and no other.
        ProviderId::Bitbucket => stored_email.unwrap_or("x-token-auth").to_string(),
        ProviderId::AzureDevops => "pat".to_string(),
    }
}

/// The fields of one `key=value` request block this helper acts on.
#[derive(Default)]
struct Request {
    protocol: Option<String>,
    host: Option<String>,
    /// `owner/repo.git`. Present because `shell::credential_args` sets
    /// `credential.useHttpPath`; it scopes the refusal marker to one repository.
    path: Option<String>,
    password: Option<String>,
}

/// Parse the request block git wrote to stdin.
///
/// `protocol` is read to refuse anything that is not HTTP(S) -- an SSH remote
/// authenticates with a key and has no business receiving a bearer token.
/// `password` is read only to recognise our own credential inside a `store` or
/// `erase`; it is compared and dropped, never written anywhere.
fn parse_request(block: &str) -> Request {
    let mut req = Request::default();
    for line in block.lines() {
        let line = line.trim_end();
        // A blank line terminates the request block.
        if line.is_empty() {
            break;
        }
        match line.split_once('=') {
            Some(("protocol", v)) => req.protocol = Some(v.to_string()),
            Some(("host", v)) => req.host = Some(v.to_string()),
            Some(("path", v)) if !v.is_empty() => req.path = Some(v.to_string()),
            Some(("password", v)) => req.password = Some(v.to_string()),
            _ => {}
        }
    }
    req
}

// ---------------------------------------------------------------- refusal marker

/// Folder holding one refusal marker per repository, next to `auth.json` so
/// their timestamps are comparable.
const MARKER_DIR: &str = "refused-tokens";

/// The host without a port, lowercased, for comparing and naming.
fn bare_host(host: &str) -> String {
    host.split(':').next().unwrap_or(host).to_ascii_lowercase()
}

/// The file recording that the host refused our token for this repository.
///
/// Per repository, not per host. A host-wide marker meant one refusal -- an
/// organization that blocks third-party apps, or a renamed repository that
/// GitHub answers with 401 even for a good sign-in, possibly from a background
/// fetch nobody saw -- switched the connected account off for every repository
/// on that host. Every push after that asked Credential Manager, which opened
/// a sign-in window for a user whose account still showed as connected
/// (GITWYRM-FRONTEND-1E).
///
/// Without a path (a git run that did not get `credential.useHttpPath`) the
/// marker falls back to the whole host, which is the old behaviour.
///
/// Names are filename-safe: GitHub, GitLab, Bitbucket and Azure names use only
/// letters, digits, `.`, `-` and `_`, and `/` becomes `~`, so two repositories
/// cannot share a marker.
fn marker_path(data_dir: &Path, host: &str, path: Option<&str>) -> PathBuf {
    let mut key = bare_host(host);
    if let Some(path) = path {
        let repo = path
            .trim_matches('/')
            .trim_end_matches(".git")
            .to_ascii_lowercase();
        if !repo.is_empty() {
            key.push('~');
            key.push_str(&repo);
        }
    }
    let safe: String = key
        .chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '~') => c,
            '/' => '~',
            _ => '-',
        })
        .collect();
    data_dir.join(MARKER_DIR).join(safe)
}

/// Where the host-wide marker of earlier versions lived.
fn legacy_marker_path(data_dir: &Path, host: &str) -> PathBuf {
    let safe: String = bare_host(host)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    data_dir.join(format!("git-token-refused-{safe}"))
}

/// Remove a host-wide marker left by an earlier version.
///
/// It switched the connected account off for every repository on the host. A
/// repository that really refuses the token records its own marker on the
/// next attempt, so dropping the old one costs at most one more refusal there.
fn forget_legacy_marker(data_dir: &Path, host: &str) {
    let _ = std::fs::remove_file(legacy_marker_path(data_dir, host));
}

/// Whether a recorded refusal still applies.
///
/// The marker outlives the refusal it records only until the user touches
/// their connection: reconnecting (or refreshing) the account rewrites
/// `auth.json`, and a marker older than the store describes a token that no
/// longer exists. Missing files fail open in the safe direction -- no marker
/// means try the token, no `auth.json` means there is no token to try.
fn refusal_active(data_dir: &Path, host: &str, path: Option<&str>) -> bool {
    let marker = match std::fs::metadata(marker_path(data_dir, host, path)) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let auth = match std::fs::metadata(data_dir.join("auth.json")) {
        Ok(m) => m,
        Err(_) => return true,
    };
    match (marker.modified(), auth.modified()) {
        (Ok(m), Ok(a)) => m >= a,
        _ => false,
    }
}

fn record_refusal(data_dir: &Path, host: &str, path: Option<&str>) {
    let marker = marker_path(data_dir, host, path);
    if let Some(dir) = marker.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Content is irrelevant; the file's mtime is the record.
    let _ = std::fs::write(marker, b"see credential_helper.rs");
}

fn clear_refusal(data_dir: &Path, host: &str, path: Option<&str>) {
    let _ = std::fs::remove_file(marker_path(data_dir, host, path));
}

/// The request block with its `path=` line removed, for Credential Manager.
///
/// The path is only there to scope our own refusal marker. Credential Manager
/// keys the user's saved sign-ins by host; handed a path it would look them up
/// per repository, miss, and prompt.
fn without_path(request: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(request);
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if !line.starts_with("path=") {
            out.push_str(line);
        }
    }
    out.into_bytes()
}

// ---------------------------------------------------------------- stored token

/// The stored credential for the host git asked about, when we may use it.
fn stored_for(data_dir: &Path, host: &str) -> Option<crate::hosting::http::StoredCredential> {
    let provider = provider_for_host(host)?;
    let stored = crate::hosting::http::credential_from_dir(data_dir, provider).ok()??;
    if stored.token.is_empty() {
        return None;
    }
    // A self-hosted install stores its own base URL. The host git asked about
    // is one of the public ones (provider_for_host matched it), so a credential
    // scoped elsewhere is not ours to hand over.
    if stored
        .base_url
        .as_ref()
        .is_some_and(|u| !u.trim().is_empty())
    {
        return None;
    }
    Some(stored)
}

/// The reply for a `get`, or None to fall back to Credential Manager.
fn answer(req: &Request, data_dir: &Path) -> Option<String> {
    // An SSH remote must never receive a token.
    if !matches!(req.protocol.as_deref(), Some("https") | Some("http")) {
        return None;
    }
    let host = req.host.as_deref()?;
    if refusal_active(data_dir, host, req.path.as_deref()) {
        return None;
    }
    let stored = stored_for(data_dir, host)?;
    let provider = provider_for_host(host)?;
    let username = username_for(provider, stored.email.as_deref());
    Some(format!("username={username}\npassword={}\n", stored.token))
}

/// Whether the credential in a `store`/`erase` request is the one we supply.
///
/// Compared by token, not username: exact, and immune to a user whose own
/// Credential Manager username happens to collide with one of ours.
fn is_ours(req: &Request, data_dir: &Path) -> bool {
    match (&req.password, req.host.as_deref()) {
        (Some(password), Some(host)) => {
            stored_for(data_dir, host).is_some_and(|s| s.token == *password)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- GCM forwarding

/// Hand one request block to Credential Manager and forward its reply.
///
/// The reply is never inspected or logged here: the credential passes through
/// this process on the way to git and leaves no trace in it. GCM decides for
/// itself whether it may prompt -- `apply_background_env` sets
/// `GCM_INTERACTIVE=never` on unattended runs and the variable inherits down
/// to the child spawned here.
fn forward_to_gcm(operation: &str, request: &[u8]) {
    let request = without_path(request);
    let git = std::env::var(GIT_PROGRAM_ENV).unwrap_or_else(|_| "git".to_string());
    let mut cmd = Command::new(git);
    cmd.args(["credential-manager", operation])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Credential Manager's own diagnostics ("Cannot prompt because user
        // interactivity has been disabled") reach git's stderr as they do
        // when it is called directly, so nothing is hidden by the detour.
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(super::shell::CREATE_NO_WINDOW);
    }

    let Ok(mut child) = cmd.spawn() else {
        // No Credential Manager reachable: say nothing, exactly as a helper
        // with no stored credential would.
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&request);
        // Dropping closes the pipe, which is how Credential Manager knows the
        // request is complete.
    }
    let Ok(output) = child.wait_with_output() else {
        return;
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = out.write_all(&output.stdout);
    let _ = out.flush();
}

// ---------------------------------------------------------------- tracing

/// Append one decision line to `credential-helper.log` in the data directory.
///
/// This process has no logger and no Sentry -- it runs before either exists --
/// so a plain file is the only way its decisions survive to be read. Callers
/// pass operation, host, and outcome only; nothing that flows through stdin or
/// stdout belongs in a trace line.
fn trace(data_dir: &Path, line: &str) {
    let path = data_dir.join("credential-helper.log");
    // Best-effort rotation: start over rather than grow without bound. The
    // file is diagnostics, not history.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "[{epoch}] {line}");
    }
}

/// The host, plus the repository when git sent one, for trace lines.
///
/// The repository name stays in this local file; nothing here is attached to
/// reports.
fn host_label(req: &Request) -> String {
    let host = req.host.as_deref().unwrap_or("<no host>");
    match req.path.as_deref() {
        Some(path) => format!("{host}/{}", path.trim_end_matches(".git")),
        None => host.to_string(),
    }
}

// ---------------------------------------------------------------- entry point

/// Answer one credential request, then exit.
///
/// Returns the process exit code. Always 0: a helper that fails is expected to
/// say nothing and let git carry on, and a non-zero exit would turn "nothing
/// stored" into a hard fetch failure with a confusing message.
pub fn run(args: &[String], data_dir: PathBuf) -> i32 {
    let operation = operation(args);
    let unattended = is_unattended(args);

    // Read the whole request before anything else: git closes its end after
    // the blank line, and a forwarded block must reach Credential Manager
    // verbatim apart from the `path=` line (see `without_path`).
    let mut request = Vec::new();
    let _ = std::io::stdin().lock().read_to_end(&mut request);
    let req = parse_request(&String::from_utf8_lossy(&request));
    let path = req.path.as_deref();
    if let Some(host) = req.host.as_deref() {
        forget_legacy_marker(&data_dir, host);
    }

    match operation {
        "get" => {
            if let Some(reply) = answer(&req, &data_dir) {
                trace(
                    &data_dir,
                    &format!("get {}: answered from connected account", host_label(&req)),
                );
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                // Ignore write failures: git closing the pipe early is not our
                // problem to report.
                let _ = out.write_all(reply.as_bytes());
                let _ = out.flush();
            } else {
                trace(
                    &data_dir,
                    &format!(
                        "get {}: no usable token{}, forwarding to credential manager",
                        host_label(&req),
                        if req
                            .host
                            .as_deref()
                            .is_some_and(|h| refusal_active(&data_dir, h, path))
                        {
                            " (refused here earlier)"
                        } else {
                            ""
                        }
                    ),
                );
                forward_to_gcm("get", &request);
            }
        }
        "store" => {
            if is_ours(&req, &data_dir) {
                // Our token just worked. A leftover marker describes a refusal
                // that is no longer true.
                if let Some(host) = req.host.as_deref() {
                    clear_refusal(&data_dir, host, path);
                }
                trace(
                    &data_dir,
                    &format!("store {}: our token succeeded, kept out of GCM", host_label(&req)),
                );
            } else if unattended {
                // Whatever GCM answered on `get` it already holds; an
                // unattended run re-saving it buys nothing and this path must
                // never write to the user's store.
                trace(&data_dir, &format!("store {}: dropped (unattended)", host_label(&req)));
            } else {
                trace(&data_dir, &format!("store {}: forwarded to credential manager", host_label(&req)));
                forward_to_gcm("store", &request);
            }
        }
        "erase" => {
            if is_ours(&req, &data_dir) {
                // The host refused our token for this repository. Remember
                // that instead of touching any store: next time this
                // repository goes straight to Credential Manager. Recorded for
                // unattended runs too -- scoped to one repository it cannot
                // switch the account off anywhere else.
                if let Some(host) = req.host.as_deref() {
                    record_refusal(&data_dir, host, path);
                }
                trace(
                    &data_dir,
                    &format!("erase {}: host refused our token, marker recorded", host_label(&req)),
                );
            } else if unattended {
                // The regression this mode exists to prevent: git erases from
                // every helper after any 401, and GitHub sends 401 for a
                // renamed repository even when the sign-in is fine. An
                // unattended run cannot prompt, so all an erase does is punish
                // the next thing the user tries by hand.
                trace(&data_dir, &format!("erase {}: blocked (unattended)", host_label(&req)));
            } else {
                // Attended, and not our credential: a genuinely revoked
                // Credential Manager sign-in should be erased, because the
                // login window replaces it immediately.
                trace(&data_dir, &format!("erase {}: forwarded to credential manager", host_label(&req)));
                forward_to_gcm("erase", &request);
            }
        }
        other => {
            trace(&data_dir, &format!("{other}: ignored"));
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_explicit_flag_enters_helper_mode() {
        assert!(requested(&[
            "gitwyrm.exe".into(),
            HELPER_FLAG.into(),
            "get".into()
        ]));
        // A repo path from the Explorer context menu must open the app.
        assert!(!requested(&["gitwyrm.exe".into(), "C:/code/get".into()]));
        assert!(!requested(&["gitwyrm.exe".into()]));
    }

    #[test]
    fn the_operation_skips_our_own_flags() {
        assert_eq!(
            operation(&["gitwyrm.exe".into(), HELPER_FLAG.into(), "get".into()]),
            "get"
        );
        assert_eq!(
            operation(&[
                "gitwyrm.exe".into(),
                HELPER_FLAG.into(),
                UNATTENDED_FLAG.into(),
                "erase".into()
            ]),
            "erase"
        );
        assert_eq!(operation(&["gitwyrm.exe".into(), HELPER_FLAG.into()]), "");
    }

    #[test]
    fn unattended_is_its_own_flag() {
        assert!(is_unattended(&[
            "gitwyrm.exe".into(),
            HELPER_FLAG.into(),
            UNATTENDED_FLAG.into(),
            "get".into()
        ]));
        assert!(!is_unattended(&[
            "gitwyrm.exe".into(),
            HELPER_FLAG.into(),
            "get".into()
        ]));
    }

    #[test]
    fn maps_public_hosts_and_ignores_unknown_ones() {
        assert_eq!(provider_for_host("github.com"), Some(ProviderId::Github));
        assert_eq!(provider_for_host("github.com:443"), Some(ProviderId::Github));
        assert_eq!(provider_for_host("GitHub.com"), Some(ProviderId::Github));
        assert_eq!(provider_for_host("gitlab.com"), Some(ProviderId::Gitlab));
        assert_eq!(provider_for_host("bitbucket.org"), Some(ProviderId::Bitbucket));
        assert_eq!(provider_for_host("dev.azure.com"), Some(ProviderId::AzureDevops));
        // Enterprise installs have arbitrary hosts; guessing would send a
        // token to an unrelated server.
        assert_eq!(provider_for_host("github.mycorp.com"), None);
        assert_eq!(provider_for_host("example.com"), None);
    }

    #[test]
    fn each_host_gets_the_username_it_expects() {
        assert_eq!(username_for(ProviderId::Github, None), "x-access-token");
        assert_eq!(username_for(ProviderId::Gitlab, None), "oauth2");
        assert_eq!(
            username_for(ProviderId::Bitbucket, Some("me@example.com")),
            "me@example.com"
        );
        assert_eq!(username_for(ProviderId::Bitbucket, None), "x-token-auth");
        assert_eq!(username_for(ProviderId::AzureDevops, None), "pat");
    }

    #[test]
    fn parses_the_fields_it_acts_on_and_stops_at_the_blank_line() {
        let req = parse_request("protocol=https\nhost=github.com\npassword=tok\n\nusername=late\n");
        assert_eq!(req.protocol.as_deref(), Some("https"));
        assert_eq!(req.host.as_deref(), Some("github.com"));
        assert_eq!(req.password.as_deref(), Some("tok"));
    }

    #[test]
    fn parses_the_repository_path_and_ignores_an_empty_one() {
        let req = parse_request("protocol=https\nhost=github.com\npath=me/app.git\n\n");
        assert_eq!(req.path.as_deref(), Some("me/app.git"));
        let req = parse_request("protocol=https\nhost=github.com\npath=\n\n");
        assert_eq!(req.path, None);
    }

    #[test]
    fn marker_names_are_filename_safe_and_ignore_ports_and_case() {
        let dir = Path::new("/data");
        assert_eq!(
            marker_path(dir, "github.com:443", Some("Me/App.git")),
            marker_path(dir, "GitHub.com", Some("me/app"))
        );
        let path = marker_path(dir, "weird/host?name", Some("a/b?c/d.git"));
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '~')),
            "{name}"
        );
        assert_ne!(
            marker_path(dir, "github.com", Some("a-b/c")),
            marker_path(dir, "github.com", Some("a/b-c")),
            "two repositories must never share a marker"
        );
    }

    #[test]
    fn credential_manager_never_sees_the_repository_path() {
        let block = b"protocol=https\nhost=github.com\npath=me/app.git\nusername=me\n\n";
        assert_eq!(
            without_path(block),
            b"protocol=https\nhost=github.com\nusername=me\n\n".to_vec()
        );
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gitwyrm-cred-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("auth.json"), b"{}").unwrap();
        dir
    }

    /// The report this scoping exists for: one repository refusing the token
    /// must leave every other repository on the host using it.
    #[test]
    fn a_refusal_in_one_repository_leaves_the_others_alone() {
        let dir = scratch_dir("scope");
        record_refusal(&dir, "github.com", Some("org/locked.git"));

        assert!(refusal_active(&dir, "github.com", Some("org/locked.git")));
        assert!(!refusal_active(&dir, "github.com", Some("me/app.git")));
        assert!(!refusal_active(&dir, "github.com", None));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A host-wide marker from an earlier version is removed, so a user stuck
    /// with one gets the connected account back after updating.
    #[test]
    fn an_old_host_wide_marker_is_forgotten() {
        let dir = scratch_dir("legacy");
        std::fs::write(legacy_marker_path(&dir, "github.com"), b"old").unwrap();

        forget_legacy_marker(&dir, "github.com:443");

        assert!(!legacy_marker_path(&dir, "github.com").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The refusal cycle end to end: no marker means the token is offered, a
    /// recorded refusal silences it, and rewriting `auth.json` (reconnecting
    /// the account) makes it eligible again.
    #[test]
    fn a_refusal_holds_until_the_account_is_reconnected() {
        let dir = scratch_dir("cycle");
        let repo = Some("me/app.git");

        assert!(!refusal_active(&dir, "github.com", repo));
        record_refusal(&dir, "github.com", repo);
        assert!(refusal_active(&dir, "github.com", repo));

        // Reconnecting rewrites auth.json with a fresh mtime. Filesystem
        // timestamps can be coarse, so nudge past the marker's.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("auth.json"), b"{ }").unwrap();
        assert!(!refusal_active(&dir, "github.com", repo));

        // And an explicit clear (a successful `store`) removes it outright.
        record_refusal(&dir, "github.com", repo);
        clear_refusal(&dir, "github.com", repo);
        assert!(!refusal_active(&dir, "github.com", repo));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
