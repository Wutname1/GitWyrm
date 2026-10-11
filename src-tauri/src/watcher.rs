//! Watches an open repository for external changes (editor saves, terminal
//! git commands) and emits a debounced `repo-changed` event to the frontend.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};
use std::time::{Duration, SystemTime};

use std::path::PathBuf;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify_debouncer_full::notify::RecommendedWatcher;
use notify_debouncer_full::{
    new_debouncer, notify::RecursiveMode, DebouncedEvent, Debouncer, RecommendedCache,
};
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

/// Where git keeps its state for a working tree, when that is outside it.
///
/// A linked worktree (`git worktree add`, and every agent run that works in its
/// own folder) has a `.git` *file* naming a folder inside the main checkout's
/// `.git/worktrees/`. Its HEAD, index and reflog live there, and its branches
/// live in the main checkout's shared `refs/`. None of that is under the
/// worktree's own folder, so a watch on the folder alone never saw a commit made
/// in it - the graph and the push count stayed where they were until something
/// else forced a refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GitDirs {
    /// This checkout's own git folder: HEAD, index, logs/HEAD.
    git_dir: PathBuf,
    /// The folder shared by every checkout of the repository: refs, packed-refs.
    common_dir: PathBuf,
}

/// Removes `.` and `..` without touching the disk, so a `commondir` of `../..`
/// compares equal to the paths `notify` reports. `canonicalize` would also do
/// this, but on Windows it returns `\?\` paths that never match the event
/// paths, which come back spelled the way the watch was armed.
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(comp.as_os_str());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The git folders for `workdir`. A normal checkout keeps both in `.git`; a
/// linked worktree points elsewhere through its `.git` file and that folder's
/// `commondir`. None when `.git` is missing or unreadable.
fn resolve_git_dirs(workdir: &Path) -> Option<GitDirs> {
    let dot_git = workdir.join(".git");
    if dot_git.is_dir() {
        return Some(GitDirs {
            git_dir: dot_git.clone(),
            common_dir: dot_git,
        });
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("gitdir:"))
        .map(str::trim)
        .filter(|t| !t.is_empty())?;
    let target = PathBuf::from(target);
    let git_dir = normalize_lexically(&if target.is_relative() {
        workdir.join(target)
    } else {
        target
    });
    let common_dir = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(raw) if !raw.trim().is_empty() => {
            let common = PathBuf::from(raw.trim());
            normalize_lexically(&if common.is_relative() {
                git_dir.join(common)
            } else {
                common
            })
        }
        _ => git_dir.clone(),
    };
    Some(GitDirs {
        git_dir,
        common_dir,
    })
}

impl GitDirs {
    /// Whether these folders sit outside the working tree, which is what needs
    /// watching on its own.
    fn is_external(&self, workdir: &Path) -> bool {
        !self.git_dir.starts_with(workdir)
    }

    /// Spell a path inside the external git folders as if it were under
    /// `<workdir>/.git`, so [`path_is_relevant`] judges it by the same rules as
    /// an ordinary checkout. Paths elsewhere come back unchanged.
    fn as_workdir_view(&self, path: &Path, workdir: &Path) -> PathBuf {
        // The worktree's own folder is inside the shared one, so it is checked
        // first: its HEAD must read as `.git/HEAD`, not `.git/worktrees/x/HEAD`.
        for base in [&self.git_dir, &self.common_dir] {
            if let Ok(rel) = path.strip_prefix(base) {
                return workdir.join(".git").join(rel);
            }
        }
        path.to_path_buf()
    }

    /// The files whose change means a commit, branch switch, reset or staging
    /// happened: HEAD, the index, the reflog for HEAD, packed refs, and the ref
    /// file of the branch HEAD names.
    fn state_files(&self) -> Vec<PathBuf> {
        let mut files = vec![
            self.git_dir.join("HEAD"),
            self.git_dir.join("index"),
            self.git_dir.join("logs").join("HEAD"),
            self.common_dir.join("packed-refs"),
        ];
        if let Ok(head) = std::fs::read_to_string(self.git_dir.join("HEAD")) {
            if let Some(name) = head.trim().strip_prefix("ref:") {
                let name = name.trim();
                // Only a plain refs/... name; anything else is not a file we read.
                if name.starts_with("refs/") && !name.contains("..") {
                    files.push(self.common_dir.join(name));
                }
            }
        }
        files
    }
}

/// Size and modification time of each git state file, in a fixed order. Two
/// equal signatures mean nothing a user can see in git has moved.
type StateSignature = Vec<Option<(SystemTime, u64)>>;

fn state_signature(dirs: &GitDirs) -> StateSignature {
    dirs.state_files()
        .iter()
        .map(|file| {
            std::fs::metadata(file)
                .ok()
                .map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()))
        })
        .collect()
}

/// How often the state poll looks at each repository's git state files.
///
/// The event watch is the fast path; this is the safety net. Windows drops
/// change notifications when its buffer overflows - an agent rewriting many
/// files at once, or a build running beside a commit - and `notify` then reports
/// nothing at all, so a commit made in that window never reached the screen.
/// Reading five file sizes per repository every couple of seconds costs nothing
/// and means a missed event delays the refresh by at most this long.
const STATE_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// One repository the state poll keeps an eye on.
struct PollEntry {
    dirs: GitDirs,
    /// Shared with the event handler, which records the state it just reported
    /// so the poll does not report the same change a second time.
    last: Arc<Mutex<StateSignature>>,
}

type RepoDebouncer = Debouncer<RecommendedWatcher, RecommendedCache>;

/// Arms the watch over `workdir`, skipping the directories in [`IGNORED_DIRS`].
///
/// A single recursive watch on the root would make the OS walk the entire tree,
/// including `node_modules` and `target` -- often the overwhelming majority of
/// the files in a project, and never anything we display. `IGNORED_DIRS` already
/// discards their events, but only *after* the walk has paid for them.
///
/// So the root is watched shallowly and each top-level entry is recursed into
/// individually, letting the heavy ones be skipped outright. The shallow root
/// watch is also what keeps this correct as the project changes: a top-level
/// directory created later surfaces as an event on the root, and the handler
/// arms it then.
///
/// `.git` is watched deliberately -- ref and index changes are how a commit made
/// in a terminal reaches the UI.
///
/// Returns the top-level directories it recursed into, which is what the tests
/// assert on.
fn arm_paths(debouncer: &mut RepoDebouncer, workdir: &Path) -> Result<Vec<PathBuf>, String> {
    // Non-recursive: catches top-level creates/deletes without descending.
    debouncer
        .watch(workdir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;

    let entries = match std::fs::read_dir(workdir) {
        Ok(entries) => entries,
        // An unreadable workdir is not fatal: the shallow root watch above is
        // already armed, so the repo stays partially watched rather than not at all.
        Err(e) => {
            log::warn!("watch: could not list {}: {e}", workdir.display());
            return Ok(Vec::new());
        }
    };

    let mut armed = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if IGNORED_DIRS.iter().any(|d| *d == name) {
            log::debug!("watch: skipping {name} in {}", workdir.display());
            continue;
        }
        // One heavy subtree must not stop the rest from being watched.
        match debouncer.watch(&entry.path(), RecursiveMode::Recursive) {
            Ok(()) => armed.push(entry.path()),
            Err(e) => log::warn!("watch: could not arm {}: {e}", entry.path().display()),
        }
    }

    Ok(armed)
}

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

/// Arms watches on a linked worktree's git folders, which live outside the
/// working tree [`arm_paths`] covers. A normal checkout needs nothing here: its
/// `.git` folder is one of the top-level entries `arm_paths` already recursed
/// into.
///
/// The worktree's own folder is small (HEAD, index, a reflog) and is watched
/// whole. The shared folder is not: its `objects/` is the bulk of the repository,
/// so only its top level (for `packed-refs`) and `refs/` are watched.
fn arm_git_dirs(debouncer: &mut RepoDebouncer, dirs: &GitDirs, workdir: &Path) {
    if !dirs.is_external(workdir) {
        return;
    }
    let targets = [
        (dirs.git_dir.clone(), RecursiveMode::Recursive),
        (dirs.common_dir.clone(), RecursiveMode::NonRecursive),
        (dirs.common_dir.join("refs"), RecursiveMode::Recursive),
    ];
    for (path, mode) in targets {
        if let Err(e) = debouncer.watch(&path, mode) {
            log::warn!("watch: could not arm git folder {}: {e}", path.display());
        }
    }
}

#[derive(Default)]
pub struct WatcherRegistry {
    watchers: Mutex<HashMap<String, RepoDebouncer>>,
    polls: Mutex<HashMap<String, PollEntry>>,
}

/// Started once, by the first repository to be watched.
static STATE_POLL: OnceLock<()> = OnceLock::new();

/// Runs for the life of the app: every [`STATE_POLL_INTERVAL`], compare each
/// watched repository's git state files with what was last reported and emit
/// `repo-changed` for any that moved without an event saying so.
fn start_state_poll(app: AppHandle) {
    STATE_POLL.get_or_init(|| {
        let spawned = std::thread::Builder::new()
            .name("repo-state-poll".into())
            .spawn(move || loop {
                std::thread::sleep(STATE_POLL_INTERVAL);
                let registry = app.state::<WatcherRegistry>();
                // Copied out so no disk access happens while the map is locked.
                let entries: Vec<(String, GitDirs, Arc<Mutex<StateSignature>>)> = registry
                    .polls
                    .lock()
                    .map(|polls| {
                        polls
                            .iter()
                            .map(|(id, e)| (id.clone(), e.dirs.clone(), e.last.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                for (repo_id, dirs, last) in entries {
                    let now = state_signature(&dirs);
                    let changed = {
                        let mut guard = last.lock().unwrap_or_else(|e| e.into_inner());
                        if *guard == now {
                            false
                        } else {
                            *guard = now;
                            true
                        }
                    };
                    if changed {
                        log::debug!(
                            "watch: state poll caught a change the events missed for {repo_id}"
                        );
                        let _ = app.emit("repo-changed", RepoChangedPayload { repo_id });
                    }
                }
            });
        if let Err(e) = spawned {
            log::warn!("watch: could not start the state poll: {e}");
        }
    });
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
            let result = registry.watch(app.clone(), repo_id.clone(), &workdir);
            let armed = start.elapsed().as_millis();

            // Also reported to Sentry, not just the local log. Arming is the leading
            // suspect for "opening a repo the first time is slow", and the log alone
            // cannot confirm it: it is release-only and needs the user to send a
            // file. Wait and work stay separate -- a long total only matters when the
            // *work* is long, and one number cannot say which.
            let tx =
                sentry::start_transaction(sentry::TransactionContext::new("watch.arm", "fs.watch"));
            tx.set_data("waited_ms", (waited as u64).into());
            tx.set_data("armed_ms", (armed as u64).into());
            tx.set_data("ok", result.is_ok().into());
            tx.finish();

            match result {
                Ok(()) => log::info!(
                    "watch: armed in {armed}ms (waited {waited}ms) for {}",
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
        let git_dirs = resolve_git_dirs(workdir);
        let git_dirs_for_events = git_dirs.clone();
        let last_state: Arc<Mutex<StateSignature>> = Arc::new(Mutex::new(
            git_dirs.as_ref().map(state_signature).unwrap_or_default(),
        ));
        let last_state_for_events = last_state.clone();
        let poll_app = app.clone();
        let mut debouncer = new_debouncer(
            Duration::from_millis(150),
            None,
            move |result: Result<
                Vec<DebouncedEvent>,
                Vec<notify_debouncer_full::notify::Error>,
            >| {
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

                    // A new top-level directory is only covered by the shallow root watch,
                    // which does not descend into it. Arm it now, or edits inside a folder
                    // created after open would never be seen. Cheap: `rearm_top_level`
                    // re-watches paths, which is idempotent, and only runs when a
                    // directory actually appears at the root.
                    let new_top_level = events
                        .iter()
                        .flat_map(|e| e.paths.iter())
                        .any(|p| p.parent() == Some(watch_root.as_path()) && p.is_dir());
                    if new_top_level {
                        WatcherRegistry::rearm_top_level(&app, &id, &watch_root);
                    }

                    // Only emit when at least one changed path maps to visible repo state.
                    // Build output and dependency-tree churn is filtered out so it can't
                    // drown out real edits (or, on Windows, overflow the OS event buffer
                    // and drop them). A batch with only noise is skipped entirely.
                    let guard = matcher_for_events.read();
                    let empty = Gitignore::empty();
                    let m = guard.as_deref().unwrap_or(&empty);
                    let relevant = events.iter().flat_map(|e| e.paths.iter()).any(|p| {
                        match &git_dirs_for_events {
                            Some(dirs) => path_is_relevant(
                                &dirs.as_workdir_view(p, &watch_root),
                                &watch_root,
                                m,
                            ),
                            None => path_is_relevant(p, &watch_root, m),
                        }
                    });
                    if relevant {
                        // Recorded so the state poll does not announce this same
                        // change again a moment later.
                        if let Some(dirs) = &git_dirs_for_events {
                            let now = state_signature(dirs);
                            *last_state_for_events
                                .lock()
                                .unwrap_or_else(|e| e.into_inner()) = now;
                        }
                        let _ = app.emit(
                            "repo-changed",
                            RepoChangedPayload {
                                repo_id: id.clone(),
                            },
                        );
                    }
                }
            },
        )
        .map_err(|e| e.to_string())?;

        arm_paths(&mut debouncer, workdir)?;
        if let Some(dirs) = &git_dirs {
            arm_git_dirs(&mut debouncer, dirs, workdir);
        }

        self.watchers
            .lock()
            .unwrap()
            .insert(repo_id.clone(), debouncer);
        if let Some(dirs) = git_dirs {
            self.polls.lock().unwrap().insert(
                repo_id,
                PollEntry {
                    dirs,
                    last: last_state,
                },
            );
        }
        start_state_poll(poll_app);
        Ok(())
    }

    /// Re-runs [`arm_paths`] for a repo whose root gained a directory.
    ///
    /// Watching an already-watched path is a no-op for `notify`, so re-arming the
    /// whole top level is simpler than tracking which entry is new, and costs one
    /// shallow `read_dir` -- the subtrees themselves are not re-walked.
    ///
    /// Silently does nothing if the repo has since been closed, which is the
    /// right outcome: the watch is gone because nobody is looking at it.
    fn rearm_top_level(app: &AppHandle, repo_id: &str, workdir: &Path) {
        let registry = app.state::<WatcherRegistry>();
        let mut watchers = registry.watchers.lock().unwrap();
        if let Some(debouncer) = watchers.get_mut(repo_id) {
            if let Err(e) = arm_paths(debouncer, workdir) {
                log::warn!("watch: could not re-arm {}: {e}", workdir.display());
            }
        }
    }

    pub fn unwatch(&self, repo_id: &str) {
        self.watchers.lock().unwrap().remove(repo_id);
        self.polls.lock().unwrap().remove(repo_id);
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
        assert!(path_is_relevant(
            &root().join("src/watcher.rs"),
            &root(),
            &empty()
        ));
        assert!(path_is_relevant(
            &root().join("README.md"),
            &root(),
            &empty()
        ));
    }

    #[test]
    fn build_output_is_ignored() {
        assert!(!path_is_relevant(
            &root().join("target/debug/foo.rlib"),
            &root(),
            &empty()
        ));
        assert!(!path_is_relevant(
            &root().join("node_modules/react/index.js"),
            &root(),
            &empty()
        ));
        assert!(!path_is_relevant(
            &root().join("dist/bundle.js"),
            &root(),
            &empty()
        ));
    }

    #[test]
    fn git_visible_state_is_relevant() {
        assert!(path_is_relevant(
            &root().join(".git/HEAD"),
            &root(),
            &empty()
        ));
        assert!(path_is_relevant(
            &root().join(".git/index"),
            &root(),
            &empty()
        ));
        assert!(path_is_relevant(
            &root().join(".git/refs/heads/main"),
            &root(),
            &empty()
        ));
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
        assert!(!path_is_relevant(
            &root().join(".git/objects/ab/cdef"),
            &root(),
            &empty()
        ));
        assert!(!path_is_relevant(
            &root().join(".git/index.lock"),
            &root(),
            &empty()
        ));
        assert!(!path_is_relevant(
            &root().join(".git/FETCH_HEAD"),
            &root(),
            &empty()
        ));
    }

    #[test]
    fn gitignored_paths_are_skipped() {
        let m = matcher_with(&[".venv/", "coverage/", "*.log"]);
        assert!(!path_is_relevant(
            &root().join(".venv/lib/x.py"),
            &root(),
            &m
        ));
        assert!(!path_is_relevant(
            &root().join("coverage/report.html"),
            &root(),
            &m
        ));
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

        assert_eq!(
            live.load(Ordering::SeqCst),
            0,
            "every permit must be returned"
        );
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

    /// Arming must cover real source directories while skipping the heavy ones.
    /// Verified through the watcher itself rather than a path helper, so it holds
    /// for what actually gets watched.
    #[test]
    fn arming_skips_heavy_directories() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        for name in ["src", "node_modules", "target", ".git"] {
            std::fs::create_dir(root.join(name)).expect("create dir");
        }
        std::fs::write(root.join("README.md"), "hi").expect("write file");

        let mut debouncer =
            new_debouncer(Duration::from_millis(50), None, move |_| {}).expect("debouncer");
        let armed = arm_paths(&mut debouncer, root).expect("arm");
        let names: Vec<String> = armed
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();

        // Source is watched, and `.git` with it -- ref and index changes are how a
        // commit made in a terminal reaches the UI.
        assert!(names.contains(&"src".to_string()), "source must be watched");
        assert!(names.contains(&".git".to_string()), ".git must be watched");

        // The heavy trees are never descended into, which is the whole point: their
        // events were already discarded, but the walk still paid for them.
        assert!(
            !names.contains(&"node_modules".to_string()),
            "node_modules must not be walked, got {names:?}"
        );
        assert!(
            !names.contains(&"target".to_string()),
            "target must not be walked, got {names:?}"
        );
    }

    /// A linked worktree keeps HEAD and the index in the main checkout's
    /// `.git/worktrees/<name>`, and its branches in the shared `refs/`. Both have
    /// to be found, or a commit made in the worktree never reaches the screen.
    #[test]
    fn a_linked_worktree_resolves_to_its_external_git_folders() {
        let dir = tempfile::tempdir().expect("temp dir");
        let main = dir.path().join("main");
        let admin = main.join(".git").join("worktrees").join("feature");
        let wt = dir.path().join("feature");
        std::fs::create_dir_all(&admin).expect("admin dir");
        std::fs::create_dir_all(&wt).expect("worktree dir");
        std::fs::write(
            admin.join("commondir"),
            "../..
",
        )
        .expect("commondir");
        std::fs::write(
            wt.join(".git"),
            format!(
                "gitdir: {}
",
                admin.display()
            ),
        )
        .expect(".git file");

        let dirs = resolve_git_dirs(&wt).expect("resolves");
        assert_eq!(dirs.git_dir, admin);
        assert_eq!(dirs.common_dir, main.join(".git"));
        assert!(dirs.is_external(&wt));

        // Its own HEAD reads as the worktree's .git/HEAD, and a branch moving in
        // the shared refs reads as a ref change - both relevant.
        let head = dirs.as_workdir_view(&admin.join("HEAD"), &wt);
        assert_eq!(head, wt.join(".git").join("HEAD"));
        assert!(path_is_relevant(&head, &wt, &empty()));
        let branch = dirs.as_workdir_view(&main.join(".git/refs/heads/feature"), &wt);
        assert!(path_is_relevant(&branch, &wt, &empty()));
        // Object writes in the shared folder are still noise.
        let object = dirs.as_workdir_view(&main.join(".git/objects/ab/cdef"), &wt);
        assert!(!path_is_relevant(&object, &wt, &empty()));
    }

    #[test]
    fn an_ordinary_checkout_keeps_git_inside_the_working_tree() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir(dir.path().join(".git")).expect(".git");
        let dirs = resolve_git_dirs(dir.path()).expect("resolves");
        assert!(!dirs.is_external(dir.path()));
        assert_eq!(dirs.git_dir, dirs.common_dir);
    }

    #[test]
    fn a_folder_without_git_resolves_to_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(resolve_git_dirs(dir.path()).is_none());
    }

    /// The safety net only works if a commit changes the signature. A commit
    /// moves the branch's ref file, which the signature follows through HEAD.
    #[test]
    fn a_branch_moving_changes_the_state_signature() {
        let dir = tempfile::tempdir().expect("temp dir");
        let git = dir.path().join(".git");
        std::fs::create_dir_all(git.join("refs/heads")).expect("refs");
        std::fs::write(
            git.join("HEAD"),
            "ref: refs/heads/main
",
        )
        .expect("HEAD");
        std::fs::write(git.join("refs/heads/main"), "a".repeat(40)).expect("ref");
        let dirs = resolve_git_dirs(dir.path()).expect("resolves");

        let before = state_signature(&dirs);
        assert_eq!(before, state_signature(&dirs), "stable when nothing moved");
        std::fs::write(git.join("refs/heads/main"), "b".repeat(41)).expect("move ref");
        assert_ne!(
            before,
            state_signature(&dirs),
            "a moved branch must be seen"
        );
    }

    #[test]
    fn lexical_normalization_resolves_parent_steps() {
        assert_eq!(
            normalize_lexically(Path::new("/a/b/.git/worktrees/x/../..")),
            PathBuf::from("/a/b/.git")
        );
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
