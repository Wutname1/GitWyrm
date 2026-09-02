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

/// A local repository that already has the remote the user pasted.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RemoteMatch {
    pub name: String,
    pub path: String,
    /// The name of the matching remote, e.g. `origin`.
    pub remote: String,
    /// That remote's URL exactly as the repository has it configured.
    pub url: String,
}

/// Resolves the directory holding `config` for a repository checkout.
///
/// A normal checkout has a `.git` directory. A worktree or submodule has a
/// `.git` FILE pointing elsewhere with `gitdir: <path>`; that target still
/// holds the config (or, for a worktree, a `commondir` beside it does), so
/// following the pointer is what keeps those repositories searchable.
fn git_dir_for(root: &Path) -> Option<std::path::PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let pointer = fs::read_to_string(&dot_git).ok()?;
    let target = pointer.trim().strip_prefix("gitdir:")?.trim();
    let target = root.join(target);
    if !target.is_dir() {
        return None;
    }
    // A linked worktree's own gitdir has no remotes; `commondir` names the
    // repository that does.
    if let Ok(common) = fs::read_to_string(target.join("commondir")) {
        let common = target.join(common.trim());
        if common.is_dir() {
            return Some(common);
        }
    }
    Some(target)
}

/// Reads every `[remote "name"] url = ...` pair out of a git config file.
///
/// Deliberately a text scan rather than a repository handle: this runs over
/// every repository the user has ever seen, none of which are open, and taking
/// a handle on each would lock them and stall the picker.
fn read_remotes(config: &Path) -> Vec<(String, String)> {
    let Ok(text) = fs::read_to_string(config) else {
        return Vec::new();
    };
    let mut remotes = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = section
                .trim()
                .strip_prefix("remote")
                .map(str::trim)
                .and_then(|name| name.strip_prefix('"'))
                .and_then(|name| name.strip_suffix('"'))
                .map(str::to_string);
            continue;
        }
        let Some(name) = current.as_deref() else {
            continue;
        };
        if let Some((key, value)) = line.split_once('=') {
            if key.trim().eq_ignore_ascii_case("url") {
                remotes.push((name.to_string(), value.trim().to_string()));
            }
        }
    }
    remotes
}

/// Finds which of `paths` already have `url` as one of their remotes.
///
/// Answers "do I already have this?" when a clone URL is pasted into the
/// picker's search box, so the user opens the copy on disk instead of making a
/// second one. Matching goes through `git::remote_url`, so the pasted URL does
/// not have to be written the same way the repository has it: `.git` or not,
/// https or ssh, any case.
#[tauri::command]
#[specta::specta]
pub async fn find_repos_with_remote(
    url: String,
    paths: Vec<String>,
) -> Result<Vec<RemoteMatch>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        if crate::git::remote_url::parse(&url).is_none() {
            return Ok(Vec::new());
        }
        let mut matches = Vec::new();
        for path in paths {
            let root = Path::new(&path);
            let Some(git_dir) = git_dir_for(root) else {
                continue;
            };
            let found = read_remotes(&git_dir.join("config"))
                .into_iter()
                .find(|(_, candidate)| crate::git::remote_url::same_repository(&url, candidate));
            if let Some((remote, matched_url)) = found {
                matches.push(RemoteMatch {
                    name: root
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone()),
                    path: root.to_string_lossy().into_owned(),
                    url: matched_url,
                    remote,
                });
            }
        }
        Ok(matches)
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
    fn reads_every_remote_url_out_of_a_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("config");
        fs::write(
            &config,
            "[core]
	url = not-a-remote
[remote \"origin\"]
	url = https://github.com/o/r.git
	fetch = +refs/heads/*
[remote \"fork\"]
	URL = git@github.com:me/r.git
",
        )
        .expect("write");
        assert_eq!(
            read_remotes(&config),
            vec![
                ("origin".to_string(), "https://github.com/o/r.git".to_string()),
                ("fork".to_string(), "git@github.com:me/r.git".to_string()),
            ]
        );
    }

    #[test]
    fn a_config_without_remotes_reads_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("config");
        fs::write(&config, "[core]
	bare = false
").expect("write");
        assert!(read_remotes(&config).is_empty());
        assert!(read_remotes(&dir.path().join("missing")).is_empty());
    }

    /// Proves the whole chain on real repositories: a checkout's own config
    /// is read, its origin recognised through `git::remote_url`, and an
    /// unrelated address matches nothing.
    #[test]
    fn matches_a_real_checkout_by_its_origin_url() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("gitwyrm");
        let git = root.join(".git");
        fs::create_dir_all(&git).expect("dirs");
        fs::write(
            git.join("config"),
            "[remote \"origin\"]
	url = git@github.com:Wutname1/GitWyrm.git
",
        )
        .expect("write");

        let git_dir = git_dir_for(&root).expect("git dir");
        let remotes = read_remotes(&git_dir.join("config"));
        assert!(remotes.iter().any(|(name, url)| name == "origin"
            && crate::git::remote_url::same_repository(
                "https://github.com/Wutname1/gitwyrm",
                url
            )));
        assert!(!remotes.iter().any(|(_, url)| crate::git::remote_url::same_repository(
            "https://github.com/someone/else",
            url
        )));
    }

    #[test]
    fn a_worktree_pointer_resolves_to_the_shared_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let main = dir.path().join("main/.git");
        let worktree_git = main.join("worktrees/wt");
        fs::create_dir_all(&worktree_git).expect("dirs");
        let checkout = dir.path().join("wt");
        fs::create_dir_all(&checkout).expect("dirs");
        fs::write(
            checkout.join(".git"),
            format!("gitdir: {}
", worktree_git.to_string_lossy()),
        )
        .expect("write");
        fs::write(worktree_git.join("commondir"), "../..
").expect("write");

        let resolved = git_dir_for(&checkout).expect("resolved");
        assert_eq!(
            fs::canonicalize(resolved).expect("canonical"),
            fs::canonicalize(main).expect("canonical")
        );
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
