use git2::{Diff, DiffOptions, Oid};
use serde::Deserialize;
use specta::Type;
use tauri::State;

use crate::error::AppError;
use crate::git::types::{CommitDetail, DiffLineEntry, FileChange, FileDiff, StatusCode};
use crate::state::RepoManager;

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DiffSource {
  Staged,
  Unstaged,
  Commit { sha: String },
}

fn delta_code(delta: git2::Delta) -> StatusCode {
  match delta {
    git2::Delta::Added | git2::Delta::Untracked => StatusCode::Added,
    git2::Delta::Deleted => StatusCode::Deleted,
    git2::Delta::Renamed => StatusCode::Renamed,
    git2::Delta::Conflicted => StatusCode::Conflicted,
    _ => StatusCode::Modified,
  }
}

fn build_file_diff(diff: &Diff, path: &str) -> Result<FileDiff, AppError> {
  use std::cell::RefCell;

  let lines: RefCell<Vec<DiffLineEntry>> = RefCell::new(Vec::new());
  let additions = RefCell::new(0u32);
  let deletions = RefCell::new(0u32);
  let binary = RefCell::new(false);

  let matches_path = |delta: &git2::DiffDelta| {
    delta
      .new_file()
      .path()
      .or_else(|| delta.old_file().path())
      .is_some_and(|p| p.to_string_lossy() == path)
  };

  diff
    .foreach(
      &mut |_, _| true,
      Some(&mut |delta, _| {
        if matches_path(&delta) {
          *binary.borrow_mut() = true;
        }
        true
      }),
      Some(&mut |delta, hunk| {
        if matches_path(&delta) {
          lines.borrow_mut().push(DiffLineEntry {
            sign: "@".into(),
            old_no: None,
            new_no: None,
            text: String::from_utf8_lossy(hunk.header()).trim_end().to_string(),
          });
        }
        true
      }),
      Some(&mut |delta, _hunk, line| {
        if !matches_path(&delta) {
          return true;
        }
        let origin = line.origin();
        if !matches!(origin, '+' | '-' | ' ') {
          return true;
        }
        if origin == '+' {
          *additions.borrow_mut() += 1;
        } else if origin == '-' {
          *deletions.borrow_mut() += 1;
        }
        lines.borrow_mut().push(DiffLineEntry {
          sign: if origin == ' ' { String::new() } else { origin.to_string() },
          old_no: line.old_lineno(),
          new_no: line.new_lineno(),
          text: String::from_utf8_lossy(line.content()).trim_end_matches('\n').to_string(),
        });
        true
      }),
    )
    .map_err(AppError::Git)?;

  Ok(FileDiff {
    path: path.to_string(),
    additions: additions.into_inner(),
    deletions: deletions.into_inner(),
    lines: lines.into_inner(),
    binary: binary.into_inner(),
  })
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_diff(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  source: DiffSource,
) -> Result<FileDiff, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut opts = DiffOptions::new();
    opts
      .pathspec(&path)
      .include_untracked(true)
      .show_untracked_content(true)
      .context_lines(3);

    let diff = match &source {
      DiffSource::Unstaged => repo.diff_index_to_workdir(None, Some(&mut opts))?,
      DiffSource::Staged => {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
      }
      DiffSource::Commit { sha } => {
        let oid = Oid::from_str(sha)?;
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
        repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?
      }
    };

    build_file_diff(&diff, &path)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn get_commit_detail(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
) -> Result<CommitDetail, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let oid = Oid::from_str(&sha)?;
    let commit = repo.find_commit(oid)?;
    let tree = commit.tree()?;
    let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

    let mut opts = DiffOptions::new();
    let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;

    // Per-file stats.
    let files: std::cell::RefCell<Vec<FileChange>> = std::cell::RefCell::new(Vec::new());
    diff.foreach(
      &mut |delta, _| {
        if let Some(p) = delta.new_file().path().or_else(|| delta.old_file().path()) {
          files.borrow_mut().push(FileChange {
            path: p.to_string_lossy().into_owned(),
            status: delta_code(delta.status()),
            additions: 0,
            deletions: 0,
            conflicted: false,
          });
        }
        true
      },
      None,
      None,
      Some(&mut |delta, _hunk, line| {
        if let Some(p) = delta.new_file().path().or_else(|| delta.old_file().path()) {
          let path = p.to_string_lossy();
          if let Some(f) = files.borrow_mut().iter_mut().find(|f| f.path == path) {
            match line.origin() {
              '+' => f.additions += 1,
              '-' => f.deletions += 1,
              _ => {}
            }
          }
        }
        true
      }),
    )?;
    let files = files.into_inner();

    let author = commit.author();
    Ok(CommitDetail {
      sha: oid.to_string(),
      summary: commit.summary().unwrap_or("").to_string(),
      body: commit.body().unwrap_or("").to_string(),
      author_name: author.name().unwrap_or("unknown").to_string(),
      author_email: author.email().unwrap_or("").to_string(),
      time: commit.time().seconds() as f64,
      parent_shas: commit.parent_ids().map(|p| p.to_string()).collect(),
      files,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
