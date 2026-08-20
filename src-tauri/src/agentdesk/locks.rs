//! Per-session serialization for the durable session store.
//!
//! Every mutating command in `commands/agent_desk.rs`, plus the run bridge's
//! `route_run_event`, does the same read-modify-write: load a session's JSON
//! file, mutate it in memory, write it back with an atomic rename. Each
//! `#[tauri::command]` runs on its own `spawn_blocking` task, so two calls
//! for the *same* session ID can interleave -- both read the same on-disk
//! state, and whichever writes last silently wins, discarding the other's
//! change. Concretely: a user archives a session while a linked run event is
//! routed through `route_run_event` at the same moment. Both read
//! `archived: false`; archive writes `archived: true`; the bridge, still
//! holding its stale copy, appends a message and writes afterward, reverting
//! `archived` back to `false` with no error surfaced anywhere.
//!
//! [`SessionLocks`] closes that window: every mutating path acquires the
//! mutex for its `session_id` and holds it across the full
//! read-modify-write, so two mutations of the same session are strictly
//! ordered. Different sessions never contend with each other -- the registry
//! hands out one mutex per ID, following [`crate::state::RepoManager`]'s
//! shape (a `Mutex<HashMap<..>>` of `Arc`-shared per-key locks). The guard
//! itself mirrors [`crate::state::RepoLock`]: same timing and logging for a
//! long wait or a long hold, so a stall here shows up the same way a
//! repository-lock stall does.
//!
//! Lock-ordering note: nothing here is ever acquired while holding a
//! [`super::bridge::RunSessionLinks`] lock, and `RunSessionLinks` is never
//! acquired while holding a session lock -- `route_run_event` reads the link
//! (and, in `commands/airun.rs`, the next sequence number) *before* taking
//! the session lock, then never touches `RunSessionLinks` again for the rest
//! of the read-modify-write. One fixed order, so there is no cycle to
//! deadlock on.

use std::collections::HashMap;
use std::panic::Location;
use std::sync::{Arc, LockResult, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// A wait for a session lock longer than this gets a log line, matching
/// [`crate::state::RepoLock`]'s threshold.
const LOCK_WAIT_WARN: Duration = Duration::from_secs(1);

/// A *hold* of a session lock longer than this gets a log line naming the
/// call site, matching [`crate::state::RepoLock`]'s threshold.
const LOCK_HOLD_WARN: Duration = Duration::from_secs(5);

/// One session's mutex, instrumented so a stall names its culprit. Identical
/// shape to [`crate::state::RepoLock`], just guarding `()` instead of a
/// `Repository` -- callers hold the guard for the duration of their own
/// read-modify-write rather than storing anything in it.
struct SessionLock {
    inner: Mutex<()>,
    holder: Mutex<Option<(&'static Location<'static>, Instant)>>,
}

impl SessionLock {
    fn new() -> Self {
        Self {
            inner: Mutex::new(()),
            holder: Mutex::new(None),
        }
    }

    fn lock(&self, caller: &'static Location<'static>) -> LockResult<SessionGuard<'_>> {
        let behind = *self.holder.lock().unwrap_or_else(|e| e.into_inner());
        let wait_started = Instant::now();
        let result = self.inner.lock();
        let waited = wait_started.elapsed();
        if waited >= LOCK_WAIT_WARN {
            match behind {
                Some((held_by, since)) => log::warn!(
                    "{caller}: waited {}ms for a session lock, behind {held_by} (which had held it for {}ms already)",
                    waited.as_millis(),
                    wait_started.saturating_duration_since(since).as_millis(),
                ),
                None => log::warn!(
                    "{caller}: waited {}ms for a session lock",
                    waited.as_millis()
                ),
            }
        }

        match result {
            Ok(guard) => Ok(self.wrap(caller, guard)),
            Err(poisoned) => Err(PoisonError::new(self.wrap(caller, poisoned.into_inner()))),
        }
    }

    fn wrap<'a>(
        &'a self,
        caller: &'static Location<'static>,
        guard: MutexGuard<'a, ()>,
    ) -> SessionGuard<'a> {
        *self.holder.lock().unwrap_or_else(|e| e.into_inner()) = Some((caller, Instant::now()));
        SessionGuard {
            lock: self,
            caller,
            acquired: Instant::now(),
            _guard: guard,
        }
    }
}

/// Guard for [`SessionLock`]. Callers only care that it is held for the
/// scope of their read-modify-write, so it carries no payload -- unlike
/// [`crate::state::RepoGuard`], there is nothing to deref to.
struct SessionGuard<'a> {
    lock: &'a SessionLock,
    caller: &'static Location<'static>,
    acquired: Instant,
    _guard: MutexGuard<'a, ()>,
}

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        let held = self.acquired.elapsed();
        if held >= LOCK_HOLD_WARN {
            log::warn!(
                "{}: held a session lock for {}ms",
                self.caller,
                held.as_millis()
            );
        }
        *self.lock.holder.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Registry of per-session-ID mutexes, one entry created lazily per ID and
/// kept for the life of the process (sessions number in the dozens per day,
/// per architecture.md section 2, so this never grows unreasonably large).
///
/// Managed as Tauri state (`app.manage(SessionLocks::default())`), shared by
/// every command in `commands/agent_desk.rs` and by
/// `agentdesk::bridge::route_run_event`.
#[derive(Default)]
pub struct SessionLocks {
    locks: Mutex<HashMap<String, Arc<SessionLock>>>,
}

impl SessionLocks {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_for(&self, session_id: &str) -> Arc<SessionLock> {
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        locks
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(SessionLock::new()))
            .clone()
    }

    /// Runs `f` while holding the mutex for `session_id`. Any other call to
    /// [`with_session_lock`](Self::with_session_lock) for the *same* ID,
    /// from any thread, blocks until `f` returns -- this is what makes a
    /// read-modify-write on a session atomic with respect to every other
    /// mutating path for that session. Calls for a *different* session never
    /// wait on this one.
    #[track_caller]
    pub fn with_session_lock<T>(&self, session_id: &str, f: impl FnOnce() -> T) -> T {
        let caller = Location::caller();
        let lock = self.lock_for(session_id);
        let _guard = lock.lock(caller).unwrap_or_else(|e| e.into_inner());
        f()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;

    /// Two threads racing a read-modify-write on the *same* session must not
    /// interleave: whichever runs second must observe the first's effect, so
    /// neither update is lost. This is the regression test for the bug the
    /// lock exists to close -- run without `with_session_lock` wrapping the
    /// critical section, it fails intermittently (a lost update).
    #[test]
    fn same_session_read_modify_write_never_loses_an_update() {
        let locks = SessionLocks::new();
        // A tiny shared "document": starts at 0, each of two threads should
        // add its own contribution exactly once.
        let state = Mutex::new(0i64);
        let barrier = Barrier::new(2);

        std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                locks.with_session_lock("sess-1", || {
                    let current = *state.lock().unwrap();
                    // Force a window where an unsynchronized second writer
                    // could read the same "before" value.
                    std::thread::sleep(Duration::from_millis(20));
                    *state.lock().unwrap() = current + 10;
                });
            });
            let b = scope.spawn(|| {
                barrier.wait();
                locks.with_session_lock("sess-1", || {
                    let current = *state.lock().unwrap();
                    std::thread::sleep(Duration::from_millis(20));
                    *state.lock().unwrap() = current + 1;
                });
            });
            a.join().unwrap();
            b.join().unwrap();
        });

        // If both read-modify-writes were serialized, the result is 11
        // regardless of order. A lost update yields 10 or 1.
        assert_eq!(
            *state.lock().unwrap(),
            11,
            "a lost update means the two read-modify-writes interleaved"
        );
    }

    /// Locks for different session IDs must not contend with each other --
    /// otherwise the registry would serialize unrelated sessions, which is
    /// strictly worse than the whole-store mutex this replaces.
    #[test]
    fn different_sessions_do_not_block_each_other() {
        let locks = SessionLocks::new();
        let concurrent = AtomicUsize::new(0);
        let max_concurrent = AtomicUsize::new(0);
        let barrier = Barrier::new(2);

        let run = |session_id: &str| {
            barrier.wait();
            locks.with_session_lock(session_id, || {
                let now = concurrent.fetch_add(1, Ordering::SeqCst) + 1;
                max_concurrent.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(50));
                concurrent.fetch_sub(1, Ordering::SeqCst);
            });
        };

        std::thread::scope(|scope| {
            let a = scope.spawn(|| run("sess-a"));
            let b = scope.spawn(|| run("sess-b"));
            a.join().unwrap();
            b.join().unwrap();
        });

        assert_eq!(
            max_concurrent.load(Ordering::SeqCst),
            2,
            "two different sessions should run their critical sections concurrently"
        );
    }

    /// A panic while holding the lock must not poison it forever -- the
    /// house convention (`unwrap_or_else(|e| e.into_inner())`) recovers, same
    /// as `RepoLock`.
    #[test]
    fn a_panicking_holder_does_not_wedge_later_callers() {
        let locks = SessionLocks::new();

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            locks.with_session_lock("sess-1", || {
                panic!("holder blew up");
            });
        }));
        assert!(panicked.is_err());

        // A later call for the same session must still work.
        let ran = locks.with_session_lock("sess-1", || 42);
        assert_eq!(ran, 42);
    }
}
