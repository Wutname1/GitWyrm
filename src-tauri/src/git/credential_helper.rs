//! A git credential helper backed by the account the user connected in-app.
//!
//! # Why this exists
//!
//! Connecting GitHub in Settings stored a token that only the *API* ever used --
//! pull requests, issues, the repo list. Push and fetch shell out to git, which
//! hands authentication to Git Credential Manager, a separate process that has
//! never heard of that token. So the app asked the user to sign in a second
//! time, through a different window, for the same account they had just
//! connected. Two reports said as much directly: "I thought that integration
//! with github was the connection? Is that wrong?" and, from a user whose `gh`
//! token was valid the whole time, "why am i not getting prompted? Why can i
//! use other clients?".
//!
//! With this helper installed, git asks *us* first. When we hold a token for
//! the host, git gets it and never reaches GCM.
//!
//! # Why a subprocess rather than a header or an env var
//!
//! The token must reach git somehow, and every other route leaks it:
//!
//!   - `-c http.extraheader=Authorization: ...` puts the credential in the
//!     process argument list, which any other process on the machine can read,
//!     and in `GIT_TRACE` output.
//!   - `GIT_ASKPASS` pointing at a shim needs the token in the child's
//!     environment, which is likewise readable, and leaves a script on disk.
//!
//! The credential-helper protocol hands the value over a pipe git opens itself:
//! stdout, into a process git spawned. It appears in no argument list, no
//! environment block, and no file. That is the whole reason this is a separate
//! executable mode rather than three lines in `run_streaming_with`.
//!
//! # The protocol
//!
//! Git runs `<helper> get`, writes `key=value` lines describing the request,
//! and closes stdin with a blank line. The helper replies with its own
//! `key=value` lines. Answering nothing is legal and means "I don't have one",
//! at which point git moves on to the next helper -- which is how a host we
//! have no token for still reaches GCM normally.
//!
//! `store` and `erase` are accepted and deliberately do nothing: our token's
//! lifecycle belongs to the Settings screen, and letting git delete it because
//! a push was rejected would sign the user out of the app as a side effect.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::hosting::registry::ProviderId;

/// The argv token that puts the binary in helper mode.
///
/// Deliberately not a bare word like `get`: this is the *application* binary,
/// and a repo folder or file argument must never be mistaken for a request to
/// dump a credential to stdout.
pub const HELPER_FLAG: &str = "--credential-helper";

/// Whether this process was launched by git to answer a credential request.
///
/// Checked before Tauri initializes: a helper invocation must not build a
/// window, register plugins, or trip the single-instance guard and hand its
/// arguments to the running app. It reads a file, prints a line, and exits.
pub fn requested(args: &[String]) -> bool {
    args.iter().any(|a| a == HELPER_FLAG)
}

/// The `auth.json` provider whose token authenticates `host`.
///
/// Matched on the hostname git gives us rather than a remote URL, because that
/// is all the protocol supplies. Enterprise and self-hosted installs are not
/// matched: their host names are arbitrary, and guessing wrong would send a
/// GitHub token to an unrelated server. Those fall through to GCM as before.
fn provider_for_host(host: &str) -> Option<ProviderId> {
    // Git may include a port ("github.com:443"); compare on the name alone.
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
        // this spelling is the one its own documentation uses.
        ProviderId::Github => "x-access-token".to_string(),
        ProviderId::Gitlab => "oauth2".to_string(),
        // Bitbucket app passwords authenticate against the account email, which
        // is why `StoredCredential` carries one for this host and no other.
        ProviderId::Bitbucket => stored_email.unwrap_or("x-token-auth").to_string(),
        ProviderId::AzureDevops => "pat".to_string(),
    }
}

/// Parse the `key=value` block git writes to stdin.
///
/// Only `host` is used today. `protocol` is read to refuse anything that is not
/// HTTP(S): an SSH remote authenticates with a key and has no business
/// receiving a bearer token.
fn read_request(input: &mut impl BufRead) -> (Option<String>, Option<String>) {
    let (mut protocol, mut host) = (None, None);
    for line in input.lines().map_while(Result::ok) {
        let line = line.trim_end();
        // A blank line terminates the request block.
        if line.is_empty() {
            break;
        }
        match line.split_once('=') {
            Some(("protocol", v)) => protocol = Some(v.to_string()),
            Some(("host", v)) => host = Some(v.to_string()),
            _ => {}
        }
    }
    (protocol, host)
}

/// The reply for a request, or None to stay silent and let git try GCM.
///
/// Split from [`run`] so the decision is testable without a real app data
/// directory or a live stdin.
fn answer(protocol: Option<&str>, host: Option<&str>, data_dir: &std::path::Path) -> Option<String> {
    // An SSH remote must never receive a token.
    if !matches!(protocol, Some("https") | Some("http")) {
        return None;
    }
    let provider = provider_for_host(host?)?;
    let stored = crate::hosting::http::credential_from_dir(data_dir, provider).ok()??;
    if stored.token.is_empty() {
        return None;
    }
    // A self-hosted install stores its own base URL. The host git asked about is
    // one of the public ones (provider_for_host matched it), so a credential
    // scoped elsewhere is not ours to hand over.
    if stored.base_url.is_some_and(|u| !u.trim().is_empty()) {
        return None;
    }
    let username = username_for(provider, stored.email.as_deref());
    Some(format!(
        "username={username}\npassword={}\n",
        stored.token
    ))
}

/// Answer one credential request, then exit.
///
/// Returns the process exit code. Always 0: a helper that fails is expected to
/// say nothing and let git continue to the next one, and a non-zero exit would
/// turn "no token stored" into a hard push failure.
pub fn run(args: &[String], data_dir: PathBuf) -> i32 {
    // The operation is the first argument that is not our flag. `store` and
    // `erase` are accepted silently; only `get` produces output.
    let operation = args
        .iter()
        .skip(1)
        .find(|a| *a != HELPER_FLAG)
        .map(String::as_str)
        .unwrap_or("");
    if operation != "get" {
        return 0;
    }

    let stdin = std::io::stdin();
    let (protocol, host) = read_request(&mut stdin.lock());

    if let Some(reply) = answer(protocol.as_deref(), host.as_deref(), &data_dir) {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        // Ignore write failures: git closing the pipe early is not our problem
        // to report, and there is no log to report it to in this mode.
        let _ = out.write_all(reply.as_bytes());
        let _ = out.flush();
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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
    fn reads_the_request_block_and_stops_at_the_blank_line() {
        let mut input = Cursor::new("protocol=https\nhost=github.com\n\nusername=ignored\n");
        let (protocol, host) = read_request(&mut input);
        assert_eq!(protocol.as_deref(), Some("https"));
        assert_eq!(host.as_deref(), Some("github.com"));
    }

    #[test]
    fn maps_public_hosts_and_ignores_unknown_ones() {
        assert_eq!(provider_for_host("github.com"), Some(ProviderId::Github));
        // Git may append the port.
        assert_eq!(provider_for_host("github.com:443"), Some(ProviderId::Github));
        assert_eq!(provider_for_host("GitHub.com"), Some(ProviderId::Github));
        assert_eq!(provider_for_host("gitlab.com"), Some(ProviderId::Gitlab));
        // A self-hosted host name could be anything; guessing would send a
        // GitHub token to a stranger's server.
        assert_eq!(provider_for_host("git.internal.example"), None);
    }

    /// The username field is not cosmetic: the wrong one reads as a bad token.
    #[test]
    fn each_host_gets_the_username_it_expects() {
        assert_eq!(username_for(ProviderId::Github, None), "x-access-token");
        assert_eq!(username_for(ProviderId::Gitlab, None), "oauth2");
        assert_eq!(
            username_for(ProviderId::Bitbucket, Some("me@example.com")),
            "me@example.com"
        );
    }

    /// An SSH remote authenticates with a key. Handing it a bearer token would
    /// be leaking the credential to a transport that never asked for one.
    #[test]
    fn ssh_requests_are_never_answered() {
        let dir = std::env::temp_dir();
        assert!(answer(Some("ssh"), Some("github.com"), &dir).is_none());
        assert!(answer(None, Some("github.com"), &dir).is_none());
    }

    /// No token stored means silence, so git falls through to Credential
    /// Manager exactly as it did before this helper existed.
    #[test]
    fn an_empty_store_stays_silent() {
        let dir = std::env::temp_dir().join("gitwyrm-helper-empty-store");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::remove_file(dir.join("auth.json"));
        assert!(answer(Some("https"), Some("github.com"), &dir).is_none());
    }

    /// The end-to-end shape git actually consumes.
    #[test]
    fn a_stored_token_is_returned_in_protocol_form() {
        let dir = std::env::temp_dir().join("gitwyrm-helper-token");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("auth.json"),
            r#"{"github":{"type":"api","key":"gho_exampletoken"}}"#,
        )
        .unwrap();

        let reply = answer(Some("https"), Some("github.com"), &dir).expect("should answer");
        assert_eq!(reply, "username=x-access-token\npassword=gho_exampletoken\n");
        // Git requires the trailing newline to close the block.
        assert!(reply.ends_with('\n'));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A host we hold no token for must not borrow another host's.
    #[test]
    fn a_token_for_one_host_is_not_offered_to_another() {
        let dir = std::env::temp_dir().join("gitwyrm-helper-crosshost");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("auth.json"),
            r#"{"github":{"type":"api","key":"gho_exampletoken"}}"#,
        )
        .unwrap();

        assert!(answer(Some("https"), Some("gitlab.com"), &dir).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
