//! HTTP plumbing shared by every host provider.
//!
//! Each host has its own auth header and its own way of phrasing an error in a
//! JSON body, but the surrounding work -- read the stored credential, build a
//! request, turn a non-2xx into something a human can act on -- is the same
//! four lines everywhere. Doing it once means a new provider inherits sane
//! error messages instead of reinventing them.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;

use crate::ai::auth;
use crate::error::AppError;
use crate::hosting::registry::ProviderId;

pub const TIMEOUT: Duration = Duration::from_secs(30);

/// A stored credential, already split into the parts a provider needs.
pub struct StoredCredential {
    pub token: String,
    /// The Bitbucket account email; None for every other host.
    pub email: Option<String>,
    /// Self-hosted base URL, when the user gave one.
    pub base_url: Option<String>,
}

/// Reads the credential for a provider, or None when nothing is stored.
///
/// The email and base URL ride along inside the token string rather than
/// needing a new `auth.json` shape: `AuthInfo::Api` has one field, and adding
/// variants to a file that already holds users' AI credentials risks breaking
/// deserialization of the whole file for a change only this module needs.
/// Format is `v1\nemail\nbase_url\ntoken`, with a bare token treated as the
/// legacy single-value form so GitHub's existing entries keep working.
pub fn credential(
    app: &tauri::AppHandle,
    provider: ProviderId,
) -> Result<Option<StoredCredential>, AppError> {
    let Some(info) = auth::get(app, provider.as_str())? else {
        return Ok(None);
    };
    let raw = match info {
        auth::AuthInfo::Api { key } => key,
        auth::AuthInfo::Oauth { access, .. } => access,
    };
    Ok(Some(parse_credential(&raw)))
}

/// Packs the parts into the single string `auth.json` stores.
pub fn pack_credential(token: &str, email: Option<&str>, base_url: Option<&str>) -> String {
    if email.is_none() && base_url.is_none() {
        return token.to_string();
    }
    format!(
        "v1\n{}\n{}\n{}",
        email.unwrap_or_default(),
        base_url.unwrap_or_default(),
        token
    )
}

fn parse_credential(raw: &str) -> StoredCredential {
    let mut lines = raw.split('\n');
    if lines.next() != Some("v1") {
        return StoredCredential {
            token: raw.to_string(),
            email: None,
            base_url: None,
        };
    }
    let email = lines.next().unwrap_or_default();
    let base_url = lines.next().unwrap_or_default();
    // The token is whatever remains, rejoined: tokens do not contain newlines,
    // but rejoining means a stray one cannot silently truncate the credential.
    let token: String = lines.collect::<Vec<_>>().join("\n");
    StoredCredential {
        token,
        email: non_empty(email),
        base_url: non_empty(base_url),
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub fn client() -> reqwest::Client {
    reqwest::Client::new()
}

/// Turns a non-2xx response into a readable error.
///
/// `message_keys` are the JSON fields this host puts its human-readable error
/// in, tried in order -- every host picked a different name, and falling back to
/// a truncated body beats showing a bare status code.
pub async fn check(
    res: reqwest::Response,
    host: &str,
    message_keys: &[&str],
) -> Result<reqwest::Response, AppError> {
    let status = res.status();
    if status.is_success() {
        return Ok(res);
    }
    let body = res.text().await.unwrap_or_default();
    let message = extract_message(&body, message_keys)
        .unwrap_or_else(|| body.chars().take(200).collect::<String>());
    Err(AppError::Other(match status.as_u16() {
    401 | 403 if message.to_lowercase().contains("rate limit") => {
      format!("{host} rate limit reached; try again in a few minutes")
    }
    401 => format!("{host} sign-in is no longer valid; connect {host} again"),
    403 => format!("{host} refused: {message}. Check the token has the permissions {host} needs."),
    404 => format!("{host} could not find that. It may be private, renamed, or your token may not cover it."),
    _ => format!("{host} said: {message}"),
  }))
}

/// Digs the error text out of a host's JSON body.
///
/// Handles the three shapes seen in practice: a plain string at the key, a
/// nested `{"message": ...}` object (Bitbucket's `error.message`), and an array
/// of strings (GitLab's `message` on validation failures).
fn extract_message(body: &str, keys: &[&str]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    for key in keys {
        let Some(found) = value.get(key) else {
            continue;
        };
        if let Some(text) = found.as_str() {
            return Some(text.to_string());
        }
        if let Some(nested) = found.get("message").and_then(|m| m.as_str()) {
            return Some(nested.to_string());
        }
        if let Some(list) = found.as_array() {
            let joined: Vec<String> = list
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if !joined.is_empty() {
                return Some(joined.join("; "));
            }
        }
    }
    None
}

/// How long a host stays on the bench after telling us we may not ask.
///
/// An org that has turned off third-party access, or a token that no longer
/// carries the right scopes, is not a condition that clears on its own within a
/// session -- it clears when an admin changes a setting or the user reconnects.
/// Retrying every 60s in the meantime produces nothing but log noise and
/// rate-limit pressure, so the answer is remembered for an hour and, because
/// the cache lives in memory, forgotten on relaunch.
const PERMISSION_COOLDOWN: Duration = Duration::from_secs(60 * 60);

/// The cooldown key for one refusal: the host, narrowed to the repository the
/// refused request was for.
///
/// Keying on the host alone was wrong, and wrong in a way that grew teeth once
/// the `gh` fallback existed. `is_permission_refusal` matches any `refused:`
/// message, so a single private repository the token cannot see -- an ordinary,
/// permanent condition in any account with a few orgs -- benched the string
/// `"GitHub"` for an hour. From then on every GitHub request in the workspace,
/// for every repo, skipped HTTP and went out through two `gh` process spawns.
/// The user's report was that the whole app got slow after enabling the CLI,
/// and this is why.
///
/// The scope is the repository *and* the kind of resource asked for, because
/// the refusal this exists for is narrower than a repository. Under OAuth app
/// restrictions an org blocks issues and pull requests while the same token
/// keeps working for the rest of that very repo -- contents, commits, checks.
/// That is the case the CLI fallback was added for. Keyed by repo alone, one
/// refused PR list would route that repo's every other call through `gh` too,
/// each paying a process spawn to replace an HTTP call that was working.
///
/// Requests with no repository in their path -- `/user`, rate-limit probes --
/// fall back to the bare host, which is the old behaviour for the small set of
/// calls that really are account-wide.
fn cooldown_key(host: &str, path: &str) -> String {
    match repo_scope(path) {
        Some(scope) => format!("{host}:{scope}"),
        None => host.to_string(),
    }
}

/// The `owner/repo` plus resource kind a GitHub API path addresses.
///
/// Paths look like `/repos/{owner}/{repo}/{kind}/...`, with or without a
/// leading slash, and may carry a query string that must not become part of the
/// key.
///
/// `issues` and `pulls` deliberately collapse to one bucket. They are the pair
/// an org's OAuth app restrictions block together, and a PR *is* an issue in
/// this API -- PR comments are fetched from `/issues/{n}/comments`. Splitting
/// them would make a blocked PR list re-learn the same refusal when the comment
/// call followed it a moment later.
fn repo_scope(path: &str) -> Option<String> {
    let rest = path.trim_start_matches('/').strip_prefix("repos/")?;
    let rest = rest.split(['?', '#']).next()?;
    let mut parts = rest.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    let kind = match parts.next() {
        Some("issues") | Some("pulls") => "issues+pulls",
        // The repo endpoint itself, e.g. `/repos/o/r`.
        Some(other) => other,
        None => "",
    };
    Some(format!("{owner}/{repo}/{kind}"))
}

/// Remembered permission refusals, keyed by [`cooldown_key`].
///
/// Deliberately not a `OnceLock`: reconnecting an account is exactly what a
/// user does after reading the refusal, and a permanent cache would keep
/// refusing until they restarted the app. [`clear_cooldown`] handles that case
/// directly; the TTL is the backstop for an admin-side fix we never see.
static PERMISSION_COOLDOWNS: Mutex<Option<HashMap<String, (Instant, String)>>> = Mutex::new(None);

/// True when the host refused us for a reason no retry can change.
///
/// Narrow on purpose. A timeout, a 500, or a rate limit are all worth trying
/// again shortly; only an authorization decision earns an hour of silence.
fn is_permission_refusal(message: &str) -> bool {
    let low = message.to_lowercase();
    // Rate limits recover on their own and must not be benched for an hour.
    if low.contains("rate limit") {
        return false;
    }
    low.contains("oauth app access restrictions")
        || low.contains("sign-in is no longer valid")
        || low.contains("refused:")
}

/// The remembered refusal for `key`, if one is still within its cooldown.
///
/// `key` comes from [`cooldown_key`], so it names a repository's resource kind
/// where the path had one and the bare host where it did not.
fn cooled_down(key: &str) -> Option<String> {
    PERMISSION_COOLDOWNS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()?
        .get(key)
        .filter(|(at, _)| at.elapsed() < PERMISSION_COOLDOWN)
        .map(|(_, message)| message.clone())
}

fn remember_refusal(host: &str, key: &str, message: &str) {
    log::warn!("{host} refused on permissions for {key}; not asking again for an hour: {message}");
    PERMISSION_COOLDOWNS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key.to_string(), (Instant::now(), message.to_string()));
}

/// Drops a host's cooldown so the next call goes out for real.
///
/// Called when the user connects or signs out of an account: they have just
/// acted on the refusal, so making them wait out the rest of the hour would be
/// the app ignoring what they did.
///
/// Takes the display name rather than a [`ProviderId`] because that is the key
/// `send` stores under -- `ProviderId::as_str` is the `auth.json` key
/// ("github"), which would silently match nothing here ("GitHub").
pub fn clear_cooldown(host: &str) {
    if let Some(map) = PERMISSION_COOLDOWNS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
    {
        map.remove(host);
    }
}

/// Enough about a GitHub request to reissue it through the `gh` CLI.
///
/// A built `reqwest::RequestBuilder` has already swallowed its method and path
/// into an opaque object, so the fallback cannot recover them from it. Carrying
/// them alongside is the smallest thing that works; only GitHub calls build one,
/// and the other three hosts keep using plain [`send`] untouched.
#[derive(Clone)]
pub struct GhFallback {
    pub method: &'static str,
    pub path: String,
    pub body: Option<serde_json::Value>,
}

impl GhFallback {
    pub fn get(path: impl AsRef<str>) -> Self {
        Self {
            method: "GET",
            path: path.as_ref().to_string(),
            body: None,
        }
    }

    pub fn write(method: &'static str, path: impl AsRef<str>, body: serde_json::Value) -> Self {
        Self {
            method,
            path: path.as_ref().to_string(),
            body: Some(body),
        }
    }
}

/// Whether the GitHub CLI fallback may be used, set from Settings at startup.
///
/// A process-global for the same reason `git::shell::GIT_PROGRAM` is one: the
/// providers are deep in a call chain that would otherwise have to thread the
/// setting through every method for a value that changes about once a year.
static GH_FALLBACK_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

pub fn set_gh_fallback_enabled(enabled: bool) {
    GH_FALLBACK_ENABLED.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

pub fn gh_fallback_enabled() -> bool {
    GH_FALLBACK_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Send a GitHub request, retrying through the `gh` CLI if the token is refused.
///
/// The refusal this exists for is an organization blocking third-party OAuth
/// apps. That is not something the user can fix from inside GitWyrm, but `gh`
/// is usually approved where we are not, so the retry turns a dead panel into a
/// working one. Everything else -- a rate limit, a timeout, a 404 -- is left
/// exactly as it was, because `gh` would fail the same way.
///
/// A successful fallback deliberately does NOT record a cooldown: the request
/// worked, and benching the host would stop the next one from even trying.
pub async fn send_via_gh(
    builder: reqwest::RequestBuilder,
    host: &str,
    message_keys: &[&str],
    fallback: GhFallback,
) -> Result<reqwest::Response, AppError> {
    // Scoped to the resource this request is for, so a blocked PR list benches
    // pull requests on that repo and nothing else.
    let key = cooldown_key(host, &fallback.path);
    // A remembered refusal is the signal to go straight to `gh`: the direct
    // call is known to fail, so spending a round trip to confirm it is waste.
    if let Some(remembered) = cooled_down(&key) {
        return match try_gh(&fallback).await {
            Some(Ok(res)) => Ok(res),
            Some(Err(e)) => Err(e),
            None => Err(AppError::Other(remembered)),
        };
    }

    // `send_raw`, not `send`: the refusal is recorded here against the scoped
    // key, and letting `send` also record it under the bare host would re-create
    // the workspace-wide bench this scoping exists to remove.
    let direct = send_raw(builder, host, message_keys).await;
    let Err(AppError::Other(message)) = &direct else {
        return direct;
    };
    if !is_permission_refusal(message) {
        return direct;
    }
    remember_refusal(host, &key, message);
    match try_gh(&fallback).await {
        Some(Ok(res)) => Ok(res),
        // `gh` was available and still failed. Its message is the more specific
        // of the two -- it reached the API with a better credential and was told
        // no anyway -- so it replaces ours rather than being appended.
        Some(Err(e)) => Err(e),
        None => direct,
    }
}

/// Runs the fallback, or None when the CLI cannot help.
///
/// Returns a synthesized `reqwest::Response` so callers deserialize the body
/// the same way regardless of which transport produced it.
/// Runs [`try_gh_blocking`] on the blocking pool.
///
/// Every caller is async, and the work inside is a PATH walk plus up to two
/// process spawns that block for as long as [`gh_cli`] allows. Doing that
/// inline on an async task holds a runtime thread for the whole wait, and the
/// git commands the UI runs -- `checkout_branch` and friends -- queue on that
/// same pool. A GitHub panel falling back on several requests at once was
/// enough to make switching branches feel stalled, which is how this surfaced.
///
/// [`gh_cli`]: super::gh_cli
async fn try_gh(fallback: &GhFallback) -> Option<Result<reqwest::Response, AppError>> {
    let fallback = fallback.clone();
    match tauri::async_runtime::spawn_blocking(move || try_gh_blocking(&fallback)).await {
        Ok(result) => result,
        // The pool itself failed, which is not something the fallback can
        // report usefully; leave the caller with its original error.
        Err(e) => {
            log::debug!("GitHub CLI fallback could not be scheduled: {e}");
            None
        }
    }
}

fn try_gh_blocking(fallback: &GhFallback) -> Option<Result<reqwest::Response, AppError>> {
    if !gh_fallback_enabled() {
        return None;
    }
    let exe = match super::gh_cli::availability() {
        Ok(exe) => exe,
        Err(reason) => {
            log::debug!("GitHub CLI fallback unavailable: {reason:?}");
            return None;
        }
    };
    log::info!(
        "GitHub refused our token; retrying {} {} through the GitHub CLI",
        fallback.method,
        fallback.path
    );
    match super::gh_cli::api(
        &exe,
        fallback.method,
        &fallback.path,
        fallback.body.as_ref(),
    ) {
        Ok(body) => {
            // A 204 has no body, and `serde_json` cannot parse an empty string.
            // The write paths only check status, so an empty object satisfies
            // both them and any caller that does deserialize.
            let body = if body.trim().is_empty() {
                "{}".to_string()
            } else {
                body
            };
            Some(Ok(http_response_from_body(body)))
        }
        Err(e) => Some(Err(e)),
    }
}

/// Wraps a body string as a 200 response, so the `gh` path returns the same
/// type as the HTTP path and every caller downstream stays unchanged.
fn http_response_from_body(body: String) -> reqwest::Response {
    reqwest::Response::from(
        http::Response::builder()
            .status(200)
            .body(body)
            .expect("a 200 with a string body cannot fail to build"),
    )
}

pub async fn send(
    builder: reqwest::RequestBuilder,
    host: &str,
    message_keys: &[&str],
) -> Result<reqwest::Response, AppError> {
    // A host that already told us no is not asked again until the cooldown ends.
    // Repo tabs, PR lists and issue counts all fan out to the same host, so
    // without this one refusal became dozens of identical failing requests.
    if let Some(remembered) = cooled_down(host) {
        return Err(AppError::Other(remembered));
    }
    let checked = send_raw(builder, host, message_keys).await;
    if let Err(AppError::Other(message)) = &checked {
        if is_permission_refusal(message) {
            remember_refusal(host, host, message);
        }
    }
    checked
}

/// Send and check, without touching the cooldown map.
///
/// For callers that own their own cooldown scope -- [`send_via_gh`] keys on the
/// repository and resource kind, which this function cannot see.
async fn send_raw(
    builder: reqwest::RequestBuilder,
    host: &str,
    message_keys: &[&str],
) -> Result<reqwest::Response, AppError> {
    let res = builder
        .send()
        .await
        .map_err(|e| AppError::Other(format!("could not reach {host}: {e}")))?;
    check(res, host, message_keys).await
}

/// Retry an already-failed GitHub request through the `gh` CLI.
///
/// For the call sites that inspect the response themselves (`list_prs` treats a
/// 404 as "no pull requests") and so cannot hand [`send_via_gh`] an unsent
/// builder. Takes the error they produced and either replaces it with the
/// fallback's answer or gives it back unchanged.
pub async fn retry_via_gh<T: DeserializeOwned>(
    error: AppError,
    host: &str,
    fallback: GhFallback,
) -> Result<T, AppError> {
    let AppError::Other(message) = &error else {
        return Err(error);
    };
    if !is_permission_refusal(message) {
        return Err(error);
    }
    // The caller reached the API itself, so the refusal has not been recorded
    // yet; record it against this resource before falling back.
    remember_refusal(host, &cooldown_key(host, &fallback.path), message);
    match try_gh(&fallback).await {
        Some(Ok(res)) => res
            .json()
            .await
            .map_err(|e| AppError::Other(format!("bad response from {host}: {e}"))),
        Some(Err(e)) => Err(e),
        None => Err(error),
    }
}

/// Send and deserialize through the `gh`-fallback path.
pub async fn send_json_via_gh<T: DeserializeOwned>(
    builder: reqwest::RequestBuilder,
    host: &str,
    message_keys: &[&str],
    fallback: GhFallback,
) -> Result<T, AppError> {
    send_via_gh(builder, host, message_keys, fallback)
        .await?
        .json()
        .await
        .map_err(|e| AppError::Other(format!("bad response from {host}: {e}")))
}

/// Send and deserialize, with the host named in any parse failure.
pub async fn send_json<T: DeserializeOwned>(
    builder: reqwest::RequestBuilder,
    host: &str,
    message_keys: &[&str],
) -> Result<T, AppError> {
    send(builder, host, message_keys)
        .await?
        .json()
        .await
        .map_err(|e| AppError::Other(format!("bad response from {host}: {e}")))
}

/// Percent-encodes a path segment, escaping `/` as well.
///
/// GitLab identifies a project by its URL-encoded full path, where every
/// separator must become `%2F`. Most encoding sets treat `/` as safe and leave
/// it alone, which yields a 404 that looks like a missing project rather than a
/// malformed URL.
pub fn encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusals worth an hour of silence: an admin or the user has to act,
    /// and until they do the answer cannot change.
    #[test]
    fn permission_refusals_earn_a_cooldown() {
        assert!(is_permission_refusal(
      "GitHub refused: Although you appear to have the correct authorization credentials, the `some-org` organization has enabled OAuth App access restrictions."
    ));
        assert!(is_permission_refusal(
      "GitHub said: Although you appear to have the correct authorization credentials, the `some-org` organization has enabled OAuth App access restrictions."
    ));
        assert!(is_permission_refusal(
            "GitHub sign-in is no longer valid; connect GitHub again"
        ));
    }

    /// Anything that recovers on its own must keep being retried. Benching a
    /// rate limit for an hour would turn a few minutes' wait into a dead session.
    #[test]
    fn transient_failures_are_not_benched() {
        assert!(!is_permission_refusal(
            "GitHub rate limit reached; try again in a few minutes"
        ));
        assert!(!is_permission_refusal("could not reach GitHub: timed out"));
        assert!(!is_permission_refusal(
            "bad response from GitHub: expected value"
        ));
        assert!(!is_permission_refusal(
      "GitHub could not find that. It may be private, renamed, or your token may not cover it."
    ));
        // A merge conflict is the host declining an action, not our access to it.
        assert!(!is_permission_refusal(
            "GitHub said: Pull Request has merge conflicts"
        ));
    }

    /// The bug this scoping fixes: a work org blocks issues and pull requests
    /// on its repos while every other call on the very same repo keeps working.
    /// Keyed by host alone, one refused PR list sent the entire workspace --
    /// every repo, every endpoint -- out through the `gh` CLI for an hour.
    #[test]
    fn a_refusal_is_scoped_to_one_repos_issues_and_pulls() {
        let host = "ScopeTestHost";
        let prs = cooldown_key(host, "/repos/work-org/api/pulls?per_page=50");
        let issues = cooldown_key(host, "/repos/work-org/api/issues/12/comments");
        let commits = cooldown_key(host, "/repos/work-org/api/commits");
        let other_repo = cooldown_key(host, "/repos/work-org/other/pulls");

        // Pull requests and issues share a bucket: a PR *is* an issue here.
        assert_eq!(prs, issues, "issues and pulls must share one bucket");

        remember_refusal(
            host,
            &prs,
            "ScopeTestHost refused: OAuth App access restrictions",
        );
        assert!(
            cooled_down(&prs).is_some(),
            "the refused resource is benched"
        );
        assert!(
            cooled_down(&commits).is_none(),
            "contents on the same repo must keep using the working HTTP path"
        );
        assert!(
            cooled_down(&other_repo).is_none(),
            "a different repo must be unaffected"
        );
        assert!(
            cooled_down(host).is_none(),
            "the bare host must not be benched by one repo's refusal"
        );
        clear_cooldown(&prs);
    }

    /// Account-wide calls carry no repository, so they keep the old host key.
    #[test]
    fn pathless_calls_fall_back_to_the_bare_host() {
        assert_eq!(cooldown_key("GitHub", "/user"), "GitHub");
        assert_eq!(cooldown_key("GitHub", "/rate_limit"), "GitHub");
        // A malformed repo path must not panic or invent a scope.
        assert_eq!(cooldown_key("GitHub", "/repos/only-owner"), "GitHub");
        assert_eq!(
            cooldown_key("GitHub", "repos/o/r/pulls"),
            "GitHub:o/r/issues+pulls",
            "a missing leading slash must key the same as a present one"
        );
    }

    /// Remembering must be per-host: one org's restriction cannot silence a
    /// different host the user is legitimately connected to.
    #[test]
    fn a_cooldown_is_scoped_to_its_host_and_cleared_on_reconnect() {
        // Names unique to this test: the cache is process-global.
        let host = "CooldownTestHost";
        let other = "CooldownOtherHost";
        assert!(cooled_down(host).is_none(), "starts clean");

        remember_refusal(host, host, "TestHost refused: no access");
        assert_eq!(
            cooled_down(host).as_deref(),
            Some("TestHost refused: no access")
        );
        assert!(
            cooled_down(other).is_none(),
            "a refusal must not spread to other hosts"
        );

        clear_cooldown(host);
        assert!(
            cooled_down(host).is_none(),
            "reconnecting must lift the bench immediately"
        );
    }

    /// The exact set of failures that reroute to `gh`.
    ///
    /// `send_via_gh` gates on `is_permission_refusal`, so this list IS the
    /// fallback's trigger condition. Rerouting too widely would spawn a process
    /// on every rate limit and timeout; too narrowly and the org-blocked case
    /// this exists for never fires.
    #[test]
    fn only_permission_refusals_reroute_to_the_cli() {
        // The case the fallback exists for.
        assert!(is_permission_refusal(
      "GitHub refused: Although you appear to have the correct authorization credentials, the `some-org` organization has enabled OAuth App access restrictions."
    ));
        // A token that lost its scopes: `gh` has its own and may well succeed.
        assert!(is_permission_refusal(
            "GitHub sign-in is no longer valid; connect GitHub again"
        ));

        // `gh` would hit the same rate limit from the same IP, and spawning a
        // process to be told so again helps nobody.
        assert!(!is_permission_refusal(
            "GitHub rate limit reached; try again in a few minutes"
        ));
        // Offline is offline for both transports.
        assert!(!is_permission_refusal("could not reach GitHub: timed out"));
        // A real 404 is not an access problem; rerouting would turn a clear
        // "renamed or deleted" into a confusing CLI error.
        assert!(!is_permission_refusal(
      "GitHub could not find that. It may be private, renamed, or your token may not cover it."
    ));
    }

    /// The toggle has to actually gate the fallback, not merely be stored.
    #[test]
    fn the_setting_turns_the_fallback_off() {
        // Restored at the end: the flag is process-global and other tests read it.
        let original = gh_fallback_enabled();

        set_gh_fallback_enabled(false);
        assert!(
            // The blocking half: `try_gh` only wraps it in `spawn_blocking`,
            // and the gate under test lives here.
            try_gh_blocking(&GhFallback::get("/repos/o/r/pulls")).is_none(),
            "a disabled fallback must not run the CLI at all"
        );

        set_gh_fallback_enabled(original);
    }

    #[test]
    fn encodes_slashes_for_gitlab_paths() {
        assert_eq!(encode_segment("group/sub/proj"), "group%2Fsub%2Fproj");
        assert_eq!(encode_segment("simple"), "simple");
        // Spaces appear in Azure project names.
        assert_eq!(encode_segment("My Project"), "My%20Project");
    }

    /// A bare token is how GitHub's existing entries are stored; packing must not
    /// disturb them or every connected user is signed out on upgrade.
    #[test]
    fn bare_tokens_round_trip_unchanged() {
        assert_eq!(pack_credential("ghp_abc", None, None), "ghp_abc");
        let parsed = parse_credential("ghp_abc");
        assert_eq!(parsed.token, "ghp_abc");
        assert!(parsed.email.is_none());
        assert!(parsed.base_url.is_none());
    }

    #[test]
    fn packed_credentials_round_trip() {
        let packed = pack_credential("tok", Some("me@example.com"), Some("https://git.co"));
        let parsed = parse_credential(&packed);
        assert_eq!(parsed.token, "tok");
        assert_eq!(parsed.email.as_deref(), Some("me@example.com"));
        assert_eq!(parsed.base_url.as_deref(), Some("https://git.co"));
    }

    #[test]
    fn packs_each_field_independently() {
        let only_url = parse_credential(&pack_credential("t", None, Some("https://h")));
        assert_eq!(only_url.token, "t");
        assert!(only_url.email.is_none());
        assert_eq!(only_url.base_url.as_deref(), Some("https://h"));

        let only_email = parse_credential(&pack_credential("t", Some("a@b.c"), None));
        assert_eq!(only_email.token, "t");
        assert_eq!(only_email.email.as_deref(), Some("a@b.c"));
        assert!(only_email.base_url.is_none());
    }

    #[test]
    fn reads_each_hosts_error_shape() {
        // GitHub / Azure: plain string.
        assert_eq!(
            extract_message(r#"{"message":"Not Found"}"#, &["message"]).as_deref(),
            Some("Not Found")
        );
        // Bitbucket: nested object.
        assert_eq!(
            extract_message(r#"{"error":{"message":"No such repo"}}"#, &["error"]).as_deref(),
            Some("No such repo")
        );
        // GitLab: array of validation strings.
        assert_eq!(
            extract_message(r#"{"message":["a is bad","b is bad"]}"#, &["message"]).as_deref(),
            Some("a is bad; b is bad")
        );
        // GitLab also uses `error` for auth failures.
        assert_eq!(
            extract_message(r#"{"error":"invalid_token"}"#, &["message", "error"]).as_deref(),
            Some("invalid_token")
        );
        assert!(extract_message("not json", &["message"]).is_none());
    }
}
