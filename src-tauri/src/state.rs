use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use git2::Repository;

use crate::error::AppError;

/// One open repository. git2::Repository is !Sync, so all access goes through
/// the mutex and runs inside spawn_blocking.
pub struct OpenRepo {
  pub path: PathBuf,
  pub repo: Mutex<Repository>,
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
    let open = Arc::new(OpenRepo { path: workdir, repo: Mutex::new(repo) });
    self.repos.lock().unwrap().insert(id.clone(), open.clone());
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
