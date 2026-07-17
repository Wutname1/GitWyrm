//! App-level commands: build info and log file management.

use std::fs;

use serde::Serialize;
use specta::Type;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

use crate::error::AppError;

pub const LOG_FILE_NAME: &str = "gitwyrm";

#[derive(Debug, Clone, Serialize, Type)]
pub struct BuildInfo {
  pub version: String,
  pub build_date: String,
  pub git_hash: String,
  pub debug: bool,
}

#[tauri::command]
#[specta::specta]
pub fn build_info() -> BuildInfo {
  BuildInfo {
    version: env!("CARGO_PKG_VERSION").to_string(),
    build_date: env!("GW_BUILD_DATE").to_string(),
    git_hash: env!("GW_GIT_HASH").to_string(),
    debug: cfg!(debug_assertions),
  }
}

fn log_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, AppError> {
  let dir = app
    .path()
    .app_log_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  Ok(dir.join(format!("{LOG_FILE_NAME}.log")))
}

/// Returns the current log file contents ("" when it does not exist yet).
#[tauri::command]
#[specta::specta]
pub fn read_log(app: tauri::AppHandle) -> Result<String, AppError> {
  let path = log_path(&app)?;
  Ok(fs::read_to_string(path).unwrap_or_default())
}

/// Truncates the log file in place so the logger's open handle stays valid.
#[tauri::command]
#[specta::specta]
pub fn clear_log(app: tauri::AppHandle) -> Result<(), AppError> {
  let path = log_path(&app)?;
  if path.exists() {
    fs::OpenOptions::new().write(true).truncate(true).open(path)?;
  }
  Ok(())
}

/// Opens the log directory in the OS file manager.
#[tauri::command]
#[specta::specta]
pub fn open_logs_folder(app: tauri::AppHandle) -> Result<(), AppError> {
  let dir = app
    .path()
    .app_log_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  if !dir.exists() {
    fs::create_dir_all(&dir)?;
  }
  app
    .opener()
    .open_path(dir.to_string_lossy().to_string(), None::<&str>)
    .map_err(|e| AppError::Other(e.to_string()))
}
