use git2::{Diff, DiffOptions, Oid};
use serde::Deserialize;
use specta::Type;
use tauri::State;

use crate::error::AppError;
use crate::git::types::{
    CommitDetail, DiffLineEntry, FileChange, FileDiff, HunkHeader, StatusCode,
};
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
    let hunks: RefCell<Vec<HunkHeader>> = RefCell::new(Vec::new());
    let additions = RefCell::new(0u32);
    let deletions = RefCell::new(0u32);
    let binary = RefCell::new(false);
    let old_path: RefCell<Option<String>> = RefCell::new(None);

    let matches_path = |delta: &git2::DiffDelta| {
        delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .is_some_and(|p| p.to_string_lossy() == path)
    };

    diff.foreach(
        &mut |delta, _| {
            if matches_path(&delta) && delta.status() == git2::Delta::Renamed {
                if let Some(old) = delta.old_file().path() {
                    *old_path.borrow_mut() = Some(old.to_string_lossy().into_owned());
                }
            }
            true
        },
        Some(&mut |delta, _| {
            if matches_path(&delta) {
                *binary.borrow_mut() = true;
            }
            true
        }),
        Some(&mut |delta, hunk| {
            if matches_path(&delta) {
                // Push the boundary first so its index equals the new hunk's index.
                let idx = hunks.borrow().len() as u32;
                hunks.borrow_mut().push(HunkHeader {
                    old_start: hunk.old_start(),
                    old_lines: hunk.old_lines(),
                    new_start: hunk.new_start(),
                    new_lines: hunk.new_lines(),
                    header: String::from_utf8_lossy(hunk.header())
                        .trim_end()
                        .to_string(),
                });
                lines.borrow_mut().push(DiffLineEntry {
                    sign: "@".into(),
                    old_no: None,
                    new_no: None,
                    text: String::from_utf8_lossy(hunk.header())
                        .trim_end()
                        .to_string(),
                    hunk_index: idx,
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
            // Lines always follow their hunk boundary, so the current last hunk owns them.
            let idx = hunks.borrow().len().saturating_sub(1) as u32;
            lines.borrow_mut().push(DiffLineEntry {
                sign: if origin == ' ' {
                    String::new()
                } else {
                    origin.to_string()
                },
                old_no: line.old_lineno(),
                new_no: line.new_lineno(),
                text: String::from_utf8_lossy(line.content())
                    .trim_end_matches('\n')
                    .to_string(),
                hunk_index: idx,
            });
            true
        }),
    )
    .map_err(AppError::Git)?;

    Ok(FileDiff {
        path: path.to_string(),
        old_path: old_path.into_inner(),
        additions: additions.into_inner(),
        deletions: deletions.into_inner(),
        hunks: hunks.into_inner(),
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
        let read = || -> Result<FileDiff, AppError> {
            let mut opts = DiffOptions::new();
            opts.pathspec(&path)
                .include_untracked(true)
                .recurse_untracked_dirs(true)
                .show_untracked_content(true)
                .context_lines(3);

            let mut diff = match &source {
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

            // Detect renames so `old_path` can be populated for renamed files.
            crate::git::rename_detect::find_renames(&mut diff)?;

            build_file_diff(&diff, &path)
        };

        // Only a working-tree diff reads files something else may be writing.
        let mut attempt = 1;
        loop {
            match read() {
                Err(e)
                    if matches!(source, DiffSource::Unstaged)
                        && attempt < FILE_CHANGED_ATTEMPTS
                        && is_file_changed_race(&e) =>
                {
                    log::info!("get_file_diff: {path} changed while being read; reading it again");
                    attempt += 1;
                    std::thread::sleep(FILE_CHANGED_RETRY_DELAY);
                }
                other => return other,
            }
        }
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// How many times to read a working-tree diff whose file keeps changing.
const FILE_CHANGED_ATTEMPTS: u32 = 3;
const FILE_CHANGED_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(150);

/// libgit2 notes a working file's size while building the diff, then refuses
/// to read its content if the size changed in between: "file changed before we
/// could read it" (class=Filesystem, diff_file.c). It means another program -
/// a build, a log writer, an editor saving - was writing the file at that
/// moment. A fresh diff a moment later almost always succeeds, so it is retried
/// rather than shown as a failure (GITWYRM-BACKEND-D).
fn is_file_changed_race(e: &AppError) -> bool {
    matches!(
        e,
        AppError::Git(g)
            if g.class() == git2::ErrorClass::Filesystem
                && g.message().contains("file changed before we could read it")
    )
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
        let mut diff =
            repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;

        crate::git::rename_detect::find_renames(&mut diff)?;

        // Per-file stats.
        let files: std::cell::RefCell<Vec<FileChange>> = std::cell::RefCell::new(Vec::new());
        diff.foreach(
            &mut |delta, _| {
                if let Some(p) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                    let path = p.to_string_lossy().into_owned();
                    let old_path = delta
                        .old_file()
                        .path()
                        .map(|o| o.to_string_lossy().into_owned())
                        .filter(|old| *old != path);
                    files.borrow_mut().push(FileChange {
                        path,
                        old_path,
                        status: delta_code(delta.status()),
                        additions: 0,
                        deletions: 0,
                        conflicted: false,
                        submodule: None,
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
            summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
            body: commit.body().ok().flatten().unwrap_or("").to_string(),
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

#[cfg(test)]
mod file_changed_tests {
    use super::is_file_changed_race;
    use crate::error::AppError;

    #[test]
    fn only_the_read_race_is_retried() {
        let race = AppError::Git(git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Filesystem,
            "file changed before we could read it",
        ));
        assert!(is_file_changed_race(&race));

        // A real filesystem fault must fail straight away, not three times.
        let denied = AppError::Git(git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Filesystem,
            "failed to open file: permission denied",
        ));
        assert!(!is_file_changed_race(&denied));
        assert!(!is_file_changed_race(&AppError::Other(
            "file changed before we could read it".into()
        )));
    }
}
