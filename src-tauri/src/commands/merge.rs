//! Merge and conflict resolution. Uses git2 for the local index work; a merge
//! never touches the network, so no shell-out is required here.

use git2::{build::CheckoutBuilder, MergeOptions, Oid, ResetType};
use tauri::State;

use crate::error::AppError;
use crate::git::merge_ops::{self, Resolution};
use crate::git::refs;
use crate::git::types::{
  ConflictContent, MergeAnalysis, MergeResult, MergeState, OperationKind,
};
use crate::state::RepoManager;

/// Resolve a ref name (branch, remote branch, tag, or sha) to an annotated commit.
fn resolve_annotated<'r>(
  repo: &'r git2::Repository,
  reference: &str,
) -> Result<git2::AnnotatedCommit<'r>, AppError> {
  // Try as a real reference first so we can carry its name into the merge.
  if let Ok((obj, Some(git_ref))) = repo.revparse_ext(reference) {
    if let Ok(ann) = repo.reference_to_annotated_commit(&git_ref) {
      return Ok(ann);
    }
    let oid = obj.id();
    return Ok(repo.find_annotated_commit(oid)?);
  }
  let obj = repo.revparse_single(reference)?;
  Ok(repo.find_annotated_commit(obj.id())?)
}

/// Read a state pointer file (MERGE_HEAD / CHERRY_PICK_HEAD) as an Oid.
fn read_state_head(repo: &git2::Repository, file: &str) -> Result<Oid, AppError> {
  let content = std::fs::read_to_string(repo.path().join(file))
    .map_err(|_| AppError::Other("no operation in progress".into()))?;
  Oid::from_str(content.trim()).map_err(AppError::Git)
}

#[tauri::command]
#[specta::specta]
pub async fn merge_analysis(
  manager: State<'_, RepoManager>,
  repo_id: String,
  reference: String,
) -> Result<MergeAnalysis, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let annotated = resolve_annotated(&repo, &reference)?;
    let (analysis, _pref) = repo.merge_analysis(&[&annotated])?;
    Ok(MergeAnalysis {
      up_to_date: analysis.is_up_to_date(),
      can_fast_forward: analysis.is_fast_forward(),
      normal: analysis.is_normal(),
      target_sha: annotated.id().to_string()[..7].to_string(),
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Merge `reference` into the current HEAD. Fast-forwards when possible,
/// otherwise leaves conflicts in the index for the frontend to resolve.
fn do_merge(repo: &git2::Repository, reference: &str) -> Result<MergeResult, AppError> {
  let annotated = resolve_annotated(repo, reference)?;
  let (analysis, _pref) = repo.merge_analysis(&[&annotated])?;

  if analysis.is_up_to_date() {
    return Ok(MergeResult { up_to_date: true, fast_forwarded: false, conflicts: Vec::new() });
  }

  // Anything past this point rewrites the working tree; refuse over
  // uncommitted work so a half-applied merge can't eat local changes.
  if refs::tracked_changes_present(repo)? {
    return Err(AppError::Other(
      "working tree has changes; commit or stash before merging".into(),
    ));
  }

  // Fast-forward: move HEAD and checkout, no merge commit.
  if analysis.is_fast_forward() {
    let target_oid = annotated.id();
    let target = repo.find_object(target_oid, None)?;
    repo.checkout_tree(&target, Some(CheckoutBuilder::new().safe()))?;
    match repo.head()?.name() {
      Some(head_ref) => {
        repo.reference(head_ref, target_oid, true, "fast-forward merge")?;
      }
      None => repo.set_head_detached(target_oid)?,
    }
    return Ok(MergeResult { up_to_date: false, fast_forwarded: true, conflicts: Vec::new() });
  }

  // Normal merge: write conflicts into the working tree/index. The frontend
  // resolves them and creates the merge commit via the normal commit flow.
  let mut opts = MergeOptions::new();
  let mut checkout = CheckoutBuilder::new();
  checkout.allow_conflicts(true).conflict_style_merge(true);
  repo.merge(&[&annotated], Some(&mut opts), Some(&mut checkout))?;

  let conflicts = refs::conflicted_paths(repo)?;
  Ok(MergeResult { up_to_date: false, fast_forwarded: false, conflicts })
}

#[tauri::command]
#[specta::specta]
pub async fn merge_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  reference: String,
) -> Result<MergeResult, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    do_merge(&repo, &reference)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Merge `source` into `target`, checking out `target` first when it isn't
/// already HEAD. Refuses to switch branches over a dirty tree so no work is
/// lost. This backs the direction modal's reverse ("merge current into other").
#[tauri::command]
#[specta::specta]
pub async fn merge_directional(
  manager: State<'_, RepoManager>,
  repo_id: String,
  target: String,
  source: String,
) -> Result<MergeResult, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let on_target = repo.head().ok().and_then(|h| h.shorthand().map(str::to_string))
      == Some(target.clone());

    if !on_target {
      if refs::tracked_changes_present(&repo)? {
        return Err(AppError::Other(
          "working tree has changes; commit or stash before switching to merge".into(),
        ));
      }
      let (object, reference) = repo.revparse_ext(&target)?;
      repo.checkout_tree(&object, None)?;
      match reference {
        Some(r) => repo.set_head(r.name().unwrap_or("HEAD"))?,
        None => repo.set_head_detached(object.id())?,
      }
    }

    do_merge(&repo, &source)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn get_merge_state(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<MergeState, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let operation = match repo.state() {
      git2::RepositoryState::Merge => Some(OperationKind::Merge),
      git2::RepositoryState::CherryPick | git2::RepositoryState::CherryPickSequence => {
        Some(OperationKind::CherryPick)
      }
      git2::RepositoryState::Revert | git2::RepositoryState::RevertSequence => {
        Some(OperationKind::Revert)
      }
      git2::RepositoryState::Rebase
      | git2::RepositoryState::RebaseMerge
      | git2::RepositoryState::RebaseInteractive => Some(OperationKind::Rebase),
      _ => None,
    };
    let Some(operation) = operation else {
      return Ok(MergeState {
        merging: false,
        operation: None,
        incoming_label: None,
        full_message: None,
        conflicts: Vec::new(),
      });
    };

    let (incoming_label, full_message) = if operation == OperationKind::Rebase {
      // The message of the commit the rebase stopped on, else the branch being
      // rebased. Finishing a rebase reuses each commit's message, so there is
      // no full_message to carry.
      let label = std::fs::read_to_string(repo.path().join("rebase-merge/message"))
        .ok()
        .and_then(|msg| msg.lines().next().map(str::to_string))
        .or_else(|| {
          std::fs::read_to_string(repo.path().join("rebase-merge/head-name"))
            .ok()
            .map(|name| name.trim().trim_start_matches("refs/heads/").to_string())
        });
      (label, None)
    } else {
      // Merge, cherry-pick, and revert leave the intended message in
      // MERGE_MSG; its first line is the display label, the whole file is the
      // message to commit with.
      let msg = std::fs::read_to_string(repo.path().join("MERGE_MSG")).ok();
      let label = msg.as_deref().and_then(|m| m.lines().next().map(str::to_string));
      (label, msg.map(|m| m.trim_end().to_string()))
    };

    let conflicts = refs::conflicted_paths(&repo)?;
    Ok(MergeState {
      merging: true,
      operation: Some(operation),
      incoming_label,
      full_message,
      conflicts,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn abort_merge(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    if matches!(
      repo.state(),
      git2::RepositoryState::Rebase
        | git2::RepositoryState::RebaseMerge
        | git2::RepositoryState::RebaseInteractive
    ) {
      return Err(AppError::Other(
        "a rebase is in progress; abort the rebase instead".into(),
      ));
    }
    // Reset hard to HEAD and clear the operation state. HEAD hasn't moved for
    // an in-progress merge or cherry-pick, so this mirrors both
    // `git merge --abort` and `git cherry-pick --abort`.
    let head_commit = repo
      .head()?
      .peel_to_commit()
      .map_err(|_| AppError::Other("no HEAD commit to reset to".into()))?;
    repo.reset(head_commit.as_object(), ResetType::Hard, Some(CheckoutBuilder::new().force()))?;
    repo.cleanup_state()?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn get_conflict(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<ConflictContent, AppError> {
  let open = manager.get(&repo_id)?;
  let workdir = open.path.clone();
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    merge_ops::conflict_content(&repo, &workdir, &path)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn resolve_conflict(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  resolution: Resolution,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let workdir = open.path.clone();
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    merge_ops::apply_resolution(&repo, &workdir, &path, &resolution)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn commit_merge(
  manager: State<'_, RepoManager>,
  repo_id: String,
  message: String,
) -> Result<String, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    if matches!(
      repo.state(),
      git2::RepositoryState::Rebase
        | git2::RepositoryState::RebaseMerge
        | git2::RepositoryState::RebaseInteractive
    ) {
      return Err(AppError::Other(
        "a rebase is in progress; continue the rebase instead of committing".into(),
      ));
    }

    let mut index = repo.index()?;
    if index.has_conflicts() {
      return Err(AppError::Other("resolve all conflicts before committing".into()));
    }

    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;
    let committer = repo.signature()?;
    let head_commit = repo.head()?.peel_to_commit()?;

    // A cherry-pick produces a single-parent commit that keeps the picked
    // commit's original author; a revert is single-parent but authored by the
    // reverter; a merge produces a two-parent merge commit.
    let oid = match repo.state() {
      git2::RepositoryState::CherryPick => {
        let picked_oid = read_state_head(&repo, "CHERRY_PICK_HEAD")?;
        let picked = repo.find_commit(picked_oid)?;
        let author = picked.author();
        repo.commit(
          Some("HEAD"),
          &author,
          &committer,
          &message,
          &tree,
          &[&head_commit],
        )?
      }
      git2::RepositoryState::Revert => repo.commit(
        Some("HEAD"),
        &committer,
        &committer,
        &message,
        &tree,
        &[&head_commit],
      )?,
      _ => {
        let merge_head_oid = read_state_head(&repo, "MERGE_HEAD")?;
        let merge_commit = repo.find_commit(merge_head_oid)?;
        repo.commit(
          Some("HEAD"),
          &committer,
          &committer,
          &message,
          &tree,
          &[&head_commit, &merge_commit],
        )?
      }
    };

    repo.cleanup_state()?;
    Ok(oid.to_string())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Cherry-pick a commit onto the current branch. A clean apply commits
/// immediately (single-parent, keeping the original author) and returns no
/// conflicts. A conflicting apply leaves CHERRY_PICK_HEAD and the conflicted
/// index in place for the operation-aware conflict flow to resolve and finish.
#[tauri::command]
#[specta::specta]
pub async fn cherry_pick(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
) -> Result<MergeResult, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    if refs::tracked_changes_present(&repo)? {
      return Err(AppError::Other(
        "working tree has changes; commit or stash before cherry-picking".into(),
      ));
    }

    let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    let commit = repo.find_commit(oid)?;

    repo.cherrypick(&commit, None)?;

    let conflicts = refs::conflicted_paths(&repo)?;
    if !conflicts.is_empty() {
      return Ok(MergeResult { up_to_date: false, fast_forwarded: false, conflicts });
    }

    // Clean apply: commit the picked changes as a single-parent commit that
    // preserves the original author, then clear the cherry-pick state.
    let mut index = repo.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;
    let committer = repo.signature()?;
    let head_commit = repo.head()?.peel_to_commit()?;
    let message = commit.message().unwrap_or("");
    repo.commit(
      Some("HEAD"),
      &commit.author(),
      &committer,
      message,
      &tree,
      &[&head_commit],
    )?;
    repo.cleanup_state()?;

    Ok(MergeResult { up_to_date: false, fast_forwarded: false, conflicts: Vec::new() })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
