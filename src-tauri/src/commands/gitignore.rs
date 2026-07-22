use std::fs;
use std::path::Path;

use tauri::State;

use crate::error::AppError;
use crate::state::RepoManager;

/// What happened when a pattern was added, so the UI can say something true
/// rather than always claiming a line was written.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum IgnoreOutcome {
  Added,
  AlreadyPresent,
}

/// Append `pattern` to the repository's top-level .gitignore, creating the file
/// when it does not exist yet. Existing content is never rewritten: the pattern
/// goes on its own line at the end, with a newline first if the file did not
/// end with one.
fn append_pattern(root: &Path, pattern: &str) -> Result<IgnoreOutcome, std::io::Error> {
  let file = root.join(".gitignore");
  let existing = match fs::read_to_string(&file) {
    Ok(text) => text,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
    Err(e) => return Err(e),
  };

  if existing.lines().any(|line| line.trim() == pattern) {
    return Ok(IgnoreOutcome::AlreadyPresent);
  }

  let mut next = existing;
  if !next.is_empty() && !next.ends_with('\n') {
    next.push('\n');
  }
  next.push_str(pattern);
  next.push('\n');
  fs::write(&file, next)?;
  Ok(IgnoreOutcome::Added)
}

/// Add one pattern to the repository's .gitignore.
#[tauri::command]
#[specta::specta]
pub async fn add_to_gitignore(
  manager: State<'_, RepoManager>,
  repo_id: String,
  pattern: String,
) -> Result<IgnoreOutcome, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    append_pattern(&open.path, &pattern).map_err(|e| AppError::Other(e.to_string()))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_file_then_appends_and_detects_duplicates() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();

    assert!(matches!(append_pattern(root, "*.ps1"), Ok(IgnoreOutcome::Added)));
    assert_eq!(fs::read_to_string(root.join(".gitignore")).expect("read"), "*.ps1\n");

    assert!(matches!(append_pattern(root, "test/"), Ok(IgnoreOutcome::Added)));
    assert_eq!(
      fs::read_to_string(root.join(".gitignore")).expect("read"),
      "*.ps1\ntest/\n"
    );

    assert!(matches!(append_pattern(root, "*.ps1"), Ok(IgnoreOutcome::AlreadyPresent)));
  }

  #[test]
  fn adds_missing_newline_before_appending() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::write(root.join(".gitignore"), "node_modules").expect("seed file");

    append_pattern(root, "*.log").expect("append");
    assert_eq!(
      fs::read_to_string(root.join(".gitignore")).expect("read"),
      "node_modules\n*.log\n"
    );
  }
}
