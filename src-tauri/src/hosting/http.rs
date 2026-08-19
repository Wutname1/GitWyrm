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

/// Remembered permission refusals, keyed by host name.
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

/// The remembered refusal for `host`, if one is still within its cooldown.
fn cooled_down(host: &str) -> Option<String> {
    PERMISSION_COOLDOWNS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()?
        .get(host)
        .filter(|(at, _)| at.elapsed() < PERMISSION_COOLDOWN)
        .map(|(_, message)| message.clone())
}

fn remember_refusal(host: &str, message: &str) {
    log::warn!("{host} refused on permissions; not asking again for an hour: {message}");
    PERMISSION_COOLDOWNS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(host.to_string(), (Instant::now(), message.to_string()));
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
    let res = builder
        .send()
        .await
        .map_err(|e| AppError::Other(format!("could not reach {host}: {e}")))?;
    let checked = check(res, host, message_keys).await;
    if let Err(AppError::Other(message)) = &checked {
        if is_permission_refusal(message) {
            remember_refusal(host, message);
        }
    }
    checked
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

    /// Remembering must be per-host: one org's restriction cannot silence a
    /// different host the user is legitimately connected to.
    #[test]
    fn a_cooldown_is_scoped_to_its_host_and_cleared_on_reconnect() {
        // Names unique to this test: the cache is process-global.
        let host = "CooldownTestHost";
        let other = "CooldownOtherHost";
        assert!(cooled_down(host).is_none(), "starts clean");

        remember_refusal(host, "TestHost refused: no access");
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
