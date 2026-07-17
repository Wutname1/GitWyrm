//! App settings: a JSON file in the app data dir holding the open/recent
//! repos and other UI preferences, so they survive relaunch. Mirrors the
//! `ai::auth` module's pattern (plain JSON, no encryption needed here).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecentRepo {
  pub name: String,
  pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
  Stable,
  Beta,
}

/// What to do with uncommitted changes when switching branches.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BranchSwitchMode {
  /// Stash before switching, reapply after. A conflicting reapply leaves the
  /// stash intact as a backup. Most protective; the default.
  AutoStash,
  /// Plain `git checkout`: carry changes to the new branch, but refuse the
  /// switch if any change would be overwritten.
  Carry,
  /// Refuse to switch while the working tree is dirty.
  Refuse,
}

fn default_branch_switch_mode() -> BranchSwitchMode {
  BranchSwitchMode::AutoStash
}

/// Commit-graph column layout. Column ids are validated on the frontend, so
/// unknown values here are ignored rather than rejected.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ColumnLayout {
  /// Column ids in display order.
  #[serde(default)]
  pub order: Vec<String>,
  /// Column ids the user has hidden.
  #[serde(default)]
  pub hidden: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Settings {
  /// Paths of repos open in tabs, in tab order, so they can be reopened on launch.
  #[serde(default)]
  pub open_repos: Vec<String>,
  /// Id of the repo that was active when the app last closed.
  #[serde(default)]
  pub active_repo_path: Option<String>,
  #[serde(default)]
  pub recents: Vec<RecentRepo>,
  #[serde(default)]
  pub code_folder: Option<String>,
  #[serde(default)]
  pub clone_directory: Option<String>,
  #[serde(default = "default_update_channel")]
  pub update_channel: UpdateChannel,
  #[serde(default = "default_branch_switch_mode")]
  pub branch_switch_mode: BranchSwitchMode,
  #[serde(default)]
  pub ai_provider: Option<String>,
  #[serde(default)]
  pub ai_model: Option<String>,
  /// Custom system instruction for commit-message generation. None uses the
  /// built-in default (see `crate::ai::prompt::DEFAULT_INSTRUCTION`).
  #[serde(default)]
  pub ai_instruction: Option<String>,
  /// Commit-graph column order and visibility.
  #[serde(default)]
  pub column_layout: Option<ColumnLayout>,
}

fn default_update_channel() -> UpdateChannel {
  UpdateChannel::Stable
}

impl Default for Settings {
  fn default() -> Self {
    Self {
      open_repos: Vec::new(),
      active_repo_path: None,
      recents: Vec::new(),
      code_folder: None,
      clone_directory: None,
      update_channel: default_update_channel(),
      branch_switch_mode: default_branch_switch_mode(),
      ai_provider: None,
      ai_model: None,
      ai_instruction: None,
      column_layout: None,
    }
  }
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
  let dir = app
    .path()
    .app_data_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  fs::create_dir_all(&dir)?;
  Ok(dir.join("settings.json"))
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(app: tauri::AppHandle) -> Result<Settings, AppError> {
  let path = settings_path(&app)?;
  let Ok(raw) = fs::read_to_string(&path) else {
    return Ok(Settings::default());
  };
  Ok(serde_json::from_str(&raw).unwrap_or_default())
}

#[tauri::command]
#[specta::specta]
pub fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), AppError> {
  let path = settings_path(&app)?;
  let json = serde_json::to_string_pretty(&settings).map_err(|e| AppError::Other(e.to_string()))?;
  fs::write(path, json)?;
  Ok(())
}
