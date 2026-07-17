use std::collections::HashMap;

use git2::{Delta, DiffOptions, Status, StatusOptions};
use tauri::State;

use crate::error::AppError;
use crate::git::types::{FileChange, StatusCode, WorkingStatus};
use crate::state::RepoManager;

fn code_for(delta: Delta) -> StatusCode {
  match delta {
    Delta::Added | Delta::Untracked => StatusCode::Added,
    Delta::Deleted => StatusCode::Deleted,
    Delta::Renamed => StatusCode::Renamed,
    Delta::Conflicted => StatusCode::Conflicted,
    _ => StatusCode::Modified,
  }
}

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

#[tauri::command]
#[specta::specta]
pub async fn get_status(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<WorkingStatus, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let mut opts = StatusOptions::new();
    opts
      .include_untracked(true)
      .recurse_untracked_dirs(true)
      .renames_head_to_index(true)
      .update_index(true);
    let statuses = repo.statuses(Some(&mut opts))?;

    // Line counts: staged = HEAD tree -> index; unstaged = index -> workdir.
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let mut diff_opts = DiffOptions::new();
    diff_opts.include_untracked(true).show_untracked_content(true);
    let staged_stats = repo
      .diff_tree_to_index(head_tree.as_ref(), None, None)
      .map(|d| line_stats(&d))
      .unwrap_or_default();
    let unstaged_stats = repo
      .diff_index_to_workdir(None, Some(&mut diff_opts))
      .map(|d| line_stats(&d))
      .unwrap_or_default();

    let mut staged = Vec::new();
    let mut unstaged = Vec::new();

    for entry in statuses.iter() {
      let path = entry.path().unwrap_or("").to_string();
      let st = entry.status();

      if st.is_conflicted() {
        unstaged.push(FileChange {
          path: path.clone(),
          status: StatusCode::Conflicted,
          additions: 0,
          deletions: 0,
          conflicted: true,
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
        let (a, d) = staged_stats.get(&path).copied().unwrap_or((0, 0));
        staged.push(FileChange { path: path.clone(), status: code, additions: a, deletions: d, conflicted: false });
      }

      if st.intersects(
        Status::WT_NEW | Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_RENAMED | Status::WT_TYPECHANGE,
      ) {
        let code = if st.contains(Status::WT_NEW) {
          StatusCode::Added
        } else if st.contains(Status::WT_DELETED) {
          StatusCode::Deleted
        } else {
          StatusCode::Modified
        };
        let (a, d) = unstaged_stats.get(&path).copied().unwrap_or((0, 0));
        unstaged.push(FileChange { path, status: code, additions: a, deletions: d, conflicted: false });
      }
    }

    Ok(WorkingStatus { staged, unstaged })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
