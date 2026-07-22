//! Minimal chat-completion client speaking the two API dialects that cover
//! the catalog: Anthropic Messages and OpenAI chat/completions.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::catalog::{CatalogProvider, Dialect};
use crate::error::AppError;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

pub struct ChatRequest<'a> {
  pub provider: &'a CatalogProvider,
  pub bearer: &'a str,
  pub model: &'a str,
  pub system: &'a str,
  pub user: &'a str,
  pub max_tokens: u32,
  pub timeout: Duration,
}

/// Extra headers some providers require beyond the bearer token.
pub fn extra_headers(provider_id: &str, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
  match provider_id {
    "github-copilot" => req
      .header("Copilot-Integration-Id", "vscode-chat")
      .header("Editor-Version", "GitWyrm/0.1")
      .header("X-GitHub-Api-Version", "2025-04-01")
      .header("User-Agent", "GitWyrm"),
    _ => req.header("User-Agent", "GitWyrm"),
  }
}

pub async fn chat(req: ChatRequest<'_>) -> Result<String, AppError> {
  let client = reqwest::Client::new();
  let base = req.provider.base_url.trim_end_matches('/');
  let input_chars = req.system.chars().count() + req.user.chars().count();
  let started = Instant::now();

  log::info!(
    "AI request started: provider={}, model={}, input_chars={}, max_tokens={}, timeout_secs={}",
    req.provider.id,
    req.model,
    input_chars,
    req.max_tokens,
    req.timeout.as_secs()
  );

  let (url, builder, body): (String, reqwest::RequestBuilder, Value) = match req.provider.dialect {
    Dialect::Anthropic => {
      let url = format!("{base}/v1/messages");
      let builder = client
        .post(&url)
        .header("x-api-key", req.bearer)
        .header("anthropic-version", "2023-06-01");
      let body = json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "system": req.system,
        "messages": [{ "role": "user", "content": req.user }],
      });
      (url, builder, body)
    }
    Dialect::OpenAi => {
      let url = format!("{base}/chat/completions");
      let builder = client.post(&url).bearer_auth(req.bearer);
      let body = json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "messages": [
          { "role": "system", "content": req.system },
          { "role": "user", "content": req.user },
        ],
      });
      (url, builder, body)
    }
  };

  let res = extra_headers(&req.provider.id, builder)
    .timeout(req.timeout)
    .json(&body)
    .send()
    .await
    .map_err(|error| {
      let elapsed_ms = started.elapsed().as_millis();
      log::error!(
        "AI request transport failed: provider={}, model={}, input_chars={}, elapsed_ms={}, timeout={}, connect={}, error={}",
        req.provider.id,
        req.model,
        input_chars,
        elapsed_ms,
        error.is_timeout(),
        error.is_connect(),
        error
      );
      if error.is_timeout() {
        AppError::Other(format!(
          "AI took longer than {} to respond. No changes were made. Try again, or choose a faster model.",
          duration_label(req.timeout)
        ))
      } else {
        AppError::Other(format!("AI request failed: {error}"))
      }
    })?;

  let status = res.status();
  let text = res
    .text()
    .await
    .map_err(|e| AppError::Other(format!("AI response read failed: {e}")))?;

  log::info!(
    "AI response received: provider={}, model={}, status={}, response_chars={}, elapsed_ms={}",
    req.provider.id,
    req.model,
    status,
    text.chars().count(),
    started.elapsed().as_millis()
  );

  if !status.is_success() {
    log::error!(
      "AI provider rejected request: provider={}, model={}, status={}, response={}",
      req.provider.id,
      req.model,
      status,
      snippet(&text)
    );
    return Err(AppError::Other(format!(
      "AI request to {url} failed ({status}): {}",
      snippet(&text)
    )));
  }

  let parsed: Value =
    serde_json::from_str(&text).map_err(|e| AppError::Other(format!("bad AI response: {e}")))?;

  let content = match req.provider.dialect {
    Dialect::Anthropic => parsed["content"]
      .as_array()
      .and_then(|parts| {
        parts
          .iter()
          .find(|p| p["type"] == "text")
          .and_then(|p| p["text"].as_str())
      })
      .map(str::to_string),
    Dialect::OpenAi => parsed["choices"][0]["message"]["content"]
      .as_str()
      .map(str::to_string),
  };

  content.ok_or_else(|| {
    let completion_tokens = parsed["usage"]["completion_tokens"].as_u64();
    let finish_reason = parsed["choices"][0]["finish_reason"].as_str().unwrap_or("missing");
    let choices = parsed["choices"].as_array().map(Vec::len).unwrap_or_default();
    log::error!(
      "AI response had no text: provider={}, model={}, choices={}, finish_reason={}, completion_tokens={:?}, max_tokens={}",
      req.provider.id,
      req.model,
      choices,
      finish_reason,
      completion_tokens,
      req.max_tokens
    );

    if completion_tokens.is_some_and(|used| used >= u64::from(req.max_tokens)) {
      AppError::Other(format!(
        "AI used all {} response tokens before it finished. No changes were made. Try again with fewer changes or a faster model.",
        req.max_tokens
      ))
    } else {
      AppError::Other(format!("AI response had no text: {}", snippet(&text)))
    }
  })
}

fn snippet(text: &str) -> String {
  text.chars().take(300).collect()
}

fn duration_label(duration: Duration) -> String {
  let seconds = duration.as_secs();
  if seconds >= 60 && seconds % 60 == 0 {
    let minutes = seconds / 60;
    format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" })
  } else {
    format!("{seconds} second{}", if seconds == 1 { "" } else { "s" })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn timeout_labels_are_plain_language() {
    assert_eq!(duration_label(Duration::from_secs(300)), "5 minutes");
    assert_eq!(duration_label(Duration::from_secs(45)), "45 seconds");
  }
}
