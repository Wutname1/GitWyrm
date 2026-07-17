use git2::BranchType;
use tauri::State;

use crate::error::AppError;
use crate::git::types::{BranchInfo, BranchList, TagInfo};
use crate::state::RepoManager;

#[tauri::command]
#[specta::specta]
pub async fn list_branches(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<BranchList, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut local = Vec::new();
    let mut remote = Vec::new();

    for (branch, _) in repo.branches(Some(BranchType::Local))?.flatten() {
      let Some(name) = branch.name().ok().flatten().map(str::to_string) else { continue };
      let is_head = branch.is_head();
      let upstream = branch
        .upstream()
        .ok()
        .and_then(|u| u.name().ok().flatten().map(str::to_string));
      let (ahead, behind) = match (&upstream, branch.get().target()) {
        (Some(up), Some(local_oid)) => {
          let up_oid = repo
            .find_branch(up, BranchType::Remote)
            .ok()
            .and_then(|b| b.get().target());
          match up_oid {
            Some(up_oid) => repo
              .graph_ahead_behind(local_oid, up_oid)
              .map(|(a, b)| (a as u32, b as u32))
              .unwrap_or((0, 0)),
            None => (0, 0),
          }
        }
        _ => (0, 0),
      };
      local.push(BranchInfo { name, is_head, upstream, ahead, behind });
    }

    for (branch, _) in repo.branches(Some(BranchType::Remote))?.flatten() {
      if let Some(name) = branch.name().ok().flatten() {
        if !name.ends_with("/HEAD") {
          remote.push(name.to_string());
        }
      }
    }

    local.sort_by(|a, b| b.is_head.cmp(&a.is_head).then(a.name.cmp(&b.name)));
    remote.sort();
    Ok(BranchList { local, remote })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn checkout_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    // Refuse checkout over local modifications: safer default for v1.
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false);
    let dirty = repo
      .statuses(Some(&mut opts))?
      .iter()
      .any(|e| !e.status().is_ignored());
    if dirty {
      return Err(AppError::Other(
        "working tree has changes; commit or stash before switching branches".into(),
      ));
    }

    let (object, reference) = repo.revparse_ext(&name)?;
    repo.checkout_tree(&object, None)?;
    match reference {
      Some(r) => repo.set_head(r.name().unwrap_or("HEAD"))?,
      None => repo.set_head_detached(object.id())?,
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn create_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  checkout: bool,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let head = repo
      .head()?
      .peel_to_commit()
      .map_err(|_| AppError::Other("repository has no commits yet".into()))?;
    repo.branch(&name, &head, false)?;
    if checkout {
      let refname = format!("refs/heads/{name}");
      let object = repo.revparse_single(&refname)?;
      repo.checkout_tree(&object, None)?;
      repo.set_head(&refname)?;
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn list_tags(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<TagInfo>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let names = repo.tag_names(None)?;
    let mut tags: Vec<TagInfo> = names
      .iter()
      .flatten()
      .map(|n| TagInfo { name: n.to_string() })
      .collect();
    tags.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(tags)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
