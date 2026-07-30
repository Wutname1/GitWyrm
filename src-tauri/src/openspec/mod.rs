//! OpenSpec integration: reads a repository's `openspec/` folder into typed
//! data, writes back the few things a human would write, and shells to the
//! OpenSpec CLI for validate and archive.
//!
//! Files are the database. There is no sidecar state: every surface in the app
//! renders what these modules parse, and the repo watcher (which already covers
//! the working tree) is what makes an edit from any editor or agent show up.

pub mod cli;
pub mod history;
pub mod parse;
pub mod write;

use std::path::{Path, PathBuf};

/// The `openspec/` directory for a repository, if it has one.
pub fn openspec_dir(repo_root: &Path) -> Option<PathBuf> {
  let dir = repo_root.join("openspec");
  dir.is_dir().then_some(dir)
}

// Note: there is deliberately no `openspec/`-specific watcher here. The repo
// watcher already covers the whole working tree and `openspec/` is an ordinary
// folder inside it, so a `repo-changed` event fires for spec edits from any tool.
// The frontend refreshes the spec queries on that event (see useRepoWatcher).

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_the_folder_only_when_present() {
    let dir = tempfile::tempdir().unwrap();
    assert!(openspec_dir(dir.path()).is_none());
    std::fs::create_dir(dir.path().join("openspec")).unwrap();
    assert!(openspec_dir(dir.path()).is_some());
  }
}
