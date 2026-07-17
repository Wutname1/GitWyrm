use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::git::types::RepoInfo;
use crate::state::RepoManager;
use crate::watcher::WatcherRegistry;

fn head_branch(repo: &git2::Repository) -> Option<String> {
  let head = repo.head().ok()?;
  if head.is_branch() {
    head.shorthand().map(str::to_string)
  } else {
    None
  }
}

#[tauri::command]
#[specta::specta]
pub async fn open_repo(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  watchers: State<'_, WatcherRegistry>,
  path: String,
) -> Result<RepoInfo, AppError> {
  let (id, open) = manager.open(&path)?;

  watchers
    .watch(app, id.clone(), &open.path)
    .map_err(AppError::Other)?;

  let repo = open.repo.lock().unwrap();
  let name = open
    .path
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| "repository".into());
  Ok(RepoInfo {
    id,
    name,
    path: open.path.to_string_lossy().into_owned(),
    head_branch: head_branch(&repo),
  })
}

#[tauri::command]
#[specta::specta]
pub async fn close_repo(
  manager: State<'_, RepoManager>,
  watchers: State<'_, WatcherRegistry>,
  repo_id: String,
) -> Result<(), AppError> {
  watchers.unwatch(&repo_id);
  manager.close(&repo_id);
  Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn git_available() -> Result<bool, AppError> {
  Ok(crate::git::shell::git_available())
}
