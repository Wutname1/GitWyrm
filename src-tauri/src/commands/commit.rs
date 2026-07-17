use tauri::State;

use crate::error::AppError;
use crate::state::RepoManager;

#[tauri::command]
#[specta::specta]
pub async fn create_commit(
  manager: State<'_, RepoManager>,
  repo_id: String,
  summary: String,
  description: String,
) -> Result<String, AppError> {
  if summary.trim().is_empty() {
    return Err(AppError::Other("commit message is required".into()));
  }
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let signature = repo.signature().map_err(|_| {
      AppError::Other("git user.name / user.email are not configured".into())
    })?;

    let mut index = repo.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let message = if description.trim().is_empty() {
      summary.trim().to_string()
    } else {
      format!("{}\n\n{}", summary.trim(), description.trim())
    };

    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();

    // Refuse empty commits (staged tree identical to HEAD tree).
    if let Some(p) = &parent {
      if p.tree_id() == tree_oid {
        return Err(AppError::Other("nothing staged to commit".into()));
      }
    }

    let oid = repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &parents)?;
    Ok(oid.to_string())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
