use std::collections::HashMap;

use git2::{DiffOptions, Status, StatusOptions};
use tauri::State;

use crate::error::AppError;
use crate::git::submodule::moved_submodules;
use crate::git::types::{FileChange, RepoCounts, StatusCode, WorkingStatus};
use crate::state::RepoManager;

/// Per-file +/- line counts for a diff.
fn line_stats(diff: &git2::Diff) -> HashMap<String, (u32, u32)> {
  let mut stats: HashMap<String, (u32, u32)> = HashMap::new();
  let _ = diff.foreach(
    &mut |_, _| true,
    None,
    None,
    Some(&mut |delta, _hunk, line| {
      if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
        let entry = stats.entry(path.to_string_lossy().into_owned()).or_default();
        match line.origin() {
          '+' => entry.0 += 1,
          '-' => entry.1 += 1,
          _ => {}
        }
      }
      true
    }),
  );
  stats
}

/// Current and previous path for a status entry's delta.
///
/// `StatusEntry::path()` returns the delta's *old* path, which for a rename is
/// a name that no longer exists on disk. Prefer the delta's new path, and
/// report the old one only when the two actually differ.
fn delta_paths(delta: Option<git2::DiffDelta>, fallback: &str) -> (String, Option<String>) {
  let Some(delta) = delta else {
    return (fallback.to_string(), None);
  };
  let as_str = |f: git2::DiffFile| f.path().map(|p| p.to_string_lossy().into_owned());
  let new_path = as_str(delta.new_file()).unwrap_or_else(|| fallback.to_string());
  let old_path = as_str(delta.old_file()).filter(|old| *old != new_path);
  (new_path, old_path)
}

#[tauri::command]
#[specta::specta]
pub async fn get_status(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<WorkingStatus, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let _timing = crate::perf::CommandTiming::start("get_status", "git.status");
    // Coalesced: one external change invalidates the status of every open tab
    // at once, and scanning the same working tree N times only makes the last
    // tab wait for the others to repeat its work.
    let slot = &open.status_read;
    open
      .coalesced_read(slot, |repo| working_status(repo).map_err(|e: AppError| e.to_string()))
      .map_err(AppError::Other)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// The push / pull / uncommitted numbers for a repository tab's badge.
///
/// A tab shows three numbers, but the only way to get them used to be
/// [`get_status`] plus `list_branches` -- between them a full status walk, two
/// content diffs to count changed lines, a submodule scan, and an ahead/behind
/// pass over every branch in the repo. Every open tab ran all of that, on every
/// external change, to render three integers it then threw the rest away from.
///
/// This asks for exactly the three. The savings are in what it leaves out:
///
/// - no per-file diffs, so file *contents* are never read;
/// - no rename detection, which cannot change a total (a rename is one file
///   either way, and pairing it with its old name only matters for display);
/// - only the checked-out branch's upstream, not every branch's.
///
/// It stays on the same refresh path as everything else, so the numbers are
/// exactly as fresh as before -- this is the same answer computed cheaply, not
/// a cached or delayed one.
#[tauri::command]
#[specta::specta]
pub async fn get_repo_counts(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<RepoCounts, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let _timing = crate::perf::CommandTiming::start("get_repo_counts", "git.counts");
    let slot = &open.counts_read;
    open
      .coalesced_read(slot, |repo| {
        repo_counts(repo).map_err(|e: AppError| e.to_string())
      })
      .map_err(AppError::Other)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

fn repo_counts(repo: &git2::Repository) -> Result<RepoCounts, AppError> {
  let mut opts = StatusOptions::new();
  opts
    .include_untracked(true)
    // A new folder still counts as work waiting to be committed, so its files
    // have to be reached -- but git can stop at the folder rather than listing
    // everything inside it, which is the expensive part in a fresh build tree.
    .recurse_untracked_dirs(false)
    // Deliberately no rename detection: it pairs a delete with an add for
    // display, and pairing cannot change how many files are involved.
    .include_ignored(false)
    // Read-only. `get_status` refreshes the index; doing it here too would mean
    // every background tab writing to `.git` on every change.
    .update_index(false);
  let statuses = repo.statuses(Some(&mut opts))?;

  let mut uncommitted = 0u32;
  for entry in statuses.iter() {
    let st = entry.status();
    // Counted once per file, even when it is changed both in the index and in
    // the working tree -- it is one piece of uncommitted work either way. This
    // matches what the badge means, though it can differ from `get_status`,
    // whose two lists show such a file in both.
    if st.intersects(
      Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE
        | Status::WT_NEW
        | Status::WT_MODIFIED
        | Status::WT_DELETED
        | Status::WT_RENAMED
        | Status::WT_TYPECHANGE
        | Status::CONFLICTED,
    ) {
      uncommitted += 1;
    }
  }

  // Only the checked-out branch has an upstream worth reporting on a tab.
  // A detached HEAD or a branch that was never pushed has nothing to sync,
  // which reads as zero rather than as an error.
  let (ahead, behind) = head_sync(repo).unwrap_or((0, 0));

  Ok(RepoCounts {
    ahead,
    behind,
    uncommitted,
  })
}

/// Ahead/behind for the checked-out branch against its upstream.
fn head_sync(repo: &git2::Repository) -> Option<(u32, u32)> {
  let head = repo.head().ok()?;
  if !head.is_branch() {
    return None;
  }
  let local = head.peel_to_commit().ok()?.id();
  let branch = git2::Branch::wrap(head);
  let upstream = branch.upstream().ok()?.get().peel_to_commit().ok()?.id();
  let (ahead, behind) = repo.graph_ahead_behind(local, upstream).ok()?;
  Some((ahead as u32, behind as u32))
}

/// Reads the working tree: staged and unstaged changes with per-file line
/// counts. Split out from the command so it can run under a coalesced read.
fn working_status(repo: &git2::Repository) -> Result<WorkingStatus, AppError> {
  {
    let mut opts = StatusOptions::new();
    opts
      .include_untracked(true)
      .recurse_untracked_dirs(true)
      .renames_head_to_index(true)
      // Without this, a rename that is not yet staged is reported as a delete
      // plus an add, so the same change looks different before and after
      // staging. Detect it on both sides so the two agree.
      .renames_index_to_workdir(true)
      .update_index(true);
    let statuses = repo.statuses(Some(&mut opts))?;

    // Line counts: staged = HEAD tree -> index; unstaged = index -> workdir.
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let mut diff_opts = DiffOptions::new();
    diff_opts
      .include_untracked(true)
      .show_untracked_content(true)
      .recurse_untracked_dirs(true);
    let staged_stats = repo
      .diff_tree_to_index(head_tree.as_ref(), None, None)
      .map(|d| line_stats(&d))
      .unwrap_or_default();
    let unstaged_stats = repo
      .diff_index_to_workdir(None, Some(&mut diff_opts))
      .map(|d| line_stats(&d))
      .unwrap_or_default();

    // Submodule pointer moves, keyed by path. A submodule shows up as a plain
    // WT_MODIFIED entry here; we tag those so the frontend can treat them
    // differently (ordinary discard/stash can't move a submodule pointer).
    let submodules = moved_submodules(&repo);

    let mut staged = Vec::new();
    let mut unstaged = Vec::new();

    for entry in statuses.iter() {
      let path = entry.path().unwrap_or("").to_string();
      let st = entry.status();

      if st.is_conflicted() {
        unstaged.push(FileChange {
          path: path.clone(),
          old_path: None,
          status: StatusCode::Conflicted,
          additions: 0,
          deletions: 0,
          conflicted: true,
          submodule: None,
        });
        continue;
      }

      if st.intersects(
        Status::INDEX_NEW | Status::INDEX_MODIFIED | Status::INDEX_DELETED | Status::INDEX_RENAMED | Status::INDEX_TYPECHANGE,
      ) {
        let code = if st.contains(Status::INDEX_NEW) {
          StatusCode::Added
        } else if st.contains(Status::INDEX_DELETED) {
          StatusCode::Deleted
        } else if st.contains(Status::INDEX_RENAMED) {
          StatusCode::Renamed
        } else {
          StatusCode::Modified
        };
        // `entry.path()` reports the *old* name for a rename, so a staged
        // rename would be listed under a file that no longer exists. Take the
        // current name from the delta and keep the old one alongside it.
        let (new_path, old_path) = delta_paths(entry.head_to_index(), &path);
        let (a, d) = staged_stats.get(&new_path).copied().unwrap_or((0, 0));
        staged.push(FileChange {
          old_path,
          status: code,
          additions: a,
          deletions: d,
          conflicted: false,
          submodule: submodules.get(&new_path).cloned(),
          path: new_path,
        });
      }

      if st.intersects(
        Status::WT_NEW | Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_RENAMED | Status::WT_TYPECHANGE,
      ) {
        let code = if st.contains(Status::WT_RENAMED) {
          StatusCode::Renamed
        } else if st.contains(Status::WT_NEW) {
          StatusCode::Added
        } else if st.contains(Status::WT_DELETED) {
          StatusCode::Deleted
        } else {
          StatusCode::Modified
        };
        let (new_path, old_path) = delta_paths(entry.index_to_workdir(), &path);
        let (a, d) = unstaged_stats.get(&new_path).copied().unwrap_or((0, 0));
        let submodule = submodules.get(&new_path).cloned();
        unstaged.push(FileChange {
          path: new_path,
          old_path,
          status: code,
          additions: a,
          deletions: d,
          conflicted: false,
          submodule,
        });
      }
    }

    Ok(WorkingStatus { staged, unstaged })
  }
}

#[cfg(test)]
mod tests {
  use std::fs;

  use git2::{Repository, Signature};

  use super::*;

  fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().expect("index");
    index
      .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
      .expect("add");
    index.write().expect("write index");
    let tree = repo.find_tree(index.write_tree().expect("tree id")).expect("tree");
    let sig = Signature::now("Counts Test", "counts@example.com").expect("signature");
    let parents = repo
      .head()
      .ok()
      .and_then(|h| h.peel_to_commit().ok())
      .into_iter()
      .collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();
    repo
      .commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
      .expect("commit");
  }

  fn repo_with_commit() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    fs::write(dir.path().join("base.txt"), "base\n").expect("write");
    commit_all(&repo, "base");
    (dir, repo)
  }

  /// A clean tree is zero, and nothing about a repo with no upstream reads as
  /// pending sync work.
  #[test]
  fn a_clean_repo_counts_zero() {
    let (_dir, repo) = repo_with_commit();
    let counts = repo_counts(&repo).expect("counts");
    assert_eq!(
      counts,
      RepoCounts {
        ahead: 0,
        behind: 0,
        uncommitted: 0
      }
    );
  }

  /// The number has to match what the user can see in the changes list, so it
  /// counts staged work, unstaged work, and brand-new files alike.
  #[test]
  fn counts_staged_unstaged_and_untracked_work() {
    let (dir, repo) = repo_with_commit();

    // Staged: a new file added to the index.
    fs::write(dir.path().join("staged.txt"), "staged\n").expect("write");
    let mut index = repo.index().expect("index");
    index
      .add_path(std::path::Path::new("staged.txt"))
      .expect("stage");
    index.write().expect("write index");

    // Unstaged: an edit to a committed file.
    fs::write(dir.path().join("base.txt"), "changed\n").expect("write");
    // Untracked: never seen by git.
    fs::write(dir.path().join("new.txt"), "new\n").expect("write");

    let counts = repo_counts(&repo).expect("counts");
    assert_eq!(counts.uncommitted, 3, "three files are waiting to be committed");
  }

  /// A file both staged and edited again is one piece of work on the badge,
  /// even though `get_status` lists it under staged and unstaged both.
  #[test]
  fn a_file_changed_twice_counts_once() {
    let (dir, repo) = repo_with_commit();
    fs::write(dir.path().join("base.txt"), "staged\n").expect("write");
    let mut index = repo.index().expect("index");
    index
      .add_path(std::path::Path::new("base.txt"))
      .expect("stage");
    index.write().expect("write index");
    // Edit again after staging, so it is dirty in the index and the worktree.
    fs::write(dir.path().join("base.txt"), "and again\n").expect("write");

    let counts = repo_counts(&repo).expect("counts");
    assert_eq!(counts.uncommitted, 1, "one file, not two");
  }

  /// Untracked folders count without being walked: the files inside a new
  /// directory must still register as work waiting.
  #[test]
  fn an_untracked_folder_registers_as_work() {
    let (dir, repo) = repo_with_commit();
    fs::create_dir(dir.path().join("fresh")).expect("mkdir");
    fs::write(dir.path().join("fresh/a.txt"), "a\n").expect("write");
    fs::write(dir.path().join("fresh/b.txt"), "b\n").expect("write");

    let counts = repo_counts(&repo).expect("counts");
    assert!(
      counts.uncommitted >= 1,
      "a new folder must show as pending work, got {}",
      counts.uncommitted
    );
  }

  /// Ahead/behind comes from the checked-out branch's upstream.
  #[test]
  fn ahead_counts_commits_waiting_to_push() {
    let (dir, repo) = repo_with_commit();
    let head = repo.head().expect("head").peel_to_commit().expect("commit");
    // `set_upstream` resolves the remote behind the ref, so it has to exist.
    repo
      .remote("origin", "https://example.invalid/repo.git")
      .expect("remote");
    // Pretend the remote is at our first commit, then move ahead of it.
    repo
      .reference("refs/remotes/origin/master", head.id(), true, "test remote")
      .expect("remote ref");
    let mut branch = repo
      .find_branch("master", git2::BranchType::Local)
      .expect("branch");
    branch.set_upstream(Some("origin/master")).expect("upstream");

    fs::write(dir.path().join("next.txt"), "next\n").expect("write");
    commit_all(&repo, "next");

    let counts = repo_counts(&repo).expect("counts");
    assert_eq!(counts.ahead, 1, "one commit is waiting to push");
    assert_eq!(counts.behind, 0, "nothing is waiting to pull");
  }

  /// A detached HEAD has no upstream to compare against; that is zero, not an
  /// error, and it must not stop the file count from being reported.
  #[test]
  fn a_detached_head_reports_zero_sync() {
    let (dir, repo) = repo_with_commit();
    let head = repo.head().expect("head").peel_to_commit().expect("commit");
    repo.set_head_detached(head.id()).expect("detach");
    fs::write(dir.path().join("loose.txt"), "loose\n").expect("write");

    let counts = repo_counts(&repo).expect("counts");
    assert_eq!((counts.ahead, counts.behind), (0, 0), "no upstream to compare");
    assert_eq!(counts.uncommitted, 1, "file counting still works detached");
  }
}
