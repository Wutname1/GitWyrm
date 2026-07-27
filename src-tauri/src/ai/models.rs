//! Live model discovery: asks each provider what models the user's key can
//! actually use, via its `/models` endpoint. This reflects real entitlements
//! (most visible with GitHub Copilot, where a plan enables a specific set),
//! and falls back to the static models.dev catalog whenever the live call
//! isn't available or fails.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::catalog::{CatalogModel, CatalogProvider, Dialect};
use super::{auth, client};
use crate::error::AppError;

const TIMEOUT: Duration = Duration::from_secs(15);

/// Fetches the user's usable models for a provider, best effort. Never errors
/// out to the caller for a plain network/permission problem: it returns the
/// static catalog list instead so the picker always has something to show.
///
/// `live` reports which of the two happened. The static catalog cannot know
/// plan entitlements and marks everything enabled, so a caller that would
/// auto-select a model must not treat `live: false` as an endorsement.
pub async fn list(app: &tauri::AppHandle, provider: &CatalogProvider) -> ModelList {
  match fetch_live(app, provider).await {
    Ok(models) if !models.is_empty() => {
      let mut models = dedupe_by_id(models);
      models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
      let enabled = models.iter().filter(|m| m.enabled).count();
      log::info!(
        "model detection live: provider={}, models={}, enabled={}",
        provider.id,
        models.len(),
        enabled
      );
      if enabled == 0 {
        log::warn!(
          "model detection live but no models are enabled: provider={}. For Copilot this means the signed-in account has no active subscription, or the token lacks Copilot access.",
          provider.id
        );
      }
      ModelList { models, live: true }
    }
    Ok(_) => {
      log::warn!(
        "model detection returned an empty list, falling back to static catalog: provider={}",
        provider.id
      );
      ModelList {
        models: provider.models.clone(),
        live: false,
      }
    }
    Err(e) => {
      log::warn!(
        "model detection failed, falling back to static catalog: provider={}, error={}",
        provider.id,
        e
      );
      ModelList {
        models: provider.models.clone(),
        live: false,
      }
    }
  }
}

/// Collapse duplicate model ids, keeping a single entry per id. Copilot lists
/// the same id more than once (capability variants) and the `enabled` flag can
/// differ between copies; a model is usable if ANY copy is enabled, so we OR
/// the flag together instead of arbitrarily keeping whichever sorted first.
fn dedupe_by_id(models: Vec<CatalogModel>) -> Vec<CatalogModel> {
  let mut order: Vec<String> = Vec::new();
  let mut by_id: std::collections::HashMap<String, CatalogModel> = std::collections::HashMap::new();
  for model in models {
    match by_id.get_mut(&model.id) {
      Some(existing) => existing.enabled = existing.enabled || model.enabled,
      None => {
        order.push(model.id.clone());
        by_id.insert(model.id.clone(), model);
      }
    }
  }
  order
    .into_iter()
    .filter_map(|id| by_id.remove(&id))
    .collect()
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ModelList {
  pub models: Vec<CatalogModel>,
  /// True when the list came from the provider's own `/models` endpoint, so
  /// `enabled` reflects real entitlements rather than a static assumption.
  pub live: bool,
}

async fn fetch_live(
  app: &tauri::AppHandle,
  provider: &CatalogProvider,
) -> Result<Vec<CatalogModel>, AppError> {
  // These are info! rather than debug! on purpose: release builds log at Info,
  // and when a user reports "my models are wrong" the credential shape and the
  // URL we called are the first two things needed to tell their machine apart
  // from a working one. `fingerprint` identifies a token without revealing it,
  // so two machines can be compared to see if they hold the same credential.
  let bearer = match auth::get(app, &provider.id)? {
    Some(auth::AuthInfo::Api { key }) => {
      log::info!(
        "model detection auth: provider={}, source=api-key, token={}",
        provider.id,
        token_fingerprint(&key)
      );
      key
    }
    Some(auth::AuthInfo::Oauth { refresh, .. }) => {
      log::info!(
        "model detection auth: provider={}, source=oauth, token={}",
        provider.id,
        token_fingerprint(&refresh)
      );
      refresh
    }
    None => {
      log::info!("model detection: no credential configured for provider={}", provider.id);
      return Ok(Vec::new());
    }
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

  log::info!("model detection request: provider={}, url={}", provider.id, url);

  let res = client::extra_headers(&provider.id, builder)
    .timeout(TIMEOUT)
    .send()
    .await
    .map_err(|e| {
      log::warn!(
        "model detection transport failed: provider={}, url={}, timeout={}, connect={}, error={}",
        provider.id,
        url,
        e.is_timeout(),
        e.is_connect(),
        e
      );
      AppError::Other(format!("model list request to {url} failed: {e}"))
    })?;

  let status = res.status();
  let res = res.error_for_status().map_err(|e| {
    log::warn!(
      "model detection rejected: provider={}, url={}, status={}, error={}",
      provider.id,
      url,
      status,
      e
    );
    AppError::Other(format!("model list request to {url} failed: {e}"))
  })?;

  let body: Value = res.json().await.map_err(|e| {
    log::warn!(
      "model detection bad response body: provider={}, url={}, error={}",
      provider.id,
      url,
      e
    );
    AppError::Other(format!("bad model list response: {e}"))
  })?;

  let raw_count = body.get("data").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
  // For Copilot, the whole "some models enabled" behaviour hinges on the raw
  // `model_picker_enabled` flag. Logging its distribution lets two machines on
  // the same account be compared directly to see if the endpoint returns the
  // same entitlements (a token/header problem shows up as all-absent/all-false).
  if provider.dialect == Dialect::OpenAi {
    if let Some(data) = body.get("data").and_then(Value::as_array) {
      let mut picker_true = 0;
      let mut picker_false = 0;
      let mut picker_absent = 0;
      for item in data {
        match item.get("model_picker_enabled").and_then(Value::as_bool) {
          Some(true) => picker_true += 1,
          Some(false) => picker_false += 1,
          None => picker_absent += 1,
        }
      }
      log::info!(
        "model detection picker flags: provider={}, model_picker_enabled true={}, false={}, absent={}",
        provider.id,
        picker_true,
        picker_false,
        picker_absent
      );
    }
  }
  let models = parse_models(provider, &body);
  log::info!(
    "model detection parsed: provider={}, status={}, raw_items={}, parsed={}",
    provider.id,
    status,
    raw_count,
    models.len()
  );
  // `raw_items > parsed` means the endpoint answered but items failed to
  // deserialize -- a schema change on their side, not a token problem. Without
  // this the two look identical in a user's log.
  if raw_count > models.len() {
    log::warn!(
      "model detection dropped items that did not match the expected shape: provider={}, raw_items={}, parsed={}",
      provider.id,
      raw_count,
      models.len()
    );
  }
  Ok(models)
}

/// A stable, non-reversible label for a credential: length plus a short hash.
///
/// Inconsistent Copilot detection across two machines is almost always two
/// different tokens (different account, or one issued before a subscription
/// change). Logging the token itself would leak it into a bug report, and
/// logging nothing makes the two cases indistinguishable -- so log a shape that
/// is equal when the tokens are equal and different otherwise.
fn token_fingerprint(token: &str) -> String {
  use std::hash::{Hash, Hasher};
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  token.hash(&mut hasher);
  // 32 bits is plenty to tell two tokens apart while staying far too short to
  // brute-force back to the original.
  let short = (hasher.finish() >> 32) as u32;
  // The prefix up to the first underscore is the token's type marker (ghu_,
  // gho_, github_pat_) and carries no secret material -- it says which kind of
  // credential this is, which is itself a common cause of the two machines
  // behaving differently.
  let kind = match token.find('_') {
    Some(i) => &token[..=i],
    None => "",
  };
  format!("{}…len={} hash={:08x}", kind, token.len(), short)
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

#[cfg(test)]
mod tests {
  use super::{dedupe_by_id, token_fingerprint};
  use crate::ai::catalog::CatalogModel;

  fn model(id: &str, enabled: bool) -> CatalogModel {
    CatalogModel {
      name: id.to_string(),
      id: id.to_string(),
      enabled,
    }
  }

  #[test]
  fn duplicate_ids_collapse_and_enabled_wins() {
    // Copilot lists the same id more than once and only one copy may carry the
    // picker flag; the model is usable if any copy says so.
    let out = dedupe_by_id(vec![
      model("gpt-4", false),
      model("gpt-4", true),
      model("claude", false),
    ]);
    assert_eq!(out.len(), 2);
    assert!(out[0].enabled, "gpt-4 should be enabled via the OR");
    assert!(!out[1].enabled);
    // First-seen order is preserved.
    assert_eq!(out[0].id, "gpt-4");
    assert_eq!(out[1].id, "claude");
  }

  #[test]
  fn identical_tokens_fingerprint_identically() {
    assert_eq!(token_fingerprint("ghu_abc123"), token_fingerprint("ghu_abc123"));
  }

  #[test]
  fn different_tokens_fingerprint_differently() {
    assert_ne!(token_fingerprint("ghu_abc123"), token_fingerprint("ghu_xyz789"));
  }

  #[test]
  fn fingerprint_keeps_the_kind_but_never_the_secret() {
    let fp = token_fingerprint("ghu_supersecretvalue");
    assert!(fp.starts_with("ghu_"), "kind marker should survive: {fp}");
    assert!(!fp.contains("supersecret"), "secret leaked: {fp}");
    assert!(fp.contains("len=20"));
  }

  #[test]
  fn fingerprint_handles_a_token_with_no_kind_marker() {
    let fp = token_fingerprint("plainsecret");
    assert!(!fp.contains("plainsecret"), "secret leaked: {fp}");
    assert!(fp.contains("len=11"));
  }
}
