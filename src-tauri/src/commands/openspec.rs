//! Tauri commands for the OpenSpec integration.
//!
//! Thin wrappers: the work lives in `crate::openspec`. Filesystem reads run on
//! the blocking pool like every other repo command, and the repo handle is only
//! used for its path -- nothing here touches git.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::error::AppError;
use crate::openspec::{self, cli, parse, write};
use crate::state::RepoManager;

/// Whether this repository uses OpenSpec, and whether the CLI is around.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenspecStatus {
  /// False for every repo without an `openspec/` folder -- the UI shows nothing
  /// at all in that case.
  pub present: bool,
  /// Active (non-archived) change count, for the sidebar and status bar.
  pub active_count: u32,
  /// Archived change count, for the Desk's archive link.
  pub archived_count: u32,
  pub cli: cli::CliInfo,
}

/// Resolves a repo id to its working directory.
fn repo_root(manager: &RepoManager, repo_id: &str) -> Result<PathBuf, AppError> {
  Ok(manager.get(repo_id)?.path.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn openspec_status(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<OpenspecStatus, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let Some(dir) = openspec::openspec_dir(&root) else {
      return OpenspecStatus {
        present: false,
        active_count: 0,
        archived_count: 0,
        // Don't probe for the CLI when the repo has no specs: nothing would use
        // the answer, and probing can shell out to npx.
        cli: cli::CliInfo { available: false, version: None, invocation: None },
      };
    };
    OpenspecStatus {
      present: true,
      active_count: parse::parse_changes_dir(&dir).len() as u32,
      archived_count: parse::archived_ids(&dir).len() as u32,
      cli: cli::detect(),
    }
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// Every active change, newest first. Empty for a repo without `openspec/`.
#[tauri::command]
#[specta::specta]
pub async fn openspec_list_changes(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<parse::SpecChange>, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    openspec::openspec_dir(&root)
      .map(|dir| parse::parse_changes_dir(&dir))
      .unwrap_or_default()
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// One change by id, or None when it is not there (deleted or archived while
/// the UI was looking at it).
#[tauri::command]
#[specta::specta]
pub async fn openspec_get_change(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
) -> Result<Option<parse::SpecChange>, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)?;
    parse::parse_change_dir(&dir.join("changes").join(&change_id))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// Ids of archived changes, newest first.
#[tauri::command]
#[specta::specta]
pub async fn openspec_archived_ids(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<String>, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    openspec::openspec_dir(&root)
      .map(|dir| parse::archived_ids(&dir))
      .unwrap_or_default()
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// Ticks or unticks one task, writing exactly that checkbox to tasks.md.
///
/// `line` comes from the parsed task the user clicked. If the file has moved on
/// since (an agent inserted tasks, someone reordered them), the write is skipped
/// and `LineMoved` tells the UI to re-read rather than guess.
#[tauri::command]
#[specta::specta]
pub async fn openspec_toggle_task(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
  line: u32,
  done: bool,
) -> Result<write::ToggleOutcome, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)
      .ok_or_else(|| AppError::Other("this repository has no openspec folder".to_string()))?;
    let path = write::tasks_path(&dir, &change_id);
    write::toggle_task_line(&path, line, done)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Creates a new change folder with template files.
#[tauri::command]
#[specta::specta]
pub async fn openspec_scaffold_change(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  description: String,
) -> Result<write::ScaffoldResult, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)
      .ok_or_else(|| AppError::Other("this repository has no openspec folder".to_string()))?;
    write::scaffold_change(&dir, &name, &description)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Runs `openspec validate` for one change. Never errors for a missing CLI --
/// that comes back as a `cliMissing` outcome with a plain-language hint.
#[tauri::command]
#[specta::specta]
pub async fn openspec_validate_change(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
) -> Result<cli::CliOutcome, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || cli::validate_change(&root, &change_id))
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Runs `openspec archive` for one change: merges its deltas into the specs
/// library and moves the folder into `changes/archive/`.
#[tauri::command]
#[specta::specta]
pub async fn openspec_archive_change(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
) -> Result<cli::CliOutcome, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || cli::archive_change(&root, &change_id))
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}
