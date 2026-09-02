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

/// Headline numbers for one repository the picker has selected.
///
/// Every field is best-effort: a repository that cannot be opened, or a host
/// that cannot be reached, reports what it could and leaves the rest empty
/// rather than failing the whole panel.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RepoSnapshot {
    /// Local branches.
    pub branches: u32,
    /// Remote-tracking branches, across every remote.
    pub remote_branches: u32,
    /// Tags.
    pub tags: u32,
    /// Current branch, or None when HEAD is detached or unreadable.
    pub head_branch: Option<String>,
    /// How far the current branch leads its upstream. None when it has none.
    pub ahead: Option<u32>,
    /// How far the current branch trails its upstream. None when it has none.
    pub behind: Option<u32>,
    /// Files with uncommitted changes, staged or not.
    pub changes: u32,
    /// Open pull requests, or None when the host was not reached.
    pub prs: Option<u32>,
    /// Open issues, or None when the host does not have them or was not reached.
    pub issues: Option<u32>,
    /// Whether the fetch this snapshot asked for actually ran. False means the
    /// counts are from whatever was already on disk.
    pub fetched: bool,
}

/// Counts branches, tags, ahead/behind and uncommitted files for a repository
/// on disk.
///
/// Opens its own short-lived handle rather than going through RepoManager: the
/// picker's repositories are closed, so there is no handle to borrow, and this
/// must not be what opens one.
fn read_git_stats(path: &str) -> Option<RepoSnapshot> {
    let repo = git2::Repository::open(path).ok()?;

    let mut branches = 0;
    let mut remote_branches = 0;
    if let Ok(iter) = repo.branches(None) {
        for (_, kind) in iter.flatten() {
            match kind {
                git2::BranchType::Local => branches += 1,
                git2::BranchType::Remote => remote_branches += 1,
            }
        }
    }

    let tags = repo.tag_names(None).map(|names| names.len() as u32).unwrap_or(0);

    let head = repo.head().ok();
    let head_branch = head
        .as_ref()
        .filter(|h| h.is_branch())
        .and_then(|h| h.shorthand().ok())
        .map(str::to_string);

    // Ahead/behind is only meaningful against the branch's own upstream; a
    // branch that has never been pushed reports neither rather than zero, so
    // "in step with the remote" and "has no remote" stay distinguishable.
    let (ahead, behind) = head_branch
        .as_deref()
        .and_then(|name| {
            let branch = repo.find_branch(name, git2::BranchType::Local).ok()?;
            let upstream = branch.upstream().ok()?;
            let ours = branch.get().target()?;
            let theirs = upstream.get().target()?;
            crate::git::refs::ahead_behind(&repo, ours, theirs)
        })
        .map(|(a, b)| (Some(a), Some(b)))
        .unwrap_or((None, None));

    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).include_ignored(false);
    let changes = repo
        .statuses(Some(&mut options))
        .map(|s| s.len() as u32)
        .unwrap_or(0);

    Some(RepoSnapshot {
        branches,
        remote_branches,
        tags,
        head_branch,
        ahead,
        behind,
        changes,
        prs: None,
        issues: None,
        fetched: false,
    })
}

/// Brings a closed repository's remote-tracking branches up to date.
///
/// Runs unattended on purpose. This is started by selecting a row, not by
/// pressing a sync button, so it must never put a login window in front of
/// someone who was only browsing their list. An unauthenticated remote simply
/// fails here and the snapshot reports what was already on disk.
fn fetch_quietly(path: &str) -> bool {
    let args = crate::git::shell::credential_args(crate::git::shell::Attended::Background);
    let mut all: Vec<&str> = args.iter().map(String::as_str).collect();
    all.extend_from_slice(&["fetch", "--all", "--prune", "--quiet"]);
    crate::git::shell::run_git_unattended(Some(path), &all).is_ok()
}

/// Everything the details panel shows about one repository, optionally after a
/// fetch so the ahead/behind numbers are not stale.
///
/// The host counts reuse the same per-repository counter the library scan uses,
/// so a repository selected here and a repository scanned in bulk can never
/// report different numbers.
#[tauri::command]
#[specta::specta]
pub async fn repo_snapshot(
    app: tauri::AppHandle,
    path: String,
    fetch: bool,
) -> Result<Option<RepoSnapshot>, AppError> {
    let disk = path.clone();
    let fetched = if fetch {
        tauri::async_runtime::spawn_blocking(move || fetch_quietly(&disk))
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let disk = path.clone();
    let Some(mut snapshot) =
        tauri::async_runtime::spawn_blocking(move || read_git_stats(&disk))
            .await
            .map_err(|e| AppError::Other(e.to_string()))?
    else {
        return Ok(None);
    };
    snapshot.fetched = fetched;

    let counts = crate::commands::github::count_one(&app, path).await;
    if counts.checked {
        snapshot.prs = Some(counts.prs);
        snapshot.issues = Some(counts.issues);
    }
    Ok(Some(snapshot))
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

    /// Builds a real repository and checks every number the panel shows, so a
    /// miscount is caught here rather than looking like a display bug.
    #[test]
    fn counts_branches_tags_and_uncommitted_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = git2::Repository::init(dir.path()).expect("init");
        let sig = git2::Signature::now("Scan Test", "scan@example.com").expect("signature");

        fs::write(dir.path().join("a.txt"), "one").expect("write");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("a.txt")).expect("add");
        index.write().expect("write index");
        let tree = repo
            .find_tree(index.write_tree().expect("tree id"))
            .expect("tree");
        let head = repo
            .commit(Some("HEAD"), &sig, &sig, "first", &tree, &[])
            .expect("commit");
        let commit = repo.find_commit(head).expect("find commit");

        repo.branch("feature", &commit, false).expect("branch");
        repo.tag_lightweight("v1", commit.as_object(), false)
            .expect("tag");
        // An untracked file counts as a change: the panel is answering "is
        // there work here", and a new file is work.
        fs::write(dir.path().join("b.txt"), "two").expect("write");

        let path = dir.path().to_string_lossy().into_owned();
        let snapshot = read_git_stats(&path).expect("snapshot");

        assert_eq!(snapshot.branches, 2, "the default branch plus feature");
        assert_eq!(snapshot.remote_branches, 0);
        assert_eq!(snapshot.tags, 1);
        assert_eq!(snapshot.changes, 1);
        assert!(snapshot.head_branch.is_some());
        // No upstream, so neither number is reported -- distinct from zero.
        assert_eq!(snapshot.ahead, None);
        assert_eq!(snapshot.behind, None);
    }

    #[test]
    fn a_folder_that_is_not_a_repository_has_no_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read_git_stats(&dir.path().to_string_lossy()).is_none());
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
