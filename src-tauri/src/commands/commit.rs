use tauri::State;

use crate::error::AppError;
use crate::git::commit_write::{self, CommitIdentity};
use crate::git::trailers;
use crate::state::RepoManager;

/// Create a commit from the staged tree.
///
/// `spec_id` appends a `Spec:` trailer linking the commit to an OpenSpec
/// change. The caller passes it rather than the link being read here, so the
/// commit records exactly what the form showed -- including when the user
/// removed the trailer for this one commit.
#[tauri::command]
#[specta::specta]
pub async fn create_commit(
  manager: State<'_, RepoManager>,
  repo_id: String,
  summary: String,
  description: String,
  amend: bool,
  spec_id: Option<String>,
) -> Result<String, AppError> {
  if summary.trim().is_empty() {
    return Err(AppError::Other("commit message is required".into()));
  }
  let open = manager.get(&repo_id)?;
  let repo_path = open.path.to_string_lossy().into_owned();
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

    let mut message = if description.trim().is_empty() {
      summary.trim().to_string()
    } else {
      format!("{}\n\n{}", summary.trim(), description.trim())
    };
    // A description the user typed may already carry the trailer (they pasted
    // it, or amended a commit that had one). upsert replaces instead of
    // duplicating, so the message ends with exactly one `Spec:` line.
    if let Some(spec_id) = spec_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
      message = trailers::upsert(&message, trailers::SPEC_KEY, spec_id);
    }

    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    if amend {
      let head_commit = head_commit
        .ok_or_else(|| AppError::Other("no commit to amend".into()))?;
      // Amend rolls the staged changes into the previous commit. Keep the
      // original author; use the current signature as committer. Passing the
      // freshly written tree folds any staged changes into the amended commit.
      let identity = CommitIdentity {
        author: head_commit.author().to_owned(),
        committer: signature,
      };
      let new_oid =
        commit_write::amend_head(&repo, &repo_path, &head_commit, &identity, &message, &tree)?;
      return Ok(new_oid.to_string());
    }

    let parents: Vec<&git2::Commit> = head_commit.iter().collect();

    // Refuse empty commits (staged tree identical to HEAD tree).
    if let Some(p) = &head_commit {
      if p.tree_id() == tree_oid {
        return Err(AppError::Other("nothing staged to commit".into()));
      }
    }

    let identity = CommitIdentity {
      author: signature.clone(),
      committer: signature,
    };
    let oid = commit_write::create(
      &repo,
      &repo_path,
      Some("HEAD"),
      &identity,
      &message,
      &tree,
      &parents,
    )?;
    Ok(oid.to_string())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
