use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
}

impl OpenRepo {
  #[cfg(test)]
  pub fn for_test(repo: Repository) -> Self {
    Self {
      path: repo.workdir().expect("workdir").to_path_buf(),
      repo: Mutex::new(repo),
      commit_stats: Mutex::new(HashMap::new()),
    }
  }

  pub fn cached_stats(&self, oid: Oid) -> Option<ChangeStats> {
    self.commit_stats.lock().unwrap().get(&oid).copied()
  }

  pub fn store_stats(&self, oid: Oid, stats: ChangeStats) {
    self.commit_stats.lock().unwrap().insert(oid, stats);
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
