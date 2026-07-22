use std::time::Instant;

use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::git::types::RepoInfo;
use crate::state::RepoManager;
use crate::watcher::WatcherRegistry;

/// Anything slower than this on a single phase of opening a repo is worth a
/// warning: it is the difference between "felt instant" and "user thinks we
/// hung". Recursive watch registration over a large working tree is the usual
/// culprit, so the phases are timed separately to say which one was slow.
const SLOW_PHASE: u128 = 2_000;

fn log_phase(phase: &str, path: &str, elapsed_ms: u128) {
  if elapsed_ms >= SLOW_PHASE {
    log::warn!("open_repo: {phase} took {elapsed_ms}ms (slow) for {path}");
  } else {
    log::info!("open_repo: {phase} took {elapsed_ms}ms for {path}");
  }
}

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
  log::info!("open_repo: start for {path}");
  let started = Instant::now();

  let discover_start = Instant::now();
  let (id, open) = manager.open(&path)?;
  log_phase("discover", &path, discover_start.elapsed().as_millis());

  // Registering a recursive watch walks the whole working tree, so a repo with
  // a large node_modules/target directory can spend most of the open here.
  let watch_start = Instant::now();
  watchers
    .watch(app, id.clone(), &open.path)
    .map_err(AppError::Other)?;
  log_phase("watch", &path, watch_start.elapsed().as_millis());

  let head_start = Instant::now();
  let repo = open.repo.lock().unwrap();
  let name = open
    .path
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| "repository".into());
  let head_branch = head_branch(&repo);
  log_phase("head", &path, head_start.elapsed().as_millis());

  log::info!(
    "open_repo: done in {}ms (id {id}, branch {}) for {path}",
    started.elapsed().as_millis(),
    head_branch.as_deref().unwrap_or("<detached>")
  );

  Ok(RepoInfo {
    id,
    name,
    path: open.path.to_string_lossy().into_owned(),
    head_branch,
  })
}

#[tauri::command]
#[specta::specta]
pub async fn close_repo(
  manager: State<'_, RepoManager>,
  watchers: State<'_, WatcherRegistry>,
  repo_id: String,
) -> Result<(), AppError> {
  log::info!("close_repo: {repo_id}");
  watchers.unwatch(&repo_id);
  manager.close(&repo_id);
  Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn git_available() -> Result<bool, AppError> {
  Ok(crate::git::shell::git_available())
}
