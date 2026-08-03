//! Worktree commands: list, add, remove, prune, repair.
//!
//! Every state-changing command releases GitWyrm's own hold on the target
//! folder first (see [`release_hold`]). Windows will not delete a directory
//! anything holds a handle in, and this app watches worktree paths and can have
//! one open as a tab -- the app being the reason its own removal fails is the
//! documented failure mode for tools in exactly this shape.

use std::path::{Path, PathBuf};

use tauri::State;

use crate::error::AppError;
use crate::git::worktree::{
  self, DirtyChoice, DirtyCount, IgnoredFile, RemoveOutcome, Worktree,
};
use crate::state::{repo_id_for, RepoManager};
use crate::watcher::WatcherRegistry;

/// Stop watching a folder and drop its repository handle, so nothing GitWyrm
/// owns is holding a file open in it.
///
/// Called before every removal. Nothing else can be fixed from inside the app --
/// an editor or dev server in the folder is the user's to close -- but the app
/// blocking its own delete is not something to leave to chance.
fn release_hold(manager: &RepoManager, watchers: &WatcherRegistry, path: &Path) {
  let id = repo_id_for(path);
  watchers.unwatch(&id);
  manager.close(&id);
}

/// Every checkout of this repository: the main one first, then linked
/// worktrees.
#[tauri::command]
#[specta::specta]
pub async fn list_worktrees(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<Worktree>, AppError> {
  let open = manager.get(&repo_id)?;
  let current = open.path.clone();
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    Ok(worktree::list(&repo, Some(&current)))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Modified and untracked counts for a worktree that is not the open one.
/// Powers the remove confirm and the run-discard hand-edit check.
#[tauri::command]
#[specta::specta]
pub async fn worktree_dirty_count(path: String) -> Result<DirtyCount, AppError> {
  tauri::async_runtime::spawn_blocking(move || worktree::dirty_count(Path::new(&path)))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// The suggested folder for a new worktree on `branch`: a sibling of the
/// repository, de-duplicated if it already exists.
///
/// Computed in Rust so the modal and any future caller agree on it rather than
/// each inventing their own idea of where worktrees go.
#[tauri::command]
#[specta::specta]
pub async fn suggest_worktree_path(
  manager: State<'_, RepoManager>,
  repo_id: String,
  branch: String,
) -> Result<String, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let main = worktree::main_workdir(&repo)
      .ok_or_else(|| AppError::Other("this project has no working folder".into()))?;
    Ok(worktree::suggest_path(&main, &branch))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Why the chosen folder will not work, in plain words. None when it is fine.
/// Checked as the user types, so a bad choice is a sentence in the dialog
/// rather than a half-created folder to clean up.
#[tauri::command]
#[specta::specta]
pub async fn check_worktree_path(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<Option<String>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let main = worktree::main_workdir(&repo)
      .ok_or_else(|| AppError::Other("this project has no working folder".into()))?;
    Ok(worktree::path_problem(&main, Path::new(path.trim())))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// The worktree holding `branch`, if any.
///
/// Returned as the whole worktree so the caller can offer to open that folder,
/// which is the only useful thing to do about it.
#[tauri::command]
#[specta::specta]
pub async fn branch_holder(
  manager: State<'_, RepoManager>,
  repo_id: String,
  branch: String,
) -> Result<Option<Worktree>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    Ok(worktree::holder_of_branch(&repo, &branch))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Ignored files the new worktree would not get, offered for copying.
///
/// A fresh worktree has only tracked files, so local env files and editor
/// settings are simply absent and the checkout looks broken for a reason git
/// never mentions. Dependency folders are excluded -- they are rebuilt.
#[tauri::command]
#[specta::specta]
pub async fn worktree_copyable_files(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<IgnoredFile>, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.clone();
  tauri::async_runtime::spawn_blocking(move || Ok(worktree::ignored_files_worth_copying(&path)))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Create a worktree.
///
/// Validates the folder and the branch before shelling out, so a refusal
/// happens before anything is written to disk. `copy_files` names ignored files
/// to carry across from the source checkout.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn add_worktree(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  branch: String,
  new_branch: bool,
  start_point: Option<String>,
  copy_files: Vec<String>,
) -> Result<String, AppError> {
  let open = manager.get(&repo_id)?;
  let source = open.path.clone();
  tauri::async_runtime::spawn_blocking(move || {
    let target = PathBuf::from(path.trim());
    let (main_path, holder) = {
      let repo = open.repo.lock().unwrap();
      let main = worktree::main_workdir(&repo)
        .ok_or_else(|| AppError::Other("this project has no working folder".into()))?;
      // Checked here as well as in the modal: validation that only lives in the
      // dialog is validation a second caller does not get.
      if let Some(problem) = worktree::path_problem(&main, &target) {
        return Err(AppError::Other(problem));
      }
      let holder = if new_branch {
        None
      } else {
        worktree::holder_of_branch(&repo, branch.trim())
      };
      (main, holder)
    };

    if let Some(holder) = holder {
      return Err(AppError::Other(format!(
        "{} is already open in the {} folder. A branch can only be checked out in one folder at a time.",
        branch.trim(),
        holder.folder_name
      )));
    }

    let main_str = main_path.to_string_lossy().into_owned();
    let target_str = target.to_string_lossy().into_owned();
    worktree::add(
      &main_str,
      &target_str,
      branch.trim(),
      new_branch,
      start_point.as_deref(),
    )?;

    if !copy_files.is_empty() {
      let copied = worktree::copy_ignored_files(&source, &target, &copy_files);
      log::info!("add_worktree: copied {copied} ignored file(s) into {target_str}");
    }

    Ok(target_str)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Remove a worktree.
///
/// `choice` decides what happens to uncommitted work; the UI only sends
/// anything but `refuse` after its own plain-language confirm. GitWyrm releases
/// its own hold on the folder first, and retries once after releasing, so the
/// app is never the reason a removal reports that the folder is in use.
#[tauri::command]
#[specta::specta]
pub async fn remove_worktree(
  manager: State<'_, RepoManager>,
  watchers: State<'_, WatcherRegistry>,
  repo_id: String,
  path: String,
  choice: DirtyChoice,
) -> Result<RemoveOutcome, AppError> {
  let open = manager.get(&repo_id)?;

  // Refuse the folder the caller is looking at before releasing anything --
  // removing the checkout out from under the open window is never the intent.
  if worktree::paths_equal(&open.path, Path::new(path.trim())) {
    return Err(AppError::Other(
      "That is the folder you have open. Switch to another tab first, then remove it.".into(),
    ));
  }

  release_hold(&manager, &watchers, Path::new(path.trim()));

  let repo_path = worktree::main_workdir(&open.repo.lock().unwrap())
    .map(|p| p.to_string_lossy().into_owned())
    .unwrap_or_else(|| open.path.to_string_lossy().into_owned());

  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let outcome = worktree::remove(&repo, &repo_path, path.trim(), choice)?;

    // A lock right after releasing our own hold may be a handle that was on its
    // way out. One retry costs nothing and turns a spurious failure into a
    // success; a real holder (an editor, a dev server) still reports the lock.
    if let RemoveOutcome::RefusedLocked { .. } = outcome {
      std::thread::sleep(std::time::Duration::from_millis(250));
      return worktree::remove(&repo, &repo_path, path.trim(), choice);
    }
    Ok(outcome)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Drop admin entries for worktrees whose folders are gone. Returns how many
/// were tidied up.
#[tauri::command]
#[specta::specta]
pub async fn prune_worktrees(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<u32, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    worktree::prune(&repo)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Point git at a worktree that moved. `new_path` is where the folder went;
/// omitting it repairs every worktree at once, which is the case where the main
/// repository itself was moved.
#[tauri::command]
#[specta::specta]
pub async fn repair_worktree(
  manager: State<'_, RepoManager>,
  repo_id: String,
  new_path: Option<String>,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let repo_path = worktree::main_workdir(&open.repo.lock().unwrap())
    .map(|p| p.to_string_lossy().into_owned())
    .unwrap_or_else(|| open.path.to_string_lossy().into_owned());
  tauri::async_runtime::spawn_blocking(move || {
    worktree::repair(&repo_path, new_path.as_deref())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
