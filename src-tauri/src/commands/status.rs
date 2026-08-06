use std::collections::HashMap;

use git2::{DiffOptions, Status, StatusOptions};
use tauri::State;

use crate::error::AppError;
use crate::git::submodule::moved_submodules;
use crate::git::types::{FileChange, StatusCode, WorkingStatus};
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
