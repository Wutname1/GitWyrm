use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::panic::Location;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, LockResult, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use git2::{Oid, Repository};

use crate::error::AppError;

/// Files changed, insertions, deletions for one commit.
pub type ChangeStats = (u32, u32, u32);

/// A wait for the repository lock longer than this gets a log line. Brief
/// queueing behind a status scan is normal; a wait past this means some caller
/// is visibly stalled behind whatever holds the lock.
const LOCK_WAIT_WARN: Duration = Duration::from_secs(1);

/// A *hold* of the repository lock longer than this gets a log line naming the
/// call site. Sentry showed episodes where reads across every tab queued for
/// minutes with near-identical durations -- the queueing signature -- but
/// nothing recorded what was holding the lock. This is that record.
const LOCK_HOLD_WARN: Duration = Duration::from_secs(5);

/// Idle read handles kept per repository. Enough for the per-repo pollers to
/// overlap; past this a handle is dropped rather than cached, since each one
/// keeps its own mmapped pack files alive.
const MAX_READ_HANDLES: usize = 8;

/// The repository mutex, instrumented so stalls name their culprit.
///
/// `lock()` behaves exactly like `Mutex<Repository>::lock()` -- same
/// `LockResult`, a guard that derefs to [`Repository`] -- so the many call
/// sites did not change. What it adds: the wait for the lock is timed, a long
/// wait is logged together with the call site it waited *behind*, and a long
/// hold is logged by the holder when it releases. `#[track_caller]` supplies
/// the call sites, so nothing threads names around by hand.
pub struct RepoLock {
    inner: Mutex<Repository>,
    /// Call site and acquisition time of the current holder; None when free.
    /// Only ever locked briefly, and never while waiting on `inner`.
    holder: Mutex<Option<(&'static Location<'static>, Instant)>>,
}

impl RepoLock {
    pub fn new(repo: Repository) -> Self {
        Self {
            inner: Mutex::new(repo),
            holder: Mutex::new(None),
        }
    }

    #[track_caller]
    pub fn lock(&self) -> LockResult<RepoGuard<'_>> {
        self.lock_at(Location::caller())
    }

    /// The lock body, attributed to `caller` rather than to this file --
    /// [`OpenRepo::coalesced_read`] forwards its own caller through here so a
    /// stall inside a coalesced read still names the command that asked.
    fn lock_at(&self, caller: &'static Location<'static>) -> LockResult<RepoGuard<'_>> {
        // Snapshot who holds the lock as the wait begins. If the wait turns out
        // long, this is the operation the time was spent behind.
        let behind = *self.holder.lock().unwrap_or_else(|e| e.into_inner());
        let wait_started = Instant::now();
        let result = self.inner.lock();
        let waited = wait_started.elapsed();
        if waited >= LOCK_WAIT_WARN {
            match behind {
                Some((held_by, since)) => log::warn!(
                    "{caller}: waited {}ms for the repository lock, behind {held_by} (which had held it for {}ms already)",
                    waited.as_millis(),
                    wait_started.saturating_duration_since(since).as_millis(),
                ),
                None => log::warn!(
                    "{caller}: waited {}ms for the repository lock",
                    waited.as_millis()
                ),
            }
        }

        match result {
            Ok(guard) => Ok(self.wrap(caller, guard)),
            Err(poisoned) => Err(PoisonError::new(self.wrap(caller, poisoned.into_inner()))),
        }
    }

    /// Record `caller` as the holder and hand back the timing guard.
    fn wrap<'a>(
        &'a self,
        caller: &'static Location<'static>,
        guard: MutexGuard<'a, Repository>,
    ) -> RepoGuard<'a> {
        *self.holder.lock().unwrap_or_else(|e| e.into_inner()) = Some((caller, Instant::now()));
        RepoGuard {
            lock: self,
            caller,
            acquired: Instant::now(),
            guard,
        }
    }
}

/// Guard for [`RepoLock`]; derefs to [`Repository`] so call sites read the
/// same as with a plain `MutexGuard`.
pub struct RepoGuard<'a> {
    lock: &'a RepoLock,
    caller: &'static Location<'static>,
    acquired: Instant,
    guard: MutexGuard<'a, Repository>,
}

impl Deref for RepoGuard<'_> {
    type Target = Repository;
    fn deref(&self) -> &Repository {
        &self.guard
    }
}

impl DerefMut for RepoGuard<'_> {
    fn deref_mut(&mut self) -> &mut Repository {
        &mut self.guard
    }
}

impl Drop for RepoGuard<'_> {
    fn drop(&mut self) {
        let held = self.acquired.elapsed();
        if held >= LOCK_HOLD_WARN {
            log::warn!(
                "{}: held the repository lock for {}ms",
                self.caller,
                held.as_millis()
            );
        }
        *self.lock.holder.lock().unwrap_or_else(|e| e.into_inner()) = None;
        // `guard` drops after this body, releasing the mutex itself.
    }
}

/// One open repository. git2::Repository is !Sync, so all access goes through
/// the mutex and runs inside spawn_blocking.
pub struct OpenRepo {
    pub path: PathBuf,
    pub repo: RepoLock,
    /// Memoized per-commit diff stats, keyed by commit id.
    ///
    /// Computing them is the dominant cost of building a log page: every commit
    /// needs a tree-to-tree diff with rename detection. A commit's stats can
    /// never change once it exists -- the id covers the trees on both sides -- so
    /// entries are valid for as long as the repository stays open and are only
    /// ever added, never invalidated.
    ///
    /// Separate from the repository mutex so a cache hit does not wait on
    /// whatever else is holding the repo.
    pub commit_stats: Mutex<HashMap<Oid, ChangeStats>>,
    /// The in-flight working-tree status read, if any. See
    /// [`OpenRepo::coalesced_read`]: one external change invalidates every open
    /// tab's status at once, and they would otherwise queue up repeating an
    /// identical scan of the same tree.
    pub status_read: Mutex<Option<Arc<SharedRead<crate::git::types::WorkingStatus, String>>>>,
    /// The in-flight tab-badge count read. Separate slot from `status_read` so a
    /// cheap count never waits on a full status scan that is already running.
    pub counts_read: Mutex<Option<Arc<SharedRead<crate::git::types::RepoCounts, String>>>>,
    /// Memoized "which commit owns lane zero", as `(head, ref tips, primary)`.
    ///
    /// Choosing it compares every local and remote ref against HEAD with
    /// `graph_ahead_behind`, which is a merge-base plus a revision count per ref
    /// -- on a repository with hundreds of refs that is the single most expensive
    /// part of building a log page. The answer depends only on HEAD and the ref
    /// tips, so recomputing it for page 2, 3, 4 ... of the same scroll is pure
    /// waste, and it showed up as a multi-hundred-millisecond stall each time the
    /// graph paged in more history.
    ///
    /// Both inputs are in the key, so moving HEAD (checkout, commit, reset) AND
    /// moving a ref without HEAD (a fetch, a push, a branch delete) both miss and
    /// recompute. Keying on HEAD alone made a fetch render the commits it brought
    /// in as a lane forking off history that never branched, since lane zero was
    /// still reserved for the tip the pre-fetch answer named.
    pub primary_lane: Mutex<Option<(Oid, u64, Oid)>>,
    /// Spare `Repository` handles for read-only work, so concurrent reads do not
    /// queue behind each other on [`OpenRepo::repo`].
    ///
    /// `git2::Repository` is `Send` but NOT `Sync`, so one handle cannot be
    /// shared by two threads at once and an `RwLock<Repository>` does not
    /// compile. A handle each is the way to get real read concurrency. Opening
    /// one costs ~14ms (see `open_repo` timings) and every reader already runs
    /// inside `spawn_blocking`, so the pool is built lazily and reused.
    read_handles: Mutex<Vec<Repository>>,
}

impl OpenRepo {
    #[cfg(test)]
    pub fn for_test(repo: Repository) -> Self {
        Self {
            path: repo.workdir().expect("workdir").to_path_buf(),
            repo: RepoLock::new(repo),
            commit_stats: Mutex::new(HashMap::new()),
            status_read: Mutex::new(None),
            counts_read: Mutex::new(None),
            primary_lane: Mutex::new(None),
            read_handles: Mutex::new(Vec::new()),
        }
    }

    pub fn cached_stats(&self, oid: Oid) -> Option<ChangeStats> {
        self.commit_stats.lock().unwrap().get(&oid).copied()
    }

    pub fn store_stats(&self, oid: Oid, stats: ChangeStats) {
        self.commit_stats.lock().unwrap().insert(oid, stats);
    }

    /// The memoized primary-lane commit, if it was computed for this same HEAD
    /// and the same set of ref tips.
    pub fn cached_primary_lane(&self, head: Oid, ref_tips: u64) -> Option<Oid> {
        match *self.primary_lane.lock().unwrap() {
            Some((cached_head, cached_tips, primary))
                if cached_head == head && cached_tips == ref_tips =>
            {
                Some(primary)
            }
            _ => None,
        }
    }

    pub fn store_primary_lane(&self, head: Oid, ref_tips: u64, primary: Oid) {
        *self.primary_lane.lock().unwrap() = Some((head, ref_tips, primary));
    }

    /// Runs `f` against a private read-only [`Repository`] handle, so it never
    /// waits on -- and never blocks -- any other reader.
    ///
    /// The convoy this exists to break was mostly reads blocking reads: the
    /// per-repo pollers (status, log, branches, stashes, merge state, worktrees)
    /// all invalidate together, and under the single mutex each waited for every
    /// other to finish an unrelated scan. Logs showed a cheap `get_merge_state`
    /// poll waiting 145s behind a commit-log read.
    ///
    /// Read-only by contract, and unenforced by the type system: `f` gets a
    /// `&Repository`, which is enough to call mutating git2 methods. It runs on
    /// a handle no writer holds, so a mutation here would race the write lock
    /// instead of being serialized by it. Anything that writes must use
    /// [`RepoLock::lock`].
    ///
    /// Falls back to the shared lock when a handle cannot be opened, so a repo
    /// that has become unreadable behaves exactly as it did before rather than
    /// failing in a new way.
    #[track_caller]
    pub fn with_read<T>(&self, f: impl FnOnce(&Repository) -> T) -> T {
        let caller = Location::caller();
        let Some(handle) = self.checkout_read_handle() else {
            let repo = self.repo.lock_at(caller).unwrap_or_else(|e| e.into_inner());
            return f(&repo);
        };

        // Returned to the pool even if `f` panics, so a single failed read does
        // not permanently shrink the pool.
        let mut handle = ReadHandle {
            owner: self,
            repo: Some(handle),
        };
        let started = Instant::now();
        let out = f(handle.repo.as_ref().expect("handle present"));
        let held = started.elapsed();
        if held >= LOCK_HOLD_WARN {
            log::warn!("{caller}: read took {}ms", held.as_millis());
        }
        handle.give_back();
        out
    }

    /// A private read-only handle, held for as long as the returned guard lives.
    ///
    /// The guard form of [`OpenRepo::with_read`], so a call site converts by
    /// swapping `self.repo.lock().unwrap()` for `self.read()` and leaving the
    /// body alone. Same read-only contract, same fallback to the shared lock.
    #[track_caller]
    pub fn read(&self) -> ReadGuard<'_> {
        let caller = Location::caller();
        match self.checkout_read_handle() {
            Some(repo) => ReadGuard {
                owner: Some(self),
                repo: Some(repo),
                shared: None,
                caller,
                acquired: Instant::now(),
            },
            // No handle: fall back to the shared lock so behaviour is unchanged
            // on a repository we cannot reopen.
            None => ReadGuard {
                owner: None,
                repo: None,
                shared: Some(self.repo.lock_at(caller).unwrap_or_else(|e| e.into_inner())),
                caller,
                acquired: Instant::now(),
            },
        }
    }

    /// Take a spare handle, opening one if the pool is empty.
    fn checkout_read_handle(&self) -> Option<Repository> {
        if let Some(repo) = self
            .read_handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop()
        {
            return Some(repo);
        }
        match Repository::open(&self.path) {
            Ok(repo) => Some(repo),
            Err(e) => {
                log::warn!("read handle open failed, falling back to shared lock: {e}");
                None
            }
        }
    }

    /// Runs `f` under the repository lock, but collapses concurrent callers that
    /// arrive while it is already running into that one run.
    ///
    /// A single external change (an editor save, a terminal commit) invalidates
    /// the status query for every tab at once, and each open tab asks for its own
    /// scan. Those are identical reads of the same working tree, so serializing
    /// them on the repository mutex means the last caller waits for all the
    /// others to finish repeating its work. Sharing one result is both faster and
    /// more correct: the tabs then agree, instead of showing snapshots taken at
    /// different moments.
    ///
    /// Only for pure reads. Anything that mutates the repository must take the
    /// lock directly, since a caller here may receive a result computed slightly
    /// before it asked.
    #[track_caller]
    pub fn coalesced_read<T, E>(
        &self,
        slot: &Mutex<Option<Arc<SharedRead<T, E>>>>,
        f: impl FnOnce(&Repository) -> Result<T, E>,
    ) -> Result<T, E>
    where
        T: Clone,
        E: Clone,
    {
        // The command's call site, not this file's: a stall in here should be
        // logged as the read that asked (status, counts), which is what the
        // lock instrumentation reports.
        let caller = Location::caller();

        // Join the in-flight read if there is one, otherwise become it.
        let (shared, leader) = {
            let mut guard = slot.lock().unwrap();
            match guard.as_ref() {
                Some(existing) => (existing.clone(), false),
                None => {
                    let fresh = Arc::new(SharedRead::default());
                    *guard = Some(fresh.clone());
                    (fresh, true)
                }
            }
        };

        if !leader {
            let wait_started = Instant::now();
            let published = shared.wait();
            let waited = wait_started.elapsed();
            if waited >= LOCK_WAIT_WARN {
                log::warn!(
                    "{caller}: waited {}ms for a shared repository read",
                    waited.as_millis()
                );
            }
            // `None` means the leader gave up; fall through and read directly rather
            // than reporting a failure the repository never actually had.
            if let Some(value) = published {
                return value;
            }
            let repo = self.repo.lock_at(caller).unwrap_or_else(|e| e.into_inner());
            return f(&repo);
        }

        // If the read panics, the waiters must be released rather than parked
        // forever, so unblocking is tied to the scope rather than to reaching the
        // end of it. They see `None` and each run the read themselves.
        let mut abandon = AbandonOnPanic {
            shared: &shared,
            done: false,
        };

        let result = {
            let repo = self.repo.lock_at(caller).unwrap_or_else(|e| e.into_inner());
            f(&repo)
        };
        // Clear the slot before publishing so the next caller starts a fresh read
        // rather than joining one that has already finished.
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = None;
        shared.publish(result.clone());
        abandon.done = true;
        result
    }
}

/// A borrowed read-only repository handle. Derefs to [`Repository`]; returns
/// itself to the pool on drop.
pub struct ReadGuard<'a> {
    owner: Option<&'a OpenRepo>,
    repo: Option<Repository>,
    /// Set only on the fallback path, where there was no spare handle.
    shared: Option<RepoGuard<'a>>,
    caller: &'static Location<'static>,
    acquired: Instant,
}

impl Deref for ReadGuard<'_> {
    type Target = Repository;
    fn deref(&self) -> &Repository {
        match (&self.repo, &self.shared) {
            (Some(repo), _) => repo,
            (None, Some(shared)) => shared,
            (None, None) => unreachable!("read guard always holds one of the two"),
        }
    }
}

impl Drop for ReadGuard<'_> {
    fn drop(&mut self) {
        let held = self.acquired.elapsed();
        if held >= LOCK_HOLD_WARN {
            log::warn!("{}: read took {}ms", self.caller, held.as_millis());
        }
        if let (Some(owner), Some(repo)) = (self.owner, self.repo.take()) {
            let mut pool = owner.read_handles.lock().unwrap_or_else(|e| e.into_inner());
            if pool.len() < MAX_READ_HANDLES {
                pool.push(repo);
            }
        }
    }
}

/// Returns a borrowed read handle to its pool, panic or not.
struct ReadHandle<'a> {
    owner: &'a OpenRepo,
    repo: Option<Repository>,
}

impl ReadHandle<'_> {
    fn give_back(&mut self) {
        if let Some(repo) = self.repo.take() {
            let mut pool = self
                .owner
                .read_handles
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            // Cap the pool so a burst of parallel reads does not leave dozens of
            // open handles (and their mmapped packs) alive for the session.
            if pool.len() < MAX_READ_HANDLES {
                pool.push(repo);
            }
        }
    }
}

impl Drop for ReadHandle<'_> {
    fn drop(&mut self) {
        self.give_back();
    }
}

/// Releases everyone waiting on a read that panicked partway through.
struct AbandonOnPanic<'a, T, E> {
    shared: &'a SharedRead<T, E>,
    done: bool,
}

impl<T, E> Drop for AbandonOnPanic<'_, T, E> {
    fn drop(&mut self) {
        if !self.done {
            self.shared.abandon();
        }
    }
}

/// A read that several callers are waiting on. See [`OpenRepo::coalesced_read`].
pub struct SharedRead<T, E> {
    result: Mutex<Option<Result<T, E>>>,
    ready: Condvar,
}

impl<T, E> Default for SharedRead<T, E> {
    fn default() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
        }
    }
}

impl<T, E> SharedRead<T, E> {
    /// Wakes the waiters without a result. Used when the leader panicked, so they
    /// stop waiting on an answer that is never coming.
    fn abandon(&self) {
        self.ready.notify_all();
    }
}

impl<T: Clone, E: Clone> SharedRead<T, E> {
    fn publish(&self, value: Result<T, E>) {
        *self.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
        self.ready.notify_all();
    }

    /// Blocks until the leader publishes, or returns `None` if it gave up.
    fn wait(&self) -> Option<Result<T, E>> {
        let mut guard = self.result.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(value) = guard.as_ref() {
                return Some(value.clone());
            }
            let (next, _) = self
                .ready
                .wait_timeout(guard, std::time::Duration::from_secs(30))
                .unwrap_or_else(|e| e.into_inner());
            guard = next;
            // Woken with nothing published means the leader panicked or gave up, and
            // the timeout covers the case where it died without unwinding. Either
            // way, stop waiting -- the caller reads for itself instead.
            if guard.is_none() {
                return None;
            }
        }
    }
}

#[derive(Default)]
pub struct RepoManager {
    repos: Mutex<HashMap<String, Arc<OpenRepo>>>,
}

impl RepoManager {
    /// Opens `path`, returning its id, handle, and whether an existing handle was
    /// reused.
    ///
    /// The reuse flag exists for the open-latency measurement: "slow the first
    /// time" cannot be read from a duration alone, since a warm reopen skips
    /// nearly all the work and would otherwise sit in the same average.
    pub fn open(&self, path: &str) -> Result<(String, Arc<OpenRepo>, bool), AppError> {
        let repo = Repository::discover(path)?;
        let workdir = repo
            .workdir()
            .ok_or_else(|| AppError::Other("bare repositories are not supported".into()))?
            .to_path_buf();

        let id = repo_id(&workdir);

        // Reuse the handle when this path is already open. Launch restore opens
        // every tab at once, so two calls can race here; replacing a live handle
        // would leave in-flight work holding a repository nobody can look up.
        let mut repos = self.repos.lock().unwrap();
        if let Some(existing) = repos.get(&id) {
            return Ok((id, existing.clone(), true));
        }

        let open = Arc::new(OpenRepo {
            path: workdir,
            repo: RepoLock::new(repo),
            commit_stats: Mutex::new(HashMap::new()),
            status_read: Mutex::new(None),
            counts_read: Mutex::new(None),
            primary_lane: Mutex::new(None),
            read_handles: Mutex::new(Vec::new()),
        });
        repos.insert(id.clone(), open.clone());
        Ok((id, open, false))
    }

    pub fn get(&self, id: &str) -> Result<Arc<OpenRepo>, AppError> {
        self.repos
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::Other(format!("repository not open: {id}")))
    }

    pub fn close(&self, id: &str) {
        self.repos.lock().unwrap().remove(id);
    }
}

/// The id a workdir would get if opened.
///
/// Public so worktree removal can find and release the handle for a folder it
/// is about to delete, without needing that folder to be open as a tab first.
pub fn repo_id_for(workdir: &Path) -> String {
    repo_id(&workdir.to_path_buf())
}

fn repo_id(workdir: &PathBuf) -> String {
    // Stable, filesystem-derived id; good enough as a cache key on the frontend.
    let s = workdir.to_string_lossy().to_lowercase().replace('\\', "/");
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    fn temp_repo() -> (tempfile::TempDir, OpenRepo) {
        let dir = tempfile::tempdir().expect("temp repo");
        let repo = Repository::init(dir.path()).expect("repo");
        let open = OpenRepo::for_test(repo);
        (dir, open)
    }

    /// The point of the read pool: two reads must be able to hold the repository
    /// at the same time. Under the old single mutex the second would block until
    /// the first returned, which is the convoy that made a cheap poll wait
    /// minutes behind an unrelated scan.
    #[test]
    fn reads_overlap_instead_of_queueing() {
        let (_dir, open) = temp_repo();
        let open = Arc::new(open);

        let both_inside = Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let open = open.clone();
            let barrier = both_inside.clone();
            handles.push(std::thread::spawn(move || {
                let repo = open.read();
                assert!(repo.workdir().is_some(), "read guard derefs to the repo");
                // Only completes if BOTH threads are inside a read at once;
                // a mutual-exclusion bug deadlocks here instead of failing late.
                barrier.wait();
            }));
        }
        for h in handles {
            h.join().expect("reader thread");
        }
    }

    /// Handles are returned after use rather than reopened every time.
    #[test]
    fn read_handles_return_to_the_pool() {
        let (_dir, open) = temp_repo();
        assert_eq!(open.read_handles.lock().unwrap().len(), 0, "starts empty");
        {
            let _repo = open.read();
        }
        assert_eq!(
            open.read_handles.lock().unwrap().len(),
            1,
            "the handle is back in the pool"
        );
        {
            let _repo = open.read();
        }
        assert_eq!(
            open.read_handles.lock().unwrap().len(),
            1,
            "the pooled handle is reused rather than a second one opened"
        );
    }

    /// The instrumented lock must behave exactly like the plain mutex it
    /// replaced: deref to the repository, and hand back a usable guard through
    /// `into_inner` after a holder panicked.
    #[test]
    fn repo_lock_derefs_and_recovers_from_poison() {
        let (_dir, open) = temp_repo();

        {
            let repo = open.repo.lock().unwrap();
            assert!(repo.workdir().is_some(), "guard derefs to the repository");
        }

        // Poison it: panic while holding the guard.
        let poisoned = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _repo = open.repo.lock().unwrap();
                    panic!("holder blew up");
                })
                .join()
        });
        assert!(poisoned.is_err(), "the holder should have panicked");

        // The recovery path every internal caller uses still works.
        let repo = open.repo.lock().unwrap_or_else(|e| e.into_inner());
        assert!(repo.workdir().is_some(), "poison recovery yields the guard");
    }

    /// The point of coalescing: tabs asking at the same time share one scan
    /// instead of each repeating it while the others wait on the repo lock.
    #[test]
    fn concurrent_reads_share_one_run() {
        let (_dir, open) = temp_repo();
        let slot: Mutex<Option<Arc<SharedRead<usize, String>>>> = Mutex::new(None);
        let runs = AtomicUsize::new(0);

        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        open.coalesced_read(&slot, |_repo| {
                            runs.fetch_add(1, Ordering::SeqCst);
                            // Long enough that the others pile up behind this one.
                            std::thread::sleep(Duration::from_millis(50));
                            Ok::<usize, String>(42)
                        })
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("thread"))
                .collect::<Vec<_>>()
        });

        // Everyone gets the answer, and nobody re-derives it needlessly.
        for result in &results {
            assert_eq!(
                result.as_ref().ok(),
                Some(&42),
                "every caller gets the result"
            );
        }
        let runs = runs.load(Ordering::SeqCst);
        assert!(
            runs < 8,
            "reads must be shared, but ran {runs} times for 8 callers"
        );
    }

    /// A later read must not be served the previous answer -- status changes, and
    /// a stale hit would leave the UI showing work that is already committed.
    #[test]
    fn a_later_read_runs_again() {
        let (_dir, open) = temp_repo();
        let slot: Mutex<Option<Arc<SharedRead<usize, String>>>> = Mutex::new(None);
        let runs = AtomicUsize::new(0);

        let run = || {
            open.coalesced_read(&slot, |_repo| {
                Ok::<usize, String>(runs.fetch_add(1, Ordering::SeqCst))
            })
        };

        assert_eq!(run().ok(), Some(0), "first read computes");
        assert_eq!(
            run().ok(),
            Some(1),
            "a sequential read must not reuse the slot"
        );
        assert_eq!(run().ok(), Some(2), "and again");
    }

    /// A panicking leader must not park its waiters forever; they fall back to
    /// reading for themselves.
    #[test]
    fn waiters_survive_a_panicking_leader() {
        let (_dir, open) = temp_repo();
        let slot: Mutex<Option<Arc<SharedRead<usize, String>>>> = Mutex::new(None);

        // The leader claims the slot, then dies inside the read.
        let leader = std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                open.coalesced_read(&slot, |_repo| -> Result<usize, String> {
                    panic!("scan blew up");
                })
            });
            handle.join()
        });
        assert!(leader.is_err(), "the leader should have panicked");

        // A caller arriving afterwards still gets a real answer.
        let after = open.coalesced_read(&slot, |_repo| Ok::<usize, String>(7));
        assert_eq!(after.ok(), Some(7), "a panic must not wedge later reads");
    }
}
