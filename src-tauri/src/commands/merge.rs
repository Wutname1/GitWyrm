//! Merge and conflict resolution. Uses git2 for the local index work; a merge
//! never touches the network, so no shell-out is required here.

use std::path::Path;

use git2::{build::CheckoutBuilder, MergeOptions, Oid, ResetType};
use serde::Deserialize;
use specta::Type;
use tauri::State;

use crate::error::AppError;
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

/// True when the working tree has tracked modifications (ignores untracked).
fn working_tree_dirty(repo: &git2::Repository) -> Result<bool, AppError> {
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(false);
  Ok(repo.statuses(Some(&mut opts))?.iter().any(|e| !e.status().is_ignored()))
}

/// Read a state pointer file (MERGE_HEAD / CHERRY_PICK_HEAD) as an Oid.
fn read_state_head(repo: &git2::Repository, file: &str) -> Result<Oid, AppError> {
  let content = std::fs::read_to_string(repo.path().join(file))
    .map_err(|_| AppError::Other("no operation in progress".into()))?;
  Oid::from_str(content.trim()).map_err(AppError::Git)
}

/// List paths currently conflicted in the index.
fn conflicted_paths(repo: &git2::Repository) -> Result<Vec<String>, AppError> {
  let index = repo.index()?;
  if !index.has_conflicts() {
    return Ok(Vec::new());
  }
  let mut paths = Vec::new();
  for entry in index.conflicts()? {
    let entry = entry?;
    // Prefer our side path, then theirs, then ancestor.
    let raw = entry
      .our
      .as_ref()
      .or(entry.their.as_ref())
      .or(entry.ancestor.as_ref())
      .map(|e| e.path.clone());
    if let Some(bytes) = raw {
      paths.push(String::from_utf8_lossy(&bytes).into_owned());
    }
  }
  paths.sort();
  paths.dedup();
  Ok(paths)
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
    let annotated = resolve_annotated(&repo, &reference)?;
    let (analysis, _pref) = repo.merge_analysis(&[&annotated])?;

    if analysis.is_up_to_date() {
      return Ok(MergeResult { up_to_date: true, fast_forwarded: false, conflicts: Vec::new() });
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

    let conflicts = conflicted_paths(&repo)?;
    Ok(MergeResult { up_to_date: false, fast_forwarded: false, conflicts })
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
      git2::RepositoryState::CherryPick => Some(OperationKind::CherryPick),
      _ => None,
    };
    let Some(operation) = operation else {
      return Ok(MergeState {
        merging: false,
        operation: None,
        incoming_label: None,
        conflicts: Vec::new(),
      });
    };

    // Both merge and cherry-pick leave the intended message in MERGE_MSG; its
    // first line is "Merge branch 'x'" for a merge or the picked commit summary.
    let incoming_label = std::fs::read_to_string(repo.path().join("MERGE_MSG"))
      .ok()
      .and_then(|msg| msg.lines().next().map(str::to_string));

    let conflicts = conflicted_paths(&repo)?;
    Ok(MergeState { merging: true, operation: Some(operation), incoming_label, conflicts })
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

/// Read one index stage's blob for a path as UTF-8 text (empty if absent/binary).
fn stage_text(repo: &git2::Repository, index: &git2::Index, path: &str, stage: i32) -> (String, bool) {
  let Some(entry) = index.get_path(Path::new(path), stage) else {
    return (String::new(), false);
  };
  match repo.find_blob(entry.id) {
    Ok(blob) => {
      if blob.is_binary() {
        (String::new(), true)
      } else {
        (String::from_utf8_lossy(blob.content()).into_owned(), false)
      }
    }
    Err(_) => (String::new(), false),
  }
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
    let index = repo.index()?;

    let (base, b_bin) = stage_text(&repo, &index, &path, 1);
    let (ours, o_bin) = stage_text(&repo, &index, &path, 2);
    let (theirs, t_bin) = stage_text(&repo, &index, &path, 3);

    // Working-tree copy carries the conflict markers for manual editing.
    let merged = std::fs::read_to_string(workdir.join(&path)).unwrap_or_default();

    Ok(ConflictContent {
      path,
      base,
      ours,
      theirs,
      merged,
      binary: b_bin || o_bin || t_bin,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Resolution {
  /// Keep our side wholesale.
  Ours,
  /// Keep their side wholesale.
  Theirs,
  /// Use the provided, hand-edited text.
  Manual { text: String },
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
    let mut index = repo.index()?;
    let rel = Path::new(&path);

    // Decide the resolved bytes.
    let text = match &resolution {
      Resolution::Ours => stage_text(&repo, &index, &path, 2).0,
      Resolution::Theirs => stage_text(&repo, &index, &path, 3).0,
      Resolution::Manual { text } => text.clone(),
    };

    // Write to the working tree, then clear the conflict and stage the result.
    std::fs::write(workdir.join(rel), text.as_bytes()).map_err(AppError::Io)?;
    index.remove_path(rel)?;
    index.add_path(rel)?;
    index.write()?;
    Ok(())
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

    let mut index = repo.index()?;
    if index.has_conflicts() {
      return Err(AppError::Other("resolve all conflicts before committing".into()));
    }

    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;
    let committer = repo.signature()?;
    let head_commit = repo.head()?.peel_to_commit()?;

    // A cherry-pick produces a single-parent commit that keeps the picked
    // commit's original author; a merge produces a two-parent merge commit.
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

    if working_tree_dirty(&repo)? {
      return Err(AppError::Other(
        "working tree has changes; commit or stash before cherry-picking".into(),
      ));
    }

    let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    let commit = repo.find_commit(oid)?;

    repo.cherrypick(&commit, None)?;

    let conflicts = conflicted_paths(&repo)?;
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
