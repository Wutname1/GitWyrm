//! Minimal chat-completion client speaking the two API dialects that cover
//! the catalog: Anthropic Messages and OpenAI chat/completions.

use std::time::Duration;

use serde_json::{json, Value};

use super::catalog::{CatalogProvider, Dialect};
use crate::error::AppError;

const TIMEOUT: Duration = Duration::from_secs(60);

pub struct ChatRequest<'a> {
  pub provider: &'a CatalogProvider,
  pub bearer: &'a str,
  pub model: &'a str,
  pub system: &'a str,
  pub user: &'a str,
  pub max_tokens: u32,
}

/// Extra headers some providers require beyond the bearer token.
fn extra_headers(provider_id: &str, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
  match provider_id {
    "github-copilot" => req
      .header("Copilot-Integration-Id", "vscode-chat")
      .header("Editor-Version", "GitWyrm/0.1")
      .header("User-Agent", "GitWyrm"),
    _ => req.header("User-Agent", "GitWyrm"),
  }
}

pub async fn chat(req: ChatRequest<'_>) -> Result<String, AppError> {
  let client = reqwest::Client::new();
  let base = req.provider.base_url.trim_end_matches('/');

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
    .timeout(TIMEOUT)
    .json(&body)
    .send()
    .await
    .map_err(|e| AppError::Other(format!("AI request failed: {e}")))?;

  let status = res.status();
  let text = res
    .text()
    .await
    .map_err(|e| AppError::Other(format!("AI response read failed: {e}")))?;

  if !status.is_success() {
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

  content.ok_or_else(|| AppError::Other(format!("AI response had no text: {}", snippet(&text))))
}

fn snippet(text: &str) -> String {
  text.chars().take(300).collect()
}
