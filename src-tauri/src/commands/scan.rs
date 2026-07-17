//! Scans a "code folder" for git repositories WITHOUT opening them.
//! Reads only directory metadata and HEAD as plain text, so no repository
//! handle or lock is ever taken on repos the user has not opened.

use std::fs;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Type)]
pub struct ScannedRepo {
  pub name: String,
  pub path: String,
  /// Current branch parsed from .git/HEAD as text (None when detached/unreadable).
  pub head_branch: Option<String>,
}

fn read_head_branch(git_dir: &Path) -> Option<String> {
  let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
  head
    .trim()
    .strip_prefix("ref: refs/heads/")
    .map(str::to_string)
}

#[tauri::command]
#[specta::specta]
pub async fn scan_code_folder(folder: String) -> Result<Vec<ScannedRepo>, AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    let root = Path::new(&folder);
    if !root.is_dir() {
      return Err(AppError::Other(format!("not a directory: {folder}")));
    }

    let mut repos = Vec::new();
    for entry in fs::read_dir(root)?.flatten() {
      let path = entry.path();
      if !path.is_dir() {
        continue;
      }
      let git_dir = path.join(".git");
      if !git_dir.is_dir() {
        continue;
      }
      let name = entry.file_name().to_string_lossy().into_owned();
      repos.push(ScannedRepo {
        head_branch: read_head_branch(&git_dir),
        path: path.to_string_lossy().into_owned(),
        name,
      });
    }
    repos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(repos)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
