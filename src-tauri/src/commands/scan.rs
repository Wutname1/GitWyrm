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
    head.trim()
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

/// Filenames treated as the repository's readme, in the order they win.
const README_NAMES: [&str; 4] = ["README.md", "README.markdown", "README.mdown", "README"];

/// Cap on how much readme text crosses to the UI. Long enough for a real
/// project page, short enough that a giant generated file cannot stall the
/// preview panel.
const README_MAX_BYTES: usize = 120_000;

/// Reads a repository's readme as text, or None when it has none.
///
/// Matching is case-insensitive because Windows and Linux checkouts disagree on
/// `README.md` vs `readme.md`. Only files directly in the repository root are
/// considered, and nothing here opens a git handle, so this is safe to call for
/// repositories the user has not opened.
#[tauri::command]
#[specta::specta]
pub async fn read_repo_readme(path: String) -> Result<Option<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&path);
        if !root.is_dir() {
            return Ok(None);
        }
        for candidate in README_NAMES {
            let Some(file) = find_case_insensitive(root, candidate) else {
                continue;
            };
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            return Ok(Some(truncate_on_char_boundary(text, README_MAX_BYTES)));
        }
        Ok(None)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Finds a file in `root` whose name equals `name` ignoring case.
fn find_case_insensitive(root: &Path, name: &str) -> Option<std::path::PathBuf> {
    let direct = root.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let wanted = name.to_lowercase();
    fs::read_dir(root).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        let matches = entry.file_name().to_string_lossy().to_lowercase() == wanted;
        (matches && path.is_file()).then_some(path)
    })
}

/// Trims `text` to at most `max_bytes`, stepping back to a char boundary so the
/// result is always valid UTF-8.
fn truncate_on_char_boundary(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut cut = max_bytes;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text[..cut].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_readme_regardless_of_case() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("readme.MD"), "# hi").expect("write");
        let found = find_case_insensitive(dir.path(), "README.md").expect("found");
        assert_eq!(fs::read_to_string(found).expect("read"), "# hi");
    }

    #[test]
    fn missing_readme_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(find_case_insensitive(dir.path(), "README.md").is_none());
    }

    #[test]
    fn truncation_keeps_valid_utf8() {
        let text = "a".repeat(9) + "é";
        let cut = truncate_on_char_boundary(text, 10);
        assert_eq!(cut, "a".repeat(9));
    }
}
