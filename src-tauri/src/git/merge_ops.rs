//! Conflict inspection and resolution primitives. The merge commands are thin
//! wrappers around these so the integration tests exercise the real logic.

use std::path::Path;

use git2::Repository;
use serde::Deserialize;
use specta::Type;

use crate::error::AppError;
use crate::git::types::ConflictContent;

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
  let merged = std::fs::read_to_string(workdir.join(path)).unwrap_or_default();

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
  let rel = Path::new(path);

  // Raw blob bytes so binary and non-UTF-8 files round-trip untouched.
  let bytes: Option<Vec<u8>> = match resolution {
    Resolution::Ours => stage_blob(repo, &index, path, 2)?.map(|(b, _)| b),
    Resolution::Theirs => stage_blob(repo, &index, path, 3)?.map(|(b, _)| b),
    Resolution::Manual { text } => Some(text.clone().into_bytes()),
  };

  match bytes {
    Some(bytes) => {
      std::fs::write(workdir.join(rel), &bytes).map_err(AppError::Io)?;
      index.remove_path(rel)?;
      index.add_path(rel)?;
    }
    None => {
      // The chosen side deleted the file: remove it and stage the deletion.
      match std::fs::remove_file(workdir.join(rel)) {
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
