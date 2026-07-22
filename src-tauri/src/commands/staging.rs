use std::path::Path;

use tauri::State;

use crate::error::AppError;
use crate::git::submodule::is_submodule;
use crate::state::RepoManager;

fn stage_paths(repo: &git2::Repository, root: &Path, paths: &[String]) -> Result<(), git2::Error> {
  let mut index = repo.index()?;
  for path in paths {
    let rel = Path::new(path);
    if root.join(rel).exists() {
      index.add_path(rel)?;
    } else {
      index.remove_path(rel)?;
    }
  }
  index.write()?;
  Ok(())
}

fn unstage_paths(repo: &git2::Repository, paths: &[String]) -> Result<(), git2::Error> {
  let head = repo.head().ok().and_then(|h| h.peel(git2::ObjectType::Commit).ok());
  match head {
    Some(head) => repo.reset_default(Some(&head), paths.iter().map(String::as_str))?,
    None => {
      let mut index = repo.index()?;
      for path in paths {
        index.remove_path(Path::new(path))?;
      }
      index.write()?;
    }
  }
  Ok(())
}

fn discard_paths(repo: &git2::Repository, paths: &[String]) -> Result<(), git2::Error> {
  let head = repo.head()?.peel(git2::ObjectType::Commit)?;

  // Reset the selected index entries too, so "all changes in this folder"
  // includes files that are already staged.
  repo.reset_default(Some(&head), paths.iter().map(String::as_str))?;

  let mut builder = git2::build::CheckoutBuilder::new();
  builder.force().remove_untracked(true);
  let mut has_regular_file = false;
  for path in paths {
    if is_submodule(repo, path) {
      let mut sub = repo.find_submodule(path)?;
      sub.update(false, None)?;
    } else {
      builder.path(path);
      has_regular_file = true;
    }
  }
  if has_regular_file {
    repo.checkout_head(Some(&mut builder))?;
  }
  Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn stage_file(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    stage_paths(&repo, &open.path, &[path]).map_err(AppError::Git)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Stage an exact set of files as one visible folder action.
#[tauri::command]
#[specta::specta]
pub async fn stage_files(
  manager: State<'_, RepoManager>,
  repo_id: String,
  paths: Vec<String>,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    stage_paths(&repo, &open.path, &paths).map_err(AppError::Git)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn unstage_file(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    unstage_paths(&repo, &[path]).map_err(AppError::Git)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Unstage an exact set of files as one visible folder action.
#[tauri::command]
#[specta::specta]
pub async fn unstage_files(
  manager: State<'_, RepoManager>,
  repo_id: String,
  paths: Vec<String>,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    unstage_paths(&repo, &paths).map_err(AppError::Git)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn stage_all(manager: State<'_, RepoManager>, repo_id: String) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut index = repo.index()?;
    index.add_all(["*"], git2::IndexAddOption::DEFAULT, None)?;
    index.update_all(["*"], None)?;
    index.write()?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn unstage_all(manager: State<'_, RepoManager>, repo_id: String) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    if let Some(head) = repo.head().ok().and_then(|h| h.peel(git2::ObjectType::Commit).ok()) {
      repo.reset_default(Some(&head), ["*"])?;
    } else {
      let mut index = repo.index()?;
      index.clear()?;
      index.write()?;
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn discard_file(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    // A submodule pointer can't be reset by checking out the parent's tree --
    // that leaves the nested checkout untouched and the "change" persists. Snap
    // the submodule back to its recorded commit instead, which is what discard
    // means for it.
    if is_submodule(&repo, &path) {
      let mut sub = repo.find_submodule(&path)?;
      sub.update(false, None)?;
      return Ok(());
    }
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.path(&path).force().remove_untracked(true);
    repo.checkout_head(Some(&mut builder))?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Discard staged and unstaged changes for an exact set of files. The caller
/// confirms this destructive action before invoking it.
#[tauri::command]
#[specta::specta]
pub async fn discard_files(
  manager: State<'_, RepoManager>,
  repo_id: String,
  paths: Vec<String>,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    discard_paths(&repo, &paths).map_err(AppError::Git)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Discard every uncommitted change: unstage the index, restore all tracked
/// files to HEAD, and remove untracked files. Irreversible; the caller confirms.
#[tauri::command]
#[specta::specta]
pub async fn discard_all(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    // Reset the index to HEAD so staged changes are dropped too.
    let head = repo.head()?.peel(git2::ObjectType::Commit)?;
    repo.reset(&head, git2::ResetType::Mixed, None)?;
    // Force the working tree back to HEAD and delete untracked files.
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.force().remove_untracked(true);
    repo.checkout_head(Some(&mut builder))?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  fn committed_repo() -> (tempfile::TempDir, git2::Repository) {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = git2::Repository::init(dir.path()).expect("init repo");
    fs::create_dir_all(dir.path().join("target/nested")).expect("create target folder");
    fs::write(dir.path().join("target/nested/tracked.txt"), "original\n").expect("write target file");
    fs::write(dir.path().join("outside.txt"), "outside\n").expect("write outside file");

    let mut index = repo.index().expect("index");
    index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).expect("stage fixture");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    {
      let tree = repo.find_tree(tree_id).expect("find tree");
      let signature = git2::Signature::now("Test Wyrm", "test@gitwyrm.dev").expect("signature");
      repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .expect("initial commit");
    }
    (dir, repo)
  }

  #[test]
  fn folder_batch_actions_stay_scoped_and_discard_staged_and_unstaged_work() {
    let (dir, repo) = committed_repo();
    let tracked = "target/nested/tracked.txt".to_string();
    let added = "target/nested/added.txt".to_string();
    let paths = vec![tracked.clone(), added.clone()];

    fs::write(dir.path().join(&tracked), "staged version\n").expect("edit tracked file");
    fs::write(dir.path().join(&added), "new file\n").expect("create added file");
    stage_paths(&repo, dir.path(), &paths).expect("stage folder files");

    unstage_paths(&repo, &paths).expect("unstage folder files");
    let tracked_status = repo.status_file(Path::new(&tracked)).expect("tracked status");
    assert!(tracked_status.contains(git2::Status::WT_MODIFIED));
    assert!(!tracked_status.intersects(git2::Status::INDEX_MODIFIED | git2::Status::INDEX_NEW));

    stage_paths(&repo, dir.path(), &paths).expect("restage folder files");
    fs::write(dir.path().join(&tracked), "unstaged version after staging\n")
      .expect("add unstaged edit");
    fs::write(dir.path().join("outside.txt"), "keep this edit\n").expect("edit outside file");

    discard_paths(&repo, &paths).expect("discard folder files");

    assert_eq!(fs::read_to_string(dir.path().join(&tracked)).expect("read target").trim_end(), "original");
    assert!(!dir.path().join(&added).exists());
    assert_eq!(
      fs::read_to_string(dir.path().join("outside.txt")).expect("read outside").trim_end(),
      "keep this edit"
    );
    let statuses = repo.statuses(None).expect("read statuses");
    assert!(statuses.iter().all(|entry| {
      !entry.path().is_some_and(|path| path.starts_with("target/"))
    }));
    assert!(repo.status_file(Path::new("outside.txt")).expect("outside dirty").contains(git2::Status::WT_MODIFIED));
  }
}
