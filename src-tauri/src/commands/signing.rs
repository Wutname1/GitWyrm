//! Commands behind the Security settings screen: reporting the signing setup,
//! making a key, and turning signing on or off.

use crate::error::AppError;
use crate::git::bundled::ToolSource;
use crate::git::signing::{self, SigningStatus};

/// Which git and gpg the app resolved, and where each came from. Drives the
/// "using the copy that came with GitWyrm" vs "using your own" line in Settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
  pub program: String,
  pub source: ToolSource,
  pub version: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn git_tool_info() -> Result<ToolInfo, AppError> {
  let program = crate::git::shell::git_program_name();
  let source = crate::git::shell::git_source();
  let version = crate::git::shell::run_git(None, &["--version"])
    .ok()
    .map(|out| out.stdout.trim().to_owned());

  Ok(ToolInfo {
    program,
    source,
    version,
  })
}

#[tauri::command]
#[specta::specta]
pub async fn gpg_tool_info() -> Result<ToolInfo, AppError> {
  let program = signing::gpg_program_name();
  let source = signing::gpg_source();
  let version = signing::run_gpg(&["--version"])
    .ok()
    .and_then(|out| out.lines().next().map(str::to_owned));

  Ok(ToolInfo {
    program,
    source,
    version,
  })
}

#[tauri::command]
#[specta::specta]
pub async fn get_signing_status(repo_path: String) -> Result<SigningStatus, AppError> {
  // Cheap enough to run inline, but it does spawn gpg twice; keep it off the
  // hot path by only calling it when the Security screen is open.
  tauri::async_runtime::spawn_blocking(move || signing::signing_status(&repo_path))
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn create_signing_key(name: String, email: String) -> Result<String, AppError> {
  tauri::async_runtime::spawn_blocking(move || signing::generate_key(&name, &email))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn export_signing_key(key_id: String) -> Result<String, AppError> {
  tauri::async_runtime::spawn_blocking(move || signing::export_public_key(&key_id))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn set_signing_enabled(
  repo_path: String,
  enabled: bool,
  key_id: Option<String>,
) -> Result<(), AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    if enabled {
      let key = key_id.ok_or_else(|| {
        AppError::Other("Pick a key to sign with before turning signing on.".into())
      })?;
      signing::enable_signing(&repo_path, &key)
    } else {
      signing::disable_signing(&repo_path)
    }
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn repair_signing_format(repo_path: String) -> Result<(), AppError> {
  tauri::async_runtime::spawn_blocking(move || signing::repair_gpg_format(&repo_path))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
