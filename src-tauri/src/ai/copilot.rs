//! GitHub Copilot sign-in via the OAuth device-code flow (RFC 8628).
//! The resulting GitHub token is used directly as the bearer token against
//! api.githubcopilot.com.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;

/// Public OAuth client id, same one opencode and other Copilot CLIs use.
const CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DeviceCodeInfo {
  pub device_code: String,
  pub user_code: String,
  pub verification_uri: String,
  /// Minimum seconds between polls.
  pub interval: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PollResult {
  /// Token acquired and saved; sign-in is complete.
  Complete,
  /// User has not finished authorizing yet; poll again after `interval`.
  Pending { interval: u32 },
}

fn client() -> reqwest::Client {
  reqwest::Client::new()
}

pub async fn device_start() -> Result<DeviceCodeInfo, AppError> {
  let res = client()
    .post(DEVICE_CODE_URL)
    .header("Accept", "application/json")
    .header("User-Agent", "GitWyrm")
    .json(&serde_json::json!({ "client_id": CLIENT_ID, "scope": "read:user" }))
    .timeout(TIMEOUT)
    .send()
    .await
    .and_then(reqwest::Response::error_for_status)
    .map_err(|e| AppError::Other(format!("GitHub device authorization failed: {e}")))?;

  res
    .json::<DeviceCodeInfo>()
    .await
    .map_err(|e| AppError::Other(format!("bad device code response: {e}")))
}

#[derive(Deserialize)]
struct TokenResponse {
  access_token: Option<String>,
  error: Option<String>,
  interval: Option<u32>,
}

/// One poll of the token endpoint. Returns the token when authorized,
/// Pending (with the interval to wait) while the user is still signing in,
/// and an error for terminal states (denied, expired).
pub async fn device_poll(device_code: &str, interval: u32) -> Result<PollOutcome, AppError> {
  let res = client()
    .post(ACCESS_TOKEN_URL)
    .header("Accept", "application/json")
    .header("User-Agent", "GitWyrm")
    .json(&serde_json::json!({
      "client_id": CLIENT_ID,
      "device_code": device_code,
      "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
    }))
    .timeout(TIMEOUT)
    .send()
    .await
    .and_then(reqwest::Response::error_for_status)
    .map_err(|e| AppError::Other(format!("GitHub token poll failed: {e}")))?;

  let body: TokenResponse = res
    .json()
    .await
    .map_err(|e| AppError::Other(format!("bad token response: {e}")))?;

  if let Some(token) = body.access_token {
    return Ok(PollOutcome::Token(token));
  }

  match body.error.as_deref() {
    Some("authorization_pending") => Ok(PollOutcome::Pending { interval }),
    // RFC 8628: on slow_down add 5s to the interval (GitHub may also send one).
    Some("slow_down") => Ok(PollOutcome::Pending {
      interval: body.interval.unwrap_or(interval + 5),
    }),
    Some("expired_token") => Err(AppError::Other(
      "sign-in code expired, start over to get a new code".into(),
    )),
    Some("access_denied") => Err(AppError::Other("sign-in was cancelled on GitHub".into())),
    Some(other) => Err(AppError::Other(format!("GitHub sign-in failed: {other}"))),
    None => Ok(PollOutcome::Pending { interval }),
  }
}

pub enum PollOutcome {
  Token(String),
  Pending { interval: u32 },
}
