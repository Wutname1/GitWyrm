use tauri::State;

use crate::error::AppError;
use crate::git::types::StashInfo;
use crate::state::RepoManager;

#[tauri::command]
#[specta::specta]
pub async fn stash_save(
  manager: State<'_, RepoManager>,
  repo_id: String,
  message: Option<String>,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();
    let signature = repo.signature()?;
    let msg = message.unwrap_or_else(|| "WIP".into());
    repo.stash_save(&signature, &msg, Some(git2::StashFlags::INCLUDE_UNTRACKED))?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn stash_pop(
  manager: State<'_, RepoManager>,
  repo_id: String,
  index: u32,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();
    repo.stash_pop(index as usize, None)?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn list_stashes(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<StashInfo>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();
    let mut stashes = Vec::new();
    repo.stash_foreach(|index, message, _oid| {
      stashes.push(StashInfo { index: index as u32, message: message.to_string() });
      true
    })?;
    Ok(stashes)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
