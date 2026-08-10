use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use git2::{Oid, Repository};

use crate::error::AppError;

/// Files changed, insertions, deletions for one commit.
pub type ChangeStats = (u32, u32, u32);

/// One open repository. git2::Repository is !Sync, so all access goes through
/// the mutex and runs inside spawn_blocking.
pub struct OpenRepo {
  pub path: PathBuf,
  pub repo: Mutex<Repository>,
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
  /// Memoized "which commit owns lane zero", keyed by the HEAD it was computed
  /// for.
  ///
  /// Choosing it compares every local and remote ref against HEAD with
  /// `graph_ahead_behind`, which is a merge-base plus a revision count per ref
  /// -- on a repository with hundreds of refs that is the single most expensive
  /// part of building a log page. The answer depends only on HEAD and the ref
  /// tips, so recomputing it for page 2, 3, 4 ... of the same scroll is pure
  /// waste, and it showed up as a multi-hundred-millisecond stall each time the
  /// graph paged in more history.
  ///
  /// Keyed by HEAD so moving HEAD (checkout, commit, reset) misses and
  /// recomputes. A ref that moves without HEAD moving can leave this stale
  /// until the next HEAD change; that only affects which line renders straight
  /// through, never which commits or edges exist.
  pub primary_lane: Mutex<Option<(Oid, Oid)>>,
}

impl OpenRepo {
  #[cfg(test)]
  pub fn for_test(repo: Repository) -> Self {
    Self {
      path: repo.workdir().expect("workdir").to_path_buf(),
      repo: Mutex::new(repo),
      commit_stats: Mutex::new(HashMap::new()),
      status_read: Mutex::new(None),
      counts_read: Mutex::new(None),
      primary_lane: Mutex::new(None),
    }
  }

  pub fn cached_stats(&self, oid: Oid) -> Option<ChangeStats> {
    self.commit_stats.lock().unwrap().get(&oid).copied()
  }

  pub fn store_stats(&self, oid: Oid, stats: ChangeStats) {
    self.commit_stats.lock().unwrap().insert(oid, stats);
  }

  /// The memoized primary-lane commit, if it was computed for this same HEAD.
  pub fn cached_primary_lane(&self, head: Oid) -> Option<Oid> {
    match *self.primary_lane.lock().unwrap() {
      Some((cached_head, primary)) if cached_head == head => Some(primary),
      _ => None,
    }
  }

  pub fn store_primary_lane(&self, head: Oid, primary: Oid) {
    *self.primary_lane.lock().unwrap() = Some((head, primary));
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
  pub fn coalesced_read<T, E>(
    &self,
    slot: &Mutex<Option<Arc<SharedRead<T, E>>>>,
    f: impl FnOnce(&Repository) -> Result<T, E>,
  ) -> Result<T, E>
  where
    T: Clone,
    E: Clone,
  {
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
      // `None` means the leader gave up; fall through and read directly rather
      // than reporting a failure the repository never actually had.
      if let Some(value) = shared.wait() {
        return value;
      }
      let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
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
      let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
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
  pub fn open(&self, path: &str) -> Result<(String, Arc<OpenRepo>), AppError> {
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
      return Ok((id, existing.clone()));
    }

    let open = Arc::new(OpenRepo {
      path: workdir,
      repo: Mutex::new(repo),
      commit_stats: Mutex::new(HashMap::new()),
      status_read: Mutex::new(None),
      counts_read: Mutex::new(None),
      primary_lane: Mutex::new(None),
    });
    repos.insert(id.clone(), open.clone());
    Ok((id, open))
  }

  pub fn get(&self, id: &str) -> Result<Arc<OpenRepo>, AppError> {
    self
      .repos
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
      assert_eq!(result.as_ref().ok(), Some(&42), "every caller gets the result");
    }
    let runs = runs.load(Ordering::SeqCst);
    assert!(runs < 8, "reads must be shared, but ran {runs} times for 8 callers");
  }

  /// A later read must not be served the previous answer -- status changes, and
  /// a stale hit would leave the UI showing work that is already committed.
  #[test]
  fn a_later_read_runs_again() {
    let (_dir, open) = temp_repo();
    let slot: Mutex<Option<Arc<SharedRead<usize, String>>>> = Mutex::new(None);
    let runs = AtomicUsize::new(0);

    let mut run = || {
      open.coalesced_read(&slot, |_repo| {
        Ok::<usize, String>(runs.fetch_add(1, Ordering::SeqCst))
      })
    };

    assert_eq!(run().ok(), Some(0), "first read computes");
    assert_eq!(run().ok(), Some(1), "a sequential read must not reuse the slot");
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
