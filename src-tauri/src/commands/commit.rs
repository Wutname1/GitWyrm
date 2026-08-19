use tauri::State;

use crate::error::AppError;
use crate::git::commit_write::{self, CommitIdentity};
use crate::git::trailers;
use crate::state::RepoManager;

/// Build the final commit message from what the form holds.
///
/// Split out from the command so it can be tested without a repo: this is the
/// whole of the "linked branch gets a trailer, unlinked branch does not" rule,
/// and it is the only thing standing between a spec chip appearing in the graph
/// and not.
fn compose_message(summary: &str, description: &str, spec_id: Option<&str>) -> String {
    let message = if description.trim().is_empty() {
        summary.trim().to_string()
    } else {
        format!("{}\n\n{}", summary.trim(), description.trim())
    };
    // A description the user typed may already carry the trailer (they pasted it,
    // or amended a commit that had one). upsert replaces instead of duplicating,
    // so the message ends with exactly one `Spec:` line.
    match spec_id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => trailers::upsert(&message, trailers::SPEC_KEY, id),
        None => message,
    }
}

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

    let message = compose_message(&summary, &description, spec_id.as_deref());

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

#[cfg(test)]
mod tests {
    use super::*;

    /// A commit on a linked branch has to carry the trailer, because the graph
    /// reads the trailer -- not the branch link -- when deciding to draw a spec
    /// chip. No trailer, no chip.
    #[test]
    fn a_linked_branch_gets_the_spec_trailer() {
        let message = compose_message("new: thing", "", Some("add-spec-commit-links"));
        assert_eq!(
            trailers::spec_id(&message).as_deref(),
            Some("add-spec-commit-links")
        );
    }

    #[test]
    fn the_trailer_follows_a_description_rather_than_interrupting_it() {
        let message = compose_message("new: thing", "Why it changed.", Some("add-thing"));
        assert_eq!(message, "new: thing\n\nWhy it changed.\n\nSpec: add-thing");
    }

    /// An unlinked branch must produce a byte-identical message to what the user
    /// typed. Anything else would put a chip on a commit that belongs to no
    /// change.
    #[test]
    fn an_unlinked_branch_gets_no_trailer() {
        let plain = compose_message("new: thing", "Why it changed.", None);
        assert_eq!(plain, "new: thing\n\nWhy it changed.");
        assert_eq!(trailers::spec_id(&plain), None);
    }

    /// The form sends whatever it holds; an empty or whitespace-only id is "no
    /// link", not a trailer with an empty value. `Spec:` with no id would render
    /// a chip naming nothing.
    #[test]
    fn a_blank_spec_id_is_treated_as_unlinked() {
        for blank in ["", "   ", "\t"] {
            let message = compose_message("new: thing", "", Some(blank));
            assert_eq!(
                message, "new: thing",
                "blank id {blank:?} should add nothing"
            );
            assert_eq!(trailers::spec_id(&message), None);
        }
    }

    /// Removing the trailer for one commit is a per-commit choice the form
    /// offers, so passing None must win even when the user's own description
    /// already contains a trailer block from a paste or an amend.
    #[test]
    fn a_pasted_trailer_is_not_duplicated() {
        let message = compose_message("new: thing", "Body.\n\nSpec: pasted-id", Some("real-id"));
        assert_eq!(message.matches("Spec:").count(), 1);
        assert_eq!(trailers::spec_id(&message).as_deref(), Some("real-id"));
    }
}
