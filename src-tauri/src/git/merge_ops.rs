//! Conflict inspection and resolution primitives. The merge commands are thin
//! wrappers around these so the integration tests exercise the real logic.

use std::path::Path;

use git2::Repository;
use serde::Deserialize;
use specta::Type;

use crate::error::AppError;
use crate::git::refs;
use crate::git::types::{ConflictContent, MergeState, OperationKind};

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

/// One conflict side's blob as raw bytes plus its binary flag. `None` when the
/// side has no entry, which means the file does not exist on that side.
fn stage_blob(
  repo: &Repository,
  index: &git2::Index,
  path: &str,
  stage: i32,
) -> Result<Option<(Vec<u8>, bool)>, AppError> {
  let Some(entry) = index.get_path(Path::new(path), stage) else {
    return Ok(None);
  };
  let blob = repo.find_blob(entry.id)?;
  Ok(Some((blob.content().to_vec(), blob.is_binary())))
}

/// Display text for one side: empty when absent or binary.
fn side_text(side: &Option<(Vec<u8>, bool)>) -> String {
  match side {
    Some((bytes, false)) => String::from_utf8_lossy(bytes).into_owned(),
    _ => String::new(),
  }
}

/// The in-progress operation the repo's state files describe, if any. A repo can
/// hold conflicts without being in one of these -- see `merge_state`.
fn operation_kind(repo: &Repository) -> Option<OperationKind> {
  match repo.state() {
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
  }
}

/// What the repo is in the middle of, plus every path still conflicted.
///
/// The conflicted paths are read from the index in every case, NOT only while an
/// operation is in progress. Applying a stash (including the auto-stash a branch
/// switch does), `checkout -m`, and a three-way patch apply all leave unmerged
/// index entries behind with no MERGE_HEAD and no rebase directory, so the repo
/// state reads as Clean. Gating the paths on the operation made those conflicts
/// invisible: the changes list marked the file `conflict` from `git status`
/// while the conflict view -- which reads this -- insisted there was nothing to
/// resolve. `merging` stays false for them, since there is no merge to commit or
/// abort; only the conflicts themselves need resolving.
pub fn merge_state(repo: &Repository) -> Result<MergeState, AppError> {
  let conflicts = refs::conflicted_paths(repo)?;

  let Some(operation) = operation_kind(repo) else {
    return Ok(MergeState {
      merging: false,
      operation: None,
      incoming_label: None,
      full_message: None,
      conflicts,
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
    // Merge, cherry-pick, and revert leave the intended message in MERGE_MSG;
    // its first line is the display label, the whole file is the message to
    // commit with.
    let msg = std::fs::read_to_string(repo.path().join("MERGE_MSG")).ok();
    let label = msg.as_deref().and_then(|m| m.lines().next().map(str::to_string));
    (label, msg.map(|m| m.trim_end().to_string()))
  };

  Ok(MergeState {
    merging: true,
    operation: Some(operation),
    incoming_label,
    full_message,
    conflicts,
  })
}

/// Read the three sides of a conflicted file plus the marker text on disk.
pub fn conflict_content(
  repo: &Repository,
  workdir: &Path,
  path: &str,
) -> Result<ConflictContent, AppError> {
  let index = repo.index()?;

  let base = stage_blob(repo, &index, path, 1)?;
  let ours = stage_blob(repo, &index, path, 2)?;
  let theirs = stage_blob(repo, &index, path, 3)?;

  let binary = [&base, &ours, &theirs]
    .iter()
    .any(|s| matches!(s, Some((_, true))));

  // Working-tree copy carries the conflict markers for manual editing.
  let merged = crate::commands::file::resolve_in_repo(workdir, path)
    .ok()
    .and_then(|p| std::fs::read_to_string(p).ok())
    .unwrap_or_default();

  Ok(ConflictContent {
    path: path.to_string(),
    base: side_text(&base),
    ours: side_text(&ours),
    theirs: side_text(&theirs),
    merged,
    binary,
    ours_deleted: ours.is_none(),
    theirs_deleted: theirs.is_none(),
  })
}

/// Apply a resolution for one conflicted path: write the chosen content to the
/// working tree (or delete the file when the chosen side deleted it), clear the
/// conflict from the index, and stage the result.
pub fn apply_resolution(
  repo: &Repository,
  workdir: &Path,
  path: &str,
  resolution: &Resolution,
) -> Result<(), AppError> {
  let mut index = repo.index()?;
  // `path` crosses the IPC boundary, so confine it before any write. On Windows
  // an absolute `path` would otherwise replace `workdir` entirely in `join`.
  let target = crate::commands::file::resolve_in_repo(workdir, path)?;
  let rel = Path::new(path);

  // Raw blob bytes so binary and non-UTF-8 files round-trip untouched.
  let bytes: Option<Vec<u8>> = match resolution {
    Resolution::Ours => stage_blob(repo, &index, path, 2)?.map(|(b, _)| b),
    Resolution::Theirs => stage_blob(repo, &index, path, 3)?.map(|(b, _)| b),
    Resolution::Manual { text } => Some(text.clone().into_bytes()),
  };

  match bytes {
    Some(bytes) => {
      std::fs::write(&target, &bytes).map_err(AppError::Io)?;
      index.remove_path(rel)?;
      index.add_path(rel)?;
    }
    None => {
      // The chosen side deleted the file: remove it and stage the deletion.
      match std::fs::remove_file(&target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(AppError::Io(e)),
      }
      index.remove_path(rel)?;
    }
  }

  index.write()?;
  Ok(())
}
