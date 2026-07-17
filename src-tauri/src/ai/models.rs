//! Live model discovery: asks each provider what models the user's key can
//! actually use, via its `/models` endpoint. This reflects real entitlements
//! (most visible with GitHub Copilot, where a plan enables a specific set),
//! and falls back to the static models.dev catalog whenever the live call
//! isn't available or fails.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use super::catalog::{CatalogModel, CatalogProvider, Dialect};
use super::{auth, client};
use crate::error::AppError;

const TIMEOUT: Duration = Duration::from_secs(15);

/// Fetches the user's usable models for a provider, best effort. Never errors
/// out to the caller for a plain network/permission problem: it returns the
/// static catalog list instead so the picker always has something to show.
pub async fn list(app: &tauri::AppHandle, provider: &CatalogProvider) -> Vec<CatalogModel> {
  match fetch_live(app, provider).await {
    Ok(mut models) if !models.is_empty() => {
      models.sort_by(|a, b| a.id.cmp(&b.id));
      models.dedup_by(|a, b| a.id == b.id);
      models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
      models
    }
    _ => provider.models.clone(),
  }
}

async fn fetch_live(
  app: &tauri::AppHandle,
  provider: &CatalogProvider,
) -> Result<Vec<CatalogModel>, AppError> {
  let bearer = match auth::get(app, &provider.id)? {
    Some(auth::AuthInfo::Api { key }) => key,
    Some(auth::AuthInfo::Oauth { refresh, .. }) => refresh,
    None => return Ok(Vec::new()),
  };

  let http = reqwest::Client::new();
  let base = provider.base_url.trim_end_matches('/');

  let (url, builder) = match provider.dialect {
    Dialect::Anthropic => {
      let url = format!("{base}/v1/models?limit=1000");
      let b = http
        .get(&url)
        .header("x-api-key", &bearer)
        .header("anthropic-version", "2023-06-01");
      (url, b)
    }
    Dialect::OpenAi => {
      let url = format!("{base}/models");
      let b = http.get(&url).bearer_auth(&bearer);
      (url, b)
    }
  };

  let res = client::extra_headers(&provider.id, builder)
    .timeout(TIMEOUT)
    .send()
    .await
    .and_then(reqwest::Response::error_for_status)
    .map_err(|e| AppError::Other(format!("model list request to {url} failed: {e}")))?;

  let body: Value = res
    .json()
    .await
    .map_err(|e| AppError::Other(format!("bad model list response: {e}")))?;

  Ok(parse_models(provider, &body))
}

// OpenAI-dialect /models item. Copilot labels each model in `name` (plain
// OpenAI omits it and only has `id`) and marks the models the account's plan
// can actually use with `model_picker_enabled`.
#[derive(Deserialize)]
struct OpenAiModel {
  id: String,
  #[serde(default)]
  name: Option<String>,
  // Copilot-only: true for models the active subscription enables. Absent on
  // other OpenAI-compatible providers. An account with no Copilot subscription
  // gets `false` on every model.
  #[serde(default)]
  model_picker_enabled: Option<bool>,
}

// Anthropic /models labels each model in `display_name`.
#[derive(Deserialize)]
struct AnthropicModel {
  id: String,
  #[serde(default)]
  display_name: Option<String>,
}

fn parse_models(provider: &CatalogProvider, body: &Value) -> Vec<CatalogModel> {
  let Some(data) = body.get("data").and_then(Value::as_array) else {
    return Vec::new();
  };

  match provider.dialect {
    Dialect::Anthropic => data
      .iter()
      .filter_map(|item| serde_json::from_value::<AnthropicModel>(item.clone()).ok())
      .map(|m| CatalogModel {
        name: m.display_name.unwrap_or_else(|| m.id.clone()),
        id: m.id,
        enabled: true,
      })
      .collect(),
    // Every model is kept, including ones the plan can't use, so the picker can
    // show them greyed out. Copilot's `model_picker_enabled` gives usability;
    // other OpenAI-compatible providers omit it, so their models are all usable.
    // An account with no active Copilot subscription has `false` on every model.
    Dialect::OpenAi => data
      .iter()
      .filter_map(|item| serde_json::from_value::<OpenAiModel>(item.clone()).ok())
      .map(|m| CatalogModel {
        name: m.name.unwrap_or_else(|| m.id.clone()),
        id: m.id,
        enabled: m.model_picker_enabled.unwrap_or(true),
      })
      .collect(),
  }
}
