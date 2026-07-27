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
  amend: bool,
) -> Result<String, AppError> {
  if summary.trim().is_empty() {
    return Err(AppError::Other("commit message is required".into()));
  }
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let signature = repo.signature().map_err(|_| {
      AppError::Other(
        "Git does not know your name and email yet. Add them in Settings > General, then commit again.".into(),
      )
    })?;

    let mut index = repo.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let message = if description.trim().is_empty() {
      summary.trim().to_string()
    } else {
      format!("{}\n\n{}", summary.trim(), description.trim())
    };

    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    if amend {
      let head_commit = head_commit
        .ok_or_else(|| AppError::Other("no commit to amend".into()))?;
      // Amend rolls the staged changes into the previous commit. Keep the
      // original author; use the current signature as committer. Passing the
      // freshly written tree folds any staged changes into the amended commit.
      let new_oid = head_commit.amend(
        Some("HEAD"),
        Some(&head_commit.author()),
        Some(&signature),
        None,
        Some(&message),
        Some(&tree),
      )?;
      return Ok(new_oid.to_string());
    }

    let parents: Vec<&git2::Commit> = head_commit.iter().collect();

    // Refuse empty commits (staged tree identical to HEAD tree).
    if let Some(p) = &head_commit {
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
