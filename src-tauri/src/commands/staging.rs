use std::path::Path;

use tauri::State;

use crate::error::AppError;
use crate::state::RepoManager;

#[tauri::command]
#[specta::specta]
pub async fn stage_file(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut index = repo.index()?;
    let rel = Path::new(&path);
    if open.path.join(rel).exists() {
      index.add_path(rel)?;
    } else {
      // Deleted in workdir: stage the deletion.
      index.remove_path(rel)?;
    }
    index.write()?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn unstage_file(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let head = repo.head().ok().and_then(|h| h.peel(git2::ObjectType::Commit).ok());
    match head {
      Some(head) => repo.reset_default(Some(&head), [&path])?,
      // No commits yet: unstage = remove from index entirely.
      None => {
        let mut index = repo.index()?;
        index.remove_path(Path::new(&path))?;
        index.write()?;
      }
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn stage_all(manager: State<'_, RepoManager>, repo_id: String) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut index = repo.index()?;
    index.add_all(["*"], git2::IndexAddOption::DEFAULT, None)?;
    index.update_all(["*"], None)?;
    index.write()?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn unstage_all(manager: State<'_, RepoManager>, repo_id: String) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    if let Some(head) = repo.head().ok().and_then(|h| h.peel(git2::ObjectType::Commit).ok()) {
      repo.reset_default(Some(&head), ["*"])?;
    } else {
      let mut index = repo.index()?;
      index.clear()?;
      index.write()?;
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn discard_file(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.path(&path).force().remove_untracked(true);
    repo.checkout_head(Some(&mut builder))?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
