//! Watches an open repository for external changes (editor saves, terminal
//! git commands) and emits a debounced `repo-changed` event to the frontend.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use std::path::PathBuf;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify_debouncer_full::{new_debouncer, notify::RecursiveMode, DebouncedEvent, Debouncer, RecommendedCache};
use notify_debouncer_full::notify::RecommendedWatcher;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Type)]
pub struct RepoChangedPayload {
  pub repo_id: String,
}

/// A repo's compiled `.gitignore` rules, shared between the event closure (reads)
/// and rebuilds triggered when an ignore file changes (writes). Wrapped in an
/// `Arc<RwLock>` so watching stays lock-free with respect to the repo handle that
/// `get_status` holds -- this matcher is entirely owned by the watcher.
type SharedMatcher = Arc<RwLock<Gitignore>>;

/// Builds the ignore matcher for `workdir` from the root `.gitignore`, any nested
/// `.gitignore` files, and `.git/info/exclude`. Returns an empty matcher (ignores
/// nothing) if none exist or parsing fails, so a missing ignore file just means
/// the cheap lexical filter does all the work.
fn build_matcher(workdir: &Path) -> Gitignore {
  let mut builder = GitignoreBuilder::new(workdir);
  // Root .gitignore and the repo-local exclude file.
  let _ = builder.add(workdir.join(".gitignore"));
  let _ = builder.add(workdir.join(".git").join("info").join("exclude"));
  builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// True when a changed path is an ignore file whose edits should rebuild the
/// matcher: a `.gitignore` anywhere in the tree, or `.git/info/exclude`.
fn is_ignore_file(rel: &Path) -> bool {
  if rel.file_name().map(|n| n == ".gitignore").unwrap_or(false) {
    return true;
  }
  let mut comps = rel.components().map(|c| c.as_os_str());
  matches!(
    (comps.next(), comps.next(), comps.next()),
    (Some(git), Some(info), Some(exclude))
      if git == ".git" && info == "info" && exclude == "exclude"
  )
}

/// Directory names whose churn never changes what GitWyrm displays. Build output
/// and dependency trees (`target`, `node_modules`, ...) generate thousands of
/// filesystem events during a build or HMR pass; on Windows those overflow the
/// `ReadDirectoryChangesW` buffer and cause `notify` to silently drop the real
/// source-file events we care about. Filtering them here keeps the watcher
/// responsive to actual working-tree edits. `.git` internals are handled
/// separately so we still react to ref/index changes.
const IGNORED_DIRS: &[&str] = &[
  "node_modules",
  "target",
  "dist",
  "build",
  ".next",
  ".turbo",
  ".cache",
  "out",
];

/// Files inside `.git` that reflect user-visible repo state (branch moves,
/// staging, commits). Everything else under `.git` (object writes, lock chatter,
/// FETCH_HEAD churn) is noise we skip so a background fetch or gc doesn't spam
/// the frontend.
const GIT_INTERNAL_WATCH: &[&str] = &["HEAD", "index", "MERGE_HEAD", "packed-refs", "ORIG_HEAD"];

/// Returns true when a changed path should trigger a `repo-changed` emit.
///
/// Two-stage filter: a cheap, purely lexical pass rejects the well-known heavy
/// directories and `.git` noise with no I/O, then anything that survives is
/// checked against the repo's compiled `.gitignore` so a repo-specific ignored
/// path (`.venv`, `coverage`, generated output, ...) is skipped too. The matcher
/// lookup is in-memory -- it does not touch the repo lock.
fn path_is_relevant(path: &Path, workdir: &Path, matcher: &Gitignore) -> bool {
  let rel = match path.strip_prefix(workdir) {
    Ok(rel) => rel,
    // Outside the workdir (shouldn't happen with a recursive watch) -- be safe.
    Err(_) => return true,
  };

  let mut in_git = false;
  for comp in rel.components() {
    let os = comp.as_os_str();
    if os == ".git" {
      in_git = true;
      continue;
    }
    if !in_git {
      let name = os.to_string_lossy();
      if IGNORED_DIRS.iter().any(|d| *d == name) {
        return false;
      }
    }
  }

  if in_git {
    // Only react to the handful of .git files that map to visible state.
    // `refs/` moves matter too (branch/tag updates land as files there).
    let under_refs = rel.components().any(|c| c.as_os_str() == "refs");
    if under_refs {
      return true;
    }
    let file = path.file_name().map(|n| n.to_string_lossy());
    return match file {
      Some(name) => GIT_INTERNAL_WATCH.iter().any(|f| *f == name),
      None => false,
    };
  }

  // Survived the lexical filter -- defer to the repo's own ignore rules.
  // `matched_path_or_any_parents` catches files *inside* an ignored directory
  // (e.g. `.venv/lib/x.py` under a `.venv/` rule), which the plain `matched`
  // check would miss since notify hands us the leaf path, not the directory.
  if matcher.matched_path_or_any_parents(rel, false).is_ignore() {
    return false;
  }

  true
}

type RepoDebouncer = Debouncer<RecommendedWatcher, RecommendedCache>;

#[derive(Default)]
pub struct WatcherRegistry {
  watchers: Mutex<HashMap<String, RepoDebouncer>>,
}

impl WatcherRegistry {
  /// Registers the recursive watch on a background blocking thread and returns
  /// immediately. Walking a large working tree to arm `ReadDirectoryChangesW`
  /// can take seconds, and none of `open_repo`'s callers need the watch to be
  /// live before they get their `RepoInfo` back -- external-change events just
  /// start flowing a moment later. `app` owns the managed `WatcherRegistry`, so
  /// the spawned task reaches back into it through state rather than borrowing.
  pub fn watch_deferred(app: AppHandle, repo_id: String, workdir: PathBuf) {
    tauri::async_runtime::spawn_blocking(move || {
      let start = std::time::Instant::now();
      let registry = app.state::<WatcherRegistry>();
      match registry.watch(app.clone(), repo_id.clone(), &workdir) {
        Ok(()) => log::info!(
          "watch: armed in {}ms for {}",
          start.elapsed().as_millis(),
          workdir.display()
        ),
        Err(e) => log::warn!("watch: failed to arm for {}: {e}", workdir.display()),
      }
    });
  }

  pub fn watch(&self, app: AppHandle, repo_id: String, workdir: &Path) -> Result<(), String> {
    let id = repo_id.clone();
    let watch_root = workdir.to_path_buf();
    let matcher: SharedMatcher = Arc::new(RwLock::new(build_matcher(workdir)));
    let matcher_for_events = matcher.clone();
    let mut debouncer = new_debouncer(
      Duration::from_millis(150),
      None,
      move |result: Result<Vec<DebouncedEvent>, Vec<notify_debouncer_full::notify::Error>>| {
        if let Ok(events) = result {
          // If any .gitignore (or .git/info/exclude) changed, rebuild the matcher
          // before filtering so this same batch is judged by the new rules.
          let ignore_changed = events.iter().flat_map(|e| e.paths.iter()).any(|p| {
            p.strip_prefix(&watch_root)
              .map(is_ignore_file)
              .unwrap_or(false)
          });
          if ignore_changed {
            let rebuilt = build_matcher(&watch_root);
            if let Ok(mut guard) = matcher_for_events.write() {
              *guard = rebuilt;
            }
            log::debug!("watch: rebuilt ignore matcher for {}", watch_root.display());
          }

          // Only emit when at least one changed path maps to visible repo state.
          // Build output and dependency-tree churn is filtered out so it can't
          // drown out real edits (or, on Windows, overflow the OS event buffer
          // and drop them). A batch with only noise is skipped entirely.
          let guard = matcher_for_events.read();
          let empty = Gitignore::empty();
          let m = guard.as_deref().unwrap_or(&empty);
          let relevant = events
            .iter()
            .flat_map(|e| e.paths.iter())
            .any(|p| path_is_relevant(p, &watch_root, m));
          if relevant {
            let _ = app.emit("repo-changed", RepoChangedPayload { repo_id: id.clone() });
          }
        }
      },
    )
    .map_err(|e| e.to_string())?;

    debouncer
      .watch(workdir, RecursiveMode::Recursive)
      .map_err(|e| e.to_string())?;

    self.watchers.lock().unwrap().insert(repo_id, debouncer);
    Ok(())
  }

  pub fn unwatch(&self, repo_id: &str) {
    self.watchers.lock().unwrap().remove(repo_id);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn root() -> PathBuf {
    PathBuf::from("C:/code/GitWyrm")
  }

  /// A matcher that ignores the given patterns, rooted at `root()`.
  fn matcher_with(patterns: &[&str]) -> Gitignore {
    let mut b = GitignoreBuilder::new(root());
    for p in patterns {
      b.add_line(None, p).unwrap();
    }
    b.build().unwrap()
  }

  fn empty() -> Gitignore {
    Gitignore::empty()
  }

  #[test]
  fn source_edits_are_relevant() {
    assert!(path_is_relevant(&root().join("src/watcher.rs"), &root(), &empty()));
    assert!(path_is_relevant(&root().join("README.md"), &root(), &empty()));
  }

  #[test]
  fn build_output_is_ignored() {
    assert!(!path_is_relevant(&root().join("target/debug/foo.rlib"), &root(), &empty()));
    assert!(!path_is_relevant(&root().join("node_modules/react/index.js"), &root(), &empty()));
    assert!(!path_is_relevant(&root().join("dist/bundle.js"), &root(), &empty()));
  }

  #[test]
  fn git_visible_state_is_relevant() {
    assert!(path_is_relevant(&root().join(".git/HEAD"), &root(), &empty()));
    assert!(path_is_relevant(&root().join(".git/index"), &root(), &empty()));
    assert!(path_is_relevant(&root().join(".git/refs/heads/main"), &root(), &empty()));
  }

  #[test]
  fn git_noise_is_ignored() {
    assert!(!path_is_relevant(&root().join(".git/objects/ab/cdef"), &root(), &empty()));
    assert!(!path_is_relevant(&root().join(".git/index.lock"), &root(), &empty()));
    assert!(!path_is_relevant(&root().join(".git/FETCH_HEAD"), &root(), &empty()));
  }

  #[test]
  fn gitignored_paths_are_skipped() {
    let m = matcher_with(&[".venv/", "coverage/", "*.log"]);
    assert!(!path_is_relevant(&root().join(".venv/lib/x.py"), &root(), &m));
    assert!(!path_is_relevant(&root().join("coverage/report.html"), &root(), &m));
    assert!(!path_is_relevant(&root().join("debug.log"), &root(), &m));
    // Not ignored -- a real source edit still gets through.
    assert!(path_is_relevant(&root().join("src/main.rs"), &root(), &m));
  }

  #[test]
  fn gitignore_file_itself_is_relevant() {
    // Editing .gitignore changes what git reports, so it must emit.
    let m = matcher_with(&["*.log"]);
    assert!(path_is_relevant(&root().join(".gitignore"), &root(), &m));
  }

  #[test]
  fn detects_ignore_files() {
    assert!(is_ignore_file(Path::new(".gitignore")));
    assert!(is_ignore_file(Path::new("src/.gitignore")));
    assert!(is_ignore_file(Path::new(".git/info/exclude")));
    assert!(!is_ignore_file(Path::new("src/main.rs")));
    assert!(!is_ignore_file(Path::new(".git/HEAD")));
  }
}
