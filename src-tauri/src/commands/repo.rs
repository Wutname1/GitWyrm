use std::time::Instant;

use serde::Deserialize;
use specta::Type;
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::git::types::RepoInfo;
use crate::state::RepoManager;
use crate::watcher::WatcherRegistry;

/// Anything slower than this on a single phase of opening a repo is worth a
/// warning: it is the difference between "felt instant" and "user thinks we
/// hung". Recursive watch registration over a large working tree is the usual
/// culprit, so the phases are timed separately to say which one was slow.
const SLOW_PHASE: u128 = 2_000;

#[derive(Debug, Clone, Copy, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryStarter {
  Blank,
  Node,
  Rust,
  Csharp,
  AllInOne,
}

const NODE_GITIGNORE: &str = "\
node_modules/
.npm/
.pnpm-store/
.yarn/
dist/
build/
coverage/
*.log
.env
.env.*
!.env.example
";

const RUST_GITIGNORE: &str = "\
/target/
**/*.rs.bk
";

const CSHARP_GITIGNORE: &str = "\
[Bb]in/
[Oo]bj/
.vs/
*.user
*.suo
*.userosscache
*.sln.docstates
TestResults/
artifacts/
packages/
*.nupkg
";

const ALL_IN_ONE_GITIGNORE: &str = "\
# Secrets and local configuration
.env
.env.*
!.env.example
*.pem
*.key

# Operating systems and editors
.DS_Store
Thumbs.db
Desktop.ini
.idea/
.vscode/
*.swp
*.swo
*~

# Logs, caches, and temporary files
*.log
logs/
.cache/
.tmp/
tmp/
temp/
coverage/

# JavaScript and TypeScript
node_modules/
.npm/
.pnpm-store/
.yarn/
dist/
build/
.next/
.nuxt/
.svelte-kit/

# Rust
target/
**/*.rs.bk

# .NET and Visual Studio
[Bb]in/
[Oo]bj/
.vs/
*.user
*.suo
TestResults/
artifacts/
packages/
*.nupkg

# Python
__pycache__/
*.py[cod]
.pytest_cache/
.mypy_cache/
.ruff_cache/
.venv/
venv/

# Java and Kotlin
*.class
.gradle/
out/

# Go
vendor/
*.test

# Mobile builds
.expo/
.dart_tool/
.flutter-plugins*
.pub-cache/

# Archives and generated files
*.zip
*.tar
*.tar.gz
*.7z
";

fn starter_gitignore(starter: RepositoryStarter) -> Option<&'static str> {
  match starter {
    RepositoryStarter::Blank => None,
    RepositoryStarter::Node => Some(NODE_GITIGNORE),
    RepositoryStarter::Rust => Some(RUST_GITIGNORE),
    RepositoryStarter::Csharp => Some(CSHARP_GITIGNORE),
    RepositoryStarter::AllInOne => Some(ALL_IN_ONE_GITIGNORE),
  }
}

fn log_phase(phase: &str, path: &str, elapsed_ms: u128) {
  if elapsed_ms >= SLOW_PHASE {
    log::warn!("open_repo: {phase} took {elapsed_ms}ms (slow) for {path}");
  } else {
    log::info!("open_repo: {phase} took {elapsed_ms}ms for {path}");
  }
}

fn head_branch(repo: &git2::Repository) -> Option<String> {
  let head = repo.head().ok()?;
  if head.is_branch() {
    head.shorthand().map(str::to_string)
  } else {
    None
  }
}

#[tauri::command]
#[specta::specta]
pub async fn open_repo(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  watchers: State<'_, WatcherRegistry>,
  path: String,
) -> Result<RepoInfo, AppError> {
  log::info!("open_repo: start for {path}");
  let started = Instant::now();

  let discover_start = Instant::now();
  let (id, open) = manager.open(&path)?;
  log_phase("discover", &path, discover_start.elapsed().as_millis());

  // Registering a recursive watch walks the whole working tree, so a repo with
  // a large node_modules/target directory can spend most of the open here.
  let watch_start = Instant::now();
  watchers
    .watch(app, id.clone(), &open.path)
    .map_err(AppError::Other)?;
  log_phase("watch", &path, watch_start.elapsed().as_millis());

  let head_start = Instant::now();
  let repo = open.repo.lock().unwrap();
  let name = open
    .path
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| "repository".into());
  let head_branch = head_branch(&repo);
  log_phase("head", &path, head_start.elapsed().as_millis());

  log::info!(
    "open_repo: done in {}ms (id {id}, branch {}) for {path}",
    started.elapsed().as_millis(),
    head_branch.as_deref().unwrap_or("<detached>")
  );

  Ok(RepoInfo {
    id,
    name,
    path: open.path.to_string_lossy().into_owned(),
    head_branch,
  })
}

#[tauri::command]
#[specta::specta]
pub async fn close_repo(
  manager: State<'_, RepoManager>,
  watchers: State<'_, WatcherRegistry>,
  repo_id: String,
) -> Result<(), AppError> {
  log::info!("close_repo: {repo_id}");
  watchers.unwatch(&repo_id);
  manager.close(&repo_id);
  Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn git_available() -> Result<bool, AppError> {
  Ok(crate::git::shell::git_available())
}

/// Create a brand-new git repository and its requested starter files. The
/// folder must be new or empty so existing work is never overwritten. Returns
/// the working-directory path so the caller can immediately open it.
#[tauri::command]
#[specta::specta]
pub async fn git_init(
  path: String,
  starter: RepositoryStarter,
  add_readme: bool,
  create_initial_commit: bool,
) -> Result<String, AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    let dir = std::path::Path::new(&path);
    if git2::Repository::open(dir).is_ok() {
      return Err(AppError::Other(
        "A git repository already exists in that folder".into(),
      ));
    }

    if dir.exists() {
      let has_files = std::fs::read_dir(dir)
        .map_err(|e| AppError::Other(e.to_string()))?
        .next()
        .is_some();
      if has_files {
        return Err(AppError::Other(
          "That folder already has files. Choose an empty folder or a new project name".into(),
        ));
      }
    }

    let signature = if create_initial_commit {
      let config = git2::Config::open_default()?;
      let name = config.get_string("user.name").map_err(|_| {
        AppError::Other(
          "Git does not know your name. Set it in Git before saving the starter files to history".into(),
        )
      })?;
      let email = config.get_string("user.email").map_err(|_| {
        AppError::Other(
          "Git does not know your email. Set it in Git before saving the starter files to history".into(),
        )
      })?;
      Some(git2::Signature::now(&name, &email)?)
    } else {
      None
    };

    std::fs::create_dir_all(dir).map_err(|e| AppError::Other(e.to_string()))?;

    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head("main");
    let repo = git2::Repository::init_opts(dir, &options)?;

    if let Some(contents) = starter_gitignore(starter) {
      std::fs::write(dir.join(".gitignore"), contents)
        .map_err(|e| AppError::Other(e.to_string()))?;
    }
    if add_readme {
      let project_name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("New project");
      std::fs::write(dir.join("README.md"), format!("# {project_name}\n"))
        .map_err(|e| AppError::Other(e.to_string()))?;
    }

    if let Some(signature) = signature {
      let mut index = repo.index()?;
      index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
      index.write()?;
      let tree_oid = index.write_tree()?;
      let tree = repo.find_tree(tree_oid)?;
      repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Start project",
        &tree,
        &[],
      )?;
    }

    Ok(path)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
