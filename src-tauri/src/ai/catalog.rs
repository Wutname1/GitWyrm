//! Provider/model catalog sourced from https://models.dev/api.json, cached
//! on disk in the app data dir. Only providers GitWyrm can actually talk to
//! are exposed: simple bearer-key APIs speaking either the Anthropic Messages
//! dialect or the OpenAI chat/completions dialect.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;

use crate::error::AppError;

const CATALOG_URL: &str = "https://models.dev/api.json";
const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
  Anthropic,
  OpenAi,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct CatalogModel {
  pub id: String,
  pub name: String,
  /// Whether the account can actually select this model. Always true for static
  /// catalog entries; a live Copilot list marks plan-gated models false so the
  /// picker can show them greyed out instead of hiding them.
  pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct CatalogProvider {
  pub id: String,
  pub name: String,
  pub base_url: String,
  pub dialect: Dialect,
  pub models: Vec<CatalogModel>,
}

// Raw models.dev shapes (only the fields we consume).
#[derive(Deserialize)]
struct RawModel {
  id: String,
  name: String,
  #[serde(default)]
  status: Option<String>,
}

#[derive(Deserialize)]
struct RawProvider {
  id: String,
  name: String,
  #[serde(default)]
  api: Option<String>,
  #[serde(default)]
  npm: Option<String>,
  #[serde(default)]
  models: BTreeMap<String, RawModel>,
}

/// Known base URLs for popular providers that omit `api` in models.dev
/// (implied by their AI-SDK package there). Everything else without an
/// `api` URL is skipped.
fn known_base_url(id: &str) -> Option<(&'static str, Dialect)> {
  match id {
    "anthropic" => Some(("https://api.anthropic.com", Dialect::Anthropic)),
    "openai" => Some(("https://api.openai.com/v1", Dialect::OpenAi)),
    "groq" => Some(("https://api.groq.com/openai/v1", Dialect::OpenAi)),
    "mistral" => Some(("https://api.mistral.ai/v1", Dialect::OpenAi)),
    "xai" => Some(("https://api.x.ai/v1", Dialect::OpenAi)),
    "google" => Some((
      "https://generativelanguage.googleapis.com/v1beta/openai",
      Dialect::OpenAi,
    )),
    _ => None,
  }
}

fn cache_path(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
  let dir = app
    .path()
    .app_data_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  fs::create_dir_all(&dir)?;
  Ok(dir.join("models-catalog.json"))
}

fn cache_fresh(path: &std::path::Path) -> bool {
  fs::metadata(path)
    .and_then(|m| m.modified())
    .ok()
    .and_then(|t| SystemTime::now().duration_since(t).ok())
    .is_some_and(|age| age < CACHE_TTL)
}

async fn fetch_raw(app: &tauri::AppHandle) -> Result<String, AppError> {
  let path = cache_path(app)?;
  if cache_fresh(&path) {
    if let Ok(cached) = fs::read_to_string(&path) {
      return Ok(cached);
    }
  }

  let fetched = reqwest::Client::new()
    .get(CATALOG_URL)
    .timeout(Duration::from_secs(15))
    .send()
    .await
    .and_then(reqwest::Response::error_for_status);

  match fetched {
    Ok(res) => {
      let body = res
        .text()
        .await
        .map_err(|e| AppError::Other(format!("failed to read model catalog: {e}")))?;
      let _ = fs::write(&path, &body);
      Ok(body)
    }
    // Offline or upstream down: fall back to a stale cache when we have one.
    Err(e) => fs::read_to_string(&path)
      .map_err(|_| AppError::Other(format!("failed to fetch model catalog: {e}"))),
  }
}

fn parse(raw: &str) -> Result<Vec<CatalogProvider>, AppError> {
  let providers: BTreeMap<String, RawProvider> =
    serde_json::from_str(raw).map_err(|e| AppError::Other(format!("bad model catalog: {e}")))?;

  let mut out = Vec::new();
  for (_, p) in providers {
    let (base_url, dialect) = match known_base_url(&p.id) {
      Some((url, dialect)) => (url.to_string(), dialect),
      None => match &p.api {
        Some(api) => {
          let dialect = if p.npm.as_deref().is_some_and(|n| n.contains("anthropic")) {
            Dialect::Anthropic
          } else {
            Dialect::OpenAi
          };
          (api.trim_end_matches('/').to_string(), dialect)
        }
        // No usable HTTP endpoint (bedrock/vertex/azure style auth): skip.
        None => continue,
      },
    };

    let mut models: Vec<CatalogModel> = p
      .models
      .into_values()
      .filter(|m| m.status.as_deref() != Some("deprecated"))
      .map(|m| CatalogModel { id: m.id, name: m.name, enabled: true })
      .collect();
    if models.is_empty() {
      continue;
    }
    models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    out.push(CatalogProvider {
      id: p.id,
      name: p.name,
      base_url,
      dialect,
      models,
    });
  }

  out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
  Ok(out)
}

pub async fn get(app: &tauri::AppHandle) -> Result<Vec<CatalogProvider>, AppError> {
  let raw = fetch_raw(app).await?;
  parse(&raw)
}

pub async fn find(app: &tauri::AppHandle, provider_id: &str) -> Result<CatalogProvider, AppError> {
  get(app)
    .await?
    .into_iter()
    .find(|p| p.id == provider_id)
    .ok_or_else(|| AppError::Other(format!("unknown AI provider: {provider_id}")))
}
