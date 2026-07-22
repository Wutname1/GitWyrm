//! Per-file commands driven by the right-click menu on a changed file:
//! opening one file in an editor or the file manager, deleting or restoring
//! it, and the two history views (a file's commit list, and line-by-line
//! blame).

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use git2::{BlameOptions, DiffOptions, Oid, Sort};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::AppError;
use crate::state::RepoManager;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// One commit that touched a given file, plus how the file changed in it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FileHistoryEntry {
  pub sha: String,
  pub short_sha: String,
  pub summary: String,
  pub author_name: String,
  pub author_email: String,
  pub time: f64,
  pub additions: u32,
  pub deletions: u32,
  /// Path this file had in this commit, when the commit renamed it.
  pub old_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct FileHistory {
  pub path: String,
  pub entries: Vec<FileHistoryEntry>,
  /// True when the walk stopped at `limit` and older commits remain.
  pub has_more: bool,
}

/// One line of a file, tagged with the commit that last changed it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct BlameLine {
  pub line_no: u32,
  pub text: String,
  pub sha: String,
  pub short_sha: String,
  pub summary: String,
  pub author_name: String,
  pub author_email: String,
  pub time: f64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct FileBlame {
  pub path: String,
  pub lines: Vec<BlameLine>,
  /// Set instead of `lines` when the file can't be blamed line-by-line.
  pub binary: bool,
}

/// Resolve a repo-relative path against the working directory, refusing
/// anything that escapes the repo. The path comes from the UI, but a crafted
/// `..` segment must never let a delete land outside the project.
fn resolve_in_repo(workdir: &Path, rel: &str) -> Result<PathBuf, AppError> {
  let rel_path = Path::new(rel);
  // `is_absolute` is not enough on Windows: a leading "/" has no drive prefix
  // there, so it reads as relative while `join` still resolves it from the
  // drive root. Reject any RootDir component explicitly.
  if rel_path.is_absolute()
    || rel_path.components().any(|c| {
      matches!(
        c,
        Component::ParentDir | Component::Prefix(_) | Component::RootDir
      )
    })
  {
    return Err(AppError::Other(format!("Unsafe file path: {rel}")));
  }
  Ok(workdir.join(rel_path))
}

/// Working directory of an open repo.
fn workdir(manager: &RepoManager, repo_id: &str) -> Result<PathBuf, AppError> {
  let open = manager.get(repo_id)?;
  Ok(open.path.clone())
}

/// Open a single file in VS Code. Mirrors `external::open_in_editor`, but
/// targets one file inside the repo rather than the repo folder.
#[tauri::command]
#[specta::specta]
pub fn open_file_in_editor(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let full = resolve_in_repo(&workdir(&manager, &repo_id)?, &path)?;
  let full = full.to_string_lossy().into_owned();

  #[cfg(windows)]
  let mut cmd = {
    let mut c = Command::new("cmd");
    c.args(["/C", "code", &full]);
    use std::os::windows::process::CommandExt;
    c.creation_flags(CREATE_NO_WINDOW);
    c
  };
  #[cfg(not(windows))]
  let mut cmd = {
    let mut c = Command::new("code");
    c.arg(&full);
    c
  };

  cmd.spawn().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other(
        "Could not find VS Code. Install it, then run \"Shell Command: Install 'code' command in PATH\" from VS Code.".into(),
      )
    } else {
      AppError::Io(e)
    }
  })?;
  Ok(())
}

/// Show a single file in the OS file manager, selected in its folder.
#[tauri::command]
#[specta::specta]
pub fn reveal_file_in_file_manager(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let full = resolve_in_repo(&workdir(&manager, &repo_id)?, &path)?;
  app
    .opener()
    .reveal_item_in_dir(&full)
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Send a file to the OS Recycle Bin / Trash. Recoverable on purpose: the
/// menu entry sits next to "Discard changes", and an unrecoverable delete a
/// click away from a routine action is too sharp an edge.
#[tauri::command]
#[specta::specta]
pub fn delete_file(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let full = resolve_in_repo(&workdir(&manager, &repo_id)?, &path)?;
  if !full.exists() {
    return Err(AppError::Other(format!("{path} is already gone")));
  }
  trash::delete(&full).map_err(|e| AppError::Other(e.to_string()))
}

/// Put a file back to its committed contents, undoing edits and un-deleting it
/// if it was removed. Restores from HEAD, so a file that was never committed
/// cannot be restored this way.
#[tauri::command]
#[specta::specta]
pub async fn restore_file(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let head = repo
      .head()
      .and_then(|h| h.peel_to_tree())
      .map_err(|_| AppError::Other(format!("{path} has never been committed, so there is nothing to restore it to")))?;
    // Confirm the file exists in HEAD before checkout: a pathspec that matches
    // nothing makes `checkout_tree` a silent no-op, which would look like the
    // restore worked.
    head
      .get_path(Path::new(&path))
      .map_err(|_| AppError::Other(format!("{path} is not in the last commit, so there is nothing to restore it to")))?;

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().path(&path);
    repo
      .checkout_tree(head.as_object(), Some(&mut checkout))
      .map_err(AppError::Git)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Commits that touched one file, newest first. Follows the file backwards
/// through renames so a rename doesn't truncate its story.
#[tauri::command]
#[specta::specta]
pub async fn get_file_history(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  limit: u32,
) -> Result<FileHistory, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    walk.push_head()?;

    let mut entries = Vec::new();
    let mut has_more = false;
    // The path we are tracking, which moves backwards through renames.
    let mut tracked = path.clone();

    for oid in walk.flatten() {
      if entries.len() >= limit as usize {
        has_more = true;
        break;
      }
      let commit = repo.find_commit(oid)?;
      let tree = commit.tree()?;
      let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

      let mut opts = DiffOptions::new();
      opts.pathspec(&tracked);
      let mut diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;
      let mut find = git2::DiffFindOptions::new();
      find.renames(true);
      diff.find_similar(Some(&mut find))?;

      if diff.deltas().len() == 0 {
        continue;
      }

      let stats = diff.stats()?;
      let old_path = diff.deltas().find_map(|d| {
        if d.status() != git2::Delta::Renamed {
          return None;
        }
        d.old_file()
          .path()
          .map(|p| p.to_string_lossy().into_owned())
      });

      let author = commit.author();
      entries.push(FileHistoryEntry {
        sha: oid.to_string(),
        short_sha: oid.to_string()[..7].to_string(),
        summary: commit.summary().unwrap_or("").to_string(),
        author_name: author.name().unwrap_or("unknown").to_string(),
        author_email: author.email().unwrap_or("").to_string(),
        time: commit.time().seconds() as f64,
        additions: stats.insertions().min(u32::MAX as usize) as u32,
        deletions: stats.deletions().min(u32::MAX as usize) as u32,
        old_path: old_path.clone(),
      });

      // Keep following the file under its previous name.
      if let Some(old) = old_path {
        tracked = old;
      }
    }

    Ok(FileHistory {
      path,
      entries,
      has_more,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Line-by-line authorship for a file. `sha` blames the file as of that commit;
/// omit it to blame the working copy against HEAD.
#[tauri::command]
#[specta::specta]
pub async fn get_file_blame(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  sha: Option<String>,
) -> Result<FileBlame, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let file_path = Path::new(&path);

    let mut opts = BlameOptions::new();
    if let Some(sha) = &sha {
      opts.newest_commit(Oid::from_str(sha)?);
    }
    let blame = repo.blame_file(file_path, Some(&mut opts))?;

    // Blame reports commits per line range; the text comes from the blob at
    // that revision (or the working copy when blaming HEAD).
    let contents: Vec<u8> = match &sha {
      Some(sha) => {
        let commit = repo.find_commit(Oid::from_str(sha)?)?;
        let entry = commit.tree()?.get_path(file_path)?;
        repo.find_blob(entry.id())?.content().to_vec()
      }
      None => {
        let workdir = repo
          .workdir()
          .ok_or_else(|| AppError::Other("This repository has no working folder".into()))?;
        std::fs::read(workdir.join(file_path)).map_err(AppError::Io)?
      }
    };

    if contents.contains(&0) {
      return Ok(FileBlame {
        path,
        lines: Vec::new(),
        binary: true,
      });
    }

    let text = String::from_utf8_lossy(&contents);
    let mut lines = Vec::new();
    for (i, line) in text.lines().enumerate() {
      let line_no = (i + 1) as u32;
      let Some(hunk) = blame.get_line(line_no as usize) else {
        continue;
      };
      let oid = hunk.final_commit_id();
      // Boundary hunks can point at a commit outside the walk; fall back to
      // showing the sha alone rather than failing the whole blame.
      let (summary, author_name, author_email, time) = match repo.find_commit(oid) {
        Ok(commit) => {
          let author = commit.author();
          (
            commit.summary().unwrap_or("").to_string(),
            author.name().unwrap_or("unknown").to_string(),
            author.email().unwrap_or("").to_string(),
            commit.time().seconds() as f64,
          )
        }
        Err(_) => (String::new(), "unknown".into(), String::new(), 0.0),
      };
      lines.push(BlameLine {
        line_no,
        text: line.to_string(),
        sha: oid.to_string(),
        short_sha: oid.to_string()[..7].to_string(),
        summary,
        author_name,
        author_email,
        time,
      });
    }

    Ok(FileBlame {
      path,
      lines,
      binary: false,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_paths_that_escape_the_repo() {
    let root = Path::new("/repo");
    assert!(resolve_in_repo(root, "../secrets.txt").is_err());
    assert!(resolve_in_repo(root, "src/../../etc/passwd").is_err());
    assert!(resolve_in_repo(root, "/etc/passwd").is_err());
    assert!(resolve_in_repo(root, "src/main.rs").is_ok());
  }
}
