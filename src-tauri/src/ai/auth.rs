//! Credential store for AI providers: a JSON file in the app data dir keyed
//! by provider ID. Keys are written by the settings UI and only ever read
//! back on the Rust side; they are never returned to the webview.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthInfo {
  Api {
    key: String,
  },
  Oauth {
    refresh: String,
    access: String,
    expires: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    enterprise_url: Option<String>,
  },
}

fn auth_path(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
  let dir = app
    .path()
    .app_data_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  fs::create_dir_all(&dir)?;
  Ok(dir.join("auth.json"))
}

pub fn load_all(app: &tauri::AppHandle) -> Result<BTreeMap<String, AuthInfo>, AppError> {
  let path = auth_path(app)?;
  let Ok(raw) = fs::read_to_string(&path) else {
    return Ok(BTreeMap::new());
  };
  Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn get(app: &tauri::AppHandle, provider: &str) -> Result<Option<AuthInfo>, AppError> {
  Ok(load_all(app)?.remove(provider))
}

pub fn set(app: &tauri::AppHandle, provider: &str, info: AuthInfo) -> Result<(), AppError> {
  let mut all = load_all(app)?;
  all.insert(provider.to_string(), info);
  save(app, &all)
}

pub fn remove(app: &tauri::AppHandle, provider: &str) -> Result<(), AppError> {
  let mut all = load_all(app)?;
  all.remove(provider);
  save(app, &all)
}

fn save(app: &tauri::AppHandle, all: &BTreeMap<String, AuthInfo>) -> Result<(), AppError> {
  let path = auth_path(app)?;
  let json = serde_json::to_string_pretty(all).map_err(|e| AppError::Other(e.to_string()))?;
  fs::write(&path, json)?;
  restrict_permissions(&path);
  Ok(())
}

/// Best effort: on Unix chmod 600. Windows app data is already per-user.
fn restrict_permissions(path: &std::path::Path) {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
  }
  #[cfg(not(unix))]
  let _ = path;
}
