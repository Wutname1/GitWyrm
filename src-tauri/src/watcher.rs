//! Watches an open repository for external changes (editor saves, terminal
//! git commands) and emits a debounced `repo-changed` event to the frontend.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, RwLock};
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
    // `.git/worktrees/` is where git records linked worktrees. Watching it is
    // what makes a `git worktree add` typed in a terminal show up on its own:
    // re-checking when the repository becomes active cannot see a terminal
    // running beside an already-focused window, which is the common case.
    if rel.components().any(|c| c.as_os_str() == "worktrees") {
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

/// How many repositories may arm their watch at the same time.
///
/// Arming walks the whole working tree, and every repo does it on the same
/// `spawn_blocking` pool that serves git commands. Restoring a session lets N
/// tabs start at once, so the pool fills with tree walks and ordinary commands
/// queue behind them -- a `repo.head()` (a single ref read) was measured taking
/// 8.7 seconds during a 15-tab restore, which is queueing, not work.
///
/// Two keeps the disk busy without letting arming take over the pool. Arming is
/// background work: finishing all of it a little later costs nothing, while
/// starving the foreground is immediately visible.
const ARM_CONCURRENCY: usize = 2;

/// Gate limiting concurrent watch arming to [`ARM_CONCURRENCY`].
///
/// A plain counting semaphore over std primitives rather than tokio's: the
/// waiter is inside `spawn_blocking`, where blocking the thread is expected and
/// an async permit would need a runtime handle to await on.
struct ArmGate {
  available: Mutex<usize>,
  released: Condvar,
}

impl ArmGate {
  const fn new(permits: usize) -> Self {
    Self {
      available: Mutex::new(permits),
      released: Condvar::new(),
    }
  }

  /// Blocks until a permit is free, then holds it until the guard is dropped.
  ///
  /// Poisoning is recovered from rather than propagated: the guarded value is a
  /// plain counter that a panicking holder cannot leave torn, and treating a
  /// panic in one repo's arming as fatal to the gate would stop every later
  /// repo from ever being watched.
  fn acquire(&self) -> ArmPermit<'_> {
    let mut available = self.available.lock().unwrap_or_else(|e| e.into_inner());
    while *available == 0 {
      available = self
        .released
        .wait(available)
        .unwrap_or_else(|e| e.into_inner());
    }
    *available -= 1;
    ArmPermit { gate: self }
  }
}

struct ArmPermit<'a> {
  gate: &'a ArmGate,
}

impl Drop for ArmPermit<'_> {
  fn drop(&mut self) {
    // Released even if arming panicked, so one bad repo cannot wedge the gate.
    // Unwinding through here means the mutex is poisoned, so recover rather
    // than panic again -- a double panic would abort the process.
    *self
      .gate
      .available
      .lock()
      .unwrap_or_else(|e| e.into_inner()) += 1;
    self.gate.released.notify_one();
  }
}

static ARM_GATE: ArmGate = ArmGate::new(ARM_CONCURRENCY);

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
  ///
  /// Only [`ARM_CONCURRENCY`] repos arm at once; the rest wait their turn so a
  /// session restore cannot fill the blocking pool with tree walks.
  pub fn watch_deferred(app: AppHandle, repo_id: String, workdir: PathBuf) {
    tauri::async_runtime::spawn_blocking(move || {
      let queued = std::time::Instant::now();
      let _permit = ARM_GATE.acquire();
      let waited = queued.elapsed().as_millis();
      let start = std::time::Instant::now();
      let registry = app.state::<WatcherRegistry>();
      match registry.watch(app.clone(), repo_id.clone(), &workdir) {
        // Wait and work are reported separately: a long total is only a problem
        // when the *work* is long, and the old single number could not say which.
        Ok(()) => log::info!(
          "watch: armed in {}ms (waited {waited}ms) for {}",
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
  fn worktree_admin_changes_are_relevant() {
    // A worktree created or removed in a terminal lands here, and the section
    // must appear without the user clicking anything.
    assert!(path_is_relevant(
      &root().join(".git/worktrees/GitWyrm-feature/HEAD"),
      &root(),
      &empty()
    ));
    assert!(path_is_relevant(
      &root().join(".git/worktrees/GitWyrm-feature/gitdir"),
      &root(),
      &empty()
    ));
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

  /// The gate has to be a real ceiling, not a hint: exceeding it is what let
  /// arming fill the blocking pool and starve foreground git commands.
  #[test]
  fn arm_gate_never_exceeds_its_permit_count() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static GATE: ArmGate = ArmGate::new(2);
    let live = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    std::thread::scope(|scope| {
      for _ in 0..8 {
        let live = live.clone();
        let peak = peak.clone();
        scope.spawn(move || {
          let _permit = GATE.acquire();
          let now = live.fetch_add(1, Ordering::SeqCst) + 1;
          peak.fetch_max(now, Ordering::SeqCst);
          // Hold long enough that unbounded threads would overlap here.
          std::thread::sleep(Duration::from_millis(20));
          live.fetch_sub(1, Ordering::SeqCst);
        });
      }
    });

    assert_eq!(live.load(Ordering::SeqCst), 0, "every permit must be returned");
    assert!(
      peak.load(Ordering::SeqCst) <= 2,
      "at most 2 may arm at once, saw {}",
      peak.load(Ordering::SeqCst)
    );
  }

  /// A panic while arming must not consume a permit forever, or one unreadable
  /// repo would shrink the gate and eventually stop watching entirely.
  #[test]
  fn arm_gate_permit_survives_a_panic() {
    static GATE: ArmGate = ArmGate::new(1);

    let panicked = std::thread::spawn(|| {
      let _permit = GATE.acquire();
      panic!("arming blew up");
    })
    .join();
    assert!(panicked.is_err(), "the test thread should have panicked");

    // The permit is back, so this returns instead of deadlocking.
    let recovered = std::thread::spawn(|| {
      let _permit = GATE.acquire();
    })
    .join();
    assert!(recovered.is_ok(), "a panic must release its permit");
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
