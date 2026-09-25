//! Mehen, the dependency checker, working alongside GitWyrm.
//!
//! Mehen writes a small summary of its last check, one entry per repository,
//! to `repo-status.json` in its data folder. GitWyrm only ever reads that file:
//! it never checks packages itself and never opens Mehen's database. The file
//! is versioned; a format this code does not know is treated as absent.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::error::AppError;
use crate::state::RepoManager;

const STATUS_FILE: &str = "repo-status.json";
const STATUS_FORMAT: u32 = 1;
/// Outgoing commits looked at for dependency changes before giving up.
const PUSH_WALK_LIMIT: usize = 500;
/// Changed dependency files named in a push note.
const PUSH_FILES_SHOWN: usize = 5;

// --- Mehen's file, as Mehen writes it -------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusFile {
    format: u32,
    #[serde(default)]
    exe: Option<String>,
    #[serde(default)]
    repos: Vec<FileRepo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRepo {
    path: String,
    checked_at: Option<u64>,
    checked_commit: Option<String>,
    /// Vulnerable packages that have a fixed version the repository can use.
    #[serde(default)]
    fixable: u32,
    outdated: u32,
    #[serde(default)]
    problems: Vec<FileProblem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileProblem {
    name: String,
    ecosystem: String,
    version: Option<String>,
    fixed_in: Option<String>,
    severity: Option<String>,
    summary: String,
    url: String,
}

// --- What the UI gets -----------------------------------------------------

/// What Mehen's last check found, for every repository it checks.
#[derive(Debug, Clone, Serialize, Type)]
pub struct MehenOverview {
    /// Whether Mehen can be opened from GitWyrm on this computer.
    pub can_open: bool,
    pub repos: Vec<MehenRepoStatus>,
}

/// What Mehen's last check found in one repository.
///
/// Only security problems that have a fix are passed on. A problem nobody can
/// fix yet is not something to act on, so GitWyrm does not raise it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct MehenRepoStatus {
    /// The repository's folder as Mehen spells it.
    pub path: String,
    /// When Mehen last checked this repository, seconds since epoch.
    pub checked_at: Option<f64>,
    /// Packages with a known security problem and a fixed version to move to.
    pub fixable: u32,
    /// Packages with a newer version available.
    pub outdated: u32,
    /// Fixable problems, the most serious first; a few at most.
    pub problems: Vec<MehenProblem>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct MehenProblem {
    pub name: String,
    pub ecosystem: String,
    pub version: Option<String>,
    /// The smallest version that fixes it.
    pub fixed_in: String,
    /// `CRITICAL`, `HIGH`, `MODERATE` or `LOW`.
    pub severity: Option<String>,
    pub summary: String,
    pub url: String,
}

/// A heads-up for a push that changes dependency files in a repository where
/// Mehen found packages with security problems.
#[derive(Debug, Clone, Serialize, Type)]
pub struct MehenPushNote {
    /// Packages with a known security problem and a fix available.
    pub fixable: u32,
    pub checked_at: Option<f64>,
    /// Mehen checked the repository after every one of these dependency
    /// changes was made, so its numbers include them.
    pub seen_by_mehen: bool,
    /// Dependency files the outgoing commits change; a few at most.
    pub files: Vec<String>,
    pub can_open: bool,
}

/// Mehen's data folder, resolved the way Tauri resolves it for Mehen's
/// identifier. If Mehen's identifier ever changes, this must follow.
fn mehen_data_dir() -> Option<PathBuf> {
    const IDENTIFIER: &str = "dev.mehen.app";
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })
    };
    Some(base?.join(IDENTIFIER))
}

fn read_status_file(dir: &Path) -> Option<StatusFile> {
    let raw = std::fs::read_to_string(dir.join(STATUS_FILE)).ok()?;
    let file: StatusFile = serde_json::from_str(&raw).ok()?;
    (file.format == STATUS_FORMAT).then_some(file)
}

/// Paths as Mehen and GitWyrm may spell them differently: case and slashes on
/// Windows, a trailing separator anywhere.
fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        let p = p.trim_end_matches(['/', '\\']);
        if cfg!(windows) {
            p.replace('/', "\\").to_lowercase()
        } else {
            p.to_string()
        }
    };
    norm(a) == norm(b)
}

fn find_repo(file: StatusFile, repo_path: &Path) -> Option<(FileRepo, Option<String>)> {
    let wanted = repo_path.to_string_lossy();
    let exe = file.exe;
    file.repos
        .into_iter()
        .find(|r| same_path(&r.path, &wanted))
        .map(|r| (r, exe))
}

/// Mehen's program: the installed copy first, then whichever copy last wrote
/// the status file (a development build, say).
fn mehen_exe(status_exe: Option<&str>) -> Option<PathBuf> {
    installed_mehen().or_else(|| status_exe.map(PathBuf::from).filter(|p| p.is_file()))
}

#[cfg(windows)]
fn installed_mehen() -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Mehen")
        .ok()?;
    let location: String = key.get_value("InstallLocation").ok()?;
    let binary: String = key.get_value("MainBinaryName").unwrap_or_else(|_| "mehen.exe".into());
    let exe = PathBuf::from(location.trim_matches('"')).join(binary);
    exe.is_file().then_some(exe)
}

#[cfg(not(windows))]
fn installed_mehen() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|dir| dir.join("mehen")).find(|p| p.is_file())
}

fn to_status(repo: FileRepo) -> MehenRepoStatus {
    MehenRepoStatus {
        path: repo.path,
        checked_at: repo.checked_at.map(|t| t as f64),
        fixable: repo.fixable,
        outdated: repo.outdated,
        problems: repo
            .problems
            .into_iter()
            .filter_map(|p| {
                Some(MehenProblem {
                    fixed_in: p.fixed_in?,
                    name: p.name,
                    ecosystem: p.ecosystem,
                    version: p.version,
                    severity: p.severity,
                    summary: p.summary,
                    url: p.url,
                })
            })
            .collect(),
    }
}

/// What Mehen last found in every repository it checks, read in one go so
/// every tab can show its own count. Nothing when Mehen has never written its
/// summary (or is not installed).
#[tauri::command]
#[specta::specta]
pub async fn mehen_overview() -> Result<Option<MehenOverview>, AppError> {
    tauri::async_runtime::spawn_blocking(|| {
        let file = mehen_data_dir().and_then(|dir| read_status_file(&dir))?;
        let can_open = mehen_exe(file.exe.as_deref()).is_some();
        Some(MehenOverview { can_open, repos: file.repos.into_iter().map(to_status).collect() })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Whether a file lists a project's packages or pins their versions.
pub fn is_dependency_file(path: &str) -> bool {
    const NAMES: &[&str] = &[
        "package.json",
        "package-lock.json",
        "npm-shrinkwrap.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "bun.lock",
        "bun.lockb",
        "cargo.toml",
        "cargo.lock",
        "packages.config",
        "directory.packages.props",
        "go.mod",
        "go.sum",
        "pyproject.toml",
        "pipfile",
        "pipfile.lock",
        "uv.lock",
        "poetry.lock",
        "pdm.lock",
        "pubspec.yaml",
        "pubspec.lock",
        "composer.json",
        "composer.lock",
        "gemfile",
        "gemfile.lock",
        "action.yml",
        "action.yaml",
    ];
    let path = path.replace('\\', "/").to_ascii_lowercase();
    let name = path.rsplit('/').next().unwrap_or(&path);
    NAMES.contains(&name)
        || [".csproj", ".fsproj", ".vbproj"].iter().any(|ext| name.ends_with(ext))
        || (name.starts_with("requirements") && name.ends_with(".txt"))
        || (path.starts_with(".github/workflows/") && (name.ends_with(".yml") || name.ends_with(".yaml")))
}

/// Commits on HEAD that no remote has yet, and the dependency files they
/// change. Merge commits are skipped: their changes came from another branch,
/// usually one that is already public.
fn outgoing_dependency_changes(repo: &git2::Repository) -> Result<(Vec<git2::Oid>, Vec<String>), git2::Error> {
    let mut walk = repo.revwalk()?;
    walk.push_head()?;
    walk.hide_glob("refs/remotes/*")?;
    let mut commits = Vec::new();
    let mut files: Vec<String> = Vec::new();
    for oid in walk.take(PUSH_WALK_LIMIT) {
        let commit = repo.find_commit(oid?)?;
        if commit.parent_count() > 1 {
            continue;
        }
        let parent = commit.parent(0).ok().map(|p| p.tree()).transpose()?;
        let diff = repo.diff_tree_to_tree(parent.as_ref(), Some(&commit.tree()?), None)?;
        let mut touched = false;
        for delta in diff.deltas() {
            let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) else { continue };
            let path = path.to_string_lossy().replace('\\', "/");
            if is_dependency_file(&path) {
                touched = true;
                if !files.contains(&path) {
                    files.push(path);
                }
            }
        }
        if touched {
            commits.push(commit.id());
        }
    }
    Ok((commits, files))
}

/// A note to show before and after pushing, when the push changes dependency
/// files and Mehen found packages with fixable security problems in this repository.
/// Nothing otherwise: the note never blocks a push and stays quiet by default.
#[tauri::command]
#[specta::specta]
pub async fn mehen_push_note(
    manager: State<'_, RepoManager>,
    repo_id: String,
) -> Result<Option<MehenPushNote>, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some((status, exe)) = mehen_data_dir()
            .and_then(|dir| read_status_file(&dir))
            .and_then(|file| find_repo(file, &open.path))
        else {
            return Ok(None);
        };
        if status.fixable == 0 {
            return Ok(None);
        }
        let repo = open.repo.lock().unwrap();
        let (commits, mut files) = outgoing_dependency_changes(&repo)?;
        if commits.is_empty() {
            return Ok(None);
        }
        let checked = status.checked_commit.as_deref().and_then(|sha| git2::Oid::from_str(sha).ok());
        let seen_by_mehen = checked.is_some_and(|checked| {
            commits.iter().all(|&c| c == checked || repo.graph_descendant_of(checked, c).unwrap_or(false))
        });
        files.truncate(PUSH_FILES_SHOWN);
        Ok(Some(MehenPushNote {
            fixable: status.fixable,
            checked_at: status.checked_at.map(|t| t as f64),
            seen_by_mehen,
            files,
            can_open: mehen_exe(exe.as_deref()).is_some(),
        }))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Show this repository in Mehen. A Mehen that is already running brings its
/// window forward and switches to the repository instead of starting again.
#[tauri::command]
#[specta::specta]
pub async fn open_in_mehen(manager: State<'_, RepoManager>, repo_id: String) -> Result<(), AppError> {
    let path = manager.get(&repo_id)?.path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let status_exe = mehen_data_dir().and_then(|dir| read_status_file(&dir)).and_then(|f| f.exe);
        let exe = mehen_exe(status_exe.as_deref()).ok_or_else(|| AppError::Other("Mehen is not installed".into()))?;
        let folder = path.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
        std::process::Command::new(exe)
            .arg(folder)
            .spawn()
            .map(|_| ())
            .map_err(|e| AppError::Other(format!("Could not start Mehen: {e}")))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_dependency_files_in_every_ecosystem_mehen_checks() {
        for path in [
            "package.json",
            "web/package-lock.json",
            "Cargo.lock",
            "src-tauri/Cargo.toml",
            "App/App.csproj",
            "go.sum",
            "requirements-dev.txt",
            "pubspec.yaml",
            "composer.lock",
            "Gemfile",
            ".github/workflows/ci.yml",
            "Directory.Packages.props",
        ] {
            assert!(is_dependency_file(path), "{path}");
        }
        for path in ["src/App.tsx", "README.md", "docs/package.json.md", "workflows/ci.yml", "requirements.md"] {
            assert!(!is_dependency_file(path), "{path}");
        }
    }

    #[test]
    fn matches_repositories_across_spellings() {
        assert!(same_path("C:\\code\\GitWyrm", "C:\\code\\GitWyrm\\"));
        if cfg!(windows) {
            assert!(same_path("c:/code/gitwyrm", "C:\\code\\GitWyrm"));
        }
        assert!(!same_path("C:\\code\\GitWyrm", "C:\\code\\GitWyrm-Docs"));
    }

    #[test]
    fn skips_a_file_in_a_format_it_does_not_know() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(STATUS_FILE), r#"{"format":2,"repos":[]}"#).unwrap();
        assert!(read_status_file(dir.path()).is_none());
        std::fs::write(
            dir.path().join(STATUS_FILE),
            r#"{"format":1,"writtenBy":"Mehen 0.1.0","writtenAt":1,"futureField":true,"repos":[{"path":"C:\\code\\a","checkedAt":5,"checkedCommit":null,"vulnerable":2,"fixable":1,"severity":{"high":1},"outdated":3,"problems":[{"name":"a","ecosystem":"npm","version":"1.0.0","fixedIn":"1.0.1","severity":"HIGH","summary":"s","advisory":"X","url":"u"},{"name":"b","ecosystem":"npm","version":"1.0.0","fixedIn":null,"severity":"HIGH","summary":"s","advisory":"Y","url":"u"}]}]}"#,
        )
        .unwrap();
        let file = read_status_file(dir.path()).unwrap();
        let (repo, _) = find_repo(file, Path::new("C:\\code\\a")).unwrap();
        let status = to_status(repo);
        assert_eq!((status.fixable, status.outdated), (1, 3));
        let names: Vec<&str> = status.problems.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["a"], "a problem with no fix is not passed on");
    }

    #[test]
    fn finds_dependency_changes_only_in_commits_no_remote_has() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let commit = |files: &[(&str, &str)], message: &str| {
            for (name, body) in files {
                let path = dir.path().join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, body).unwrap();
            }
            let mut index = repo.index().unwrap();
            index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parents: Vec<git2::Commit> = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect();
            let refs: Vec<&git2::Commit> = parents.iter().collect();
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &refs).unwrap()
        };
        let published = commit(&[("package.json", "{}"), ("README.md", "hi")], "first");
        repo.reference("refs/remotes/origin/main", published, true, "pushed").unwrap();

        commit(&[("README.md", "hello")], "docs only");
        assert_eq!(outgoing_dependency_changes(&repo).unwrap(), (vec![], vec![]));

        let bump = commit(&[("package.json", "{\"a\":1}"), ("web/Cargo.lock", "x")], "bump");
        let (commits, files) = outgoing_dependency_changes(&repo).unwrap();
        assert_eq!(commits, vec![bump]);
        assert_eq!(files, vec!["package.json".to_string(), "web/Cargo.lock".to_string()]);
    }
}
