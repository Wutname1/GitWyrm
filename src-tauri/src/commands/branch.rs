use git2::{build::CheckoutBuilder, BranchType, Oid, ResetType};
use tauri::State;

use crate::error::AppError;
use crate::git::types::{BranchInfo, BranchList, CheckoutOutcome, RefMove, ResetMode, TagInfo};
use crate::settings::BranchSwitchMode;
use crate::state::RepoManager;

/// Name of the branch HEAD points at, or an error if HEAD is detached.
fn current_branch_name(repo: &git2::Repository) -> Result<String, AppError> {
  let head = repo.head()?;
  if !head.is_branch() {
    return Err(AppError::Other("HEAD is detached; check out a branch first".into()));
  }
  head
    .shorthand()
    .map(str::to_string)
    .ok_or_else(|| AppError::Other("could not read current branch name".into()))
}

/// True when the working tree has tracked modifications (ignores untracked).
fn tree_dirty(repo: &git2::Repository) -> Result<bool, AppError> {
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(false);
  Ok(repo.statuses(Some(&mut opts))?.iter().any(|e| !e.status().is_ignored()))
}

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

/// Is the working tree dirty (any non-ignored change, including untracked)?
fn is_dirty(repo: &git2::Repository) -> Result<bool, AppError> {
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(true).recurse_untracked_dirs(true);
  Ok(repo.statuses(Some(&mut opts))?.iter().any(|e| !e.status().is_ignored()))
}

/// Move HEAD to `name` and update the working tree to its content. Uses a SAFE
/// checkout, so git refuses to clobber conflicting local changes.
fn switch_to(repo: &git2::Repository, name: &str) -> Result<(), AppError> {
  let (object, reference) = repo.revparse_ext(name)?;
  let mut builder = git2::build::CheckoutBuilder::new();
  builder.safe();
  repo.checkout_tree(&object, Some(&mut builder))?;
  match reference {
    Some(r) => repo.set_head(r.name().unwrap_or("HEAD"))?,
    None => repo.set_head_detached(object.id())?,
  }
  Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn checkout_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  mode: BranchSwitchMode,
) -> Result<CheckoutOutcome, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();

    if !is_dirty(&repo)? {
      switch_to(&repo, &name)?;
      return Ok(CheckoutOutcome::Clean);
    }

    match mode {
      BranchSwitchMode::Refuse => Err(AppError::Other(
        "working tree has changes; commit or stash before switching branches".into(),
      )),

      // Plain `git checkout`: carry changes across. A safe checkout errors if a
      // change would be overwritten, so surface a clear message in that case.
      BranchSwitchMode::Carry => {
        switch_to(&repo, &name).map_err(|_| {
          AppError::Other(
            "your local changes conflict with that branch; commit, stash, or discard them first"
              .into(),
          )
        })?;
        Ok(CheckoutOutcome::Clean)
      }

      // Stash, switch, then bring changes back.
      //
      // We deliberately use stash_APPLY (not pop) and drop the stash ourselves
      // only on a clean apply. git2's stash_pop returns Ok even when the apply
      // conflicts AND drops the stash regardless -- so a naive pop would destroy
      // the user's backup exactly when they need it. Applying and conditionally
      // dropping keeps the stash as a backup whenever conflicts remain.
      BranchSwitchMode::AutoStash => {
        let signature = repo.signature()?;
        repo.stash_save(
          &signature,
          &format!("gitwyrm: auto-stash before switching to {name}"),
          Some(git2::StashFlags::INCLUDE_UNTRACKED),
        )?;

        // If the switch itself fails, restore the stash so nothing is lost.
        if let Err(e) = switch_to(&repo, &name) {
          let _ = repo.stash_pop(0, None);
          return Err(e);
        }

        repo.stash_apply(0, None)?;

        if repo.index()?.has_conflicts() {
          // Leave stash@{0} in place as a backup; the working tree has markers.
          Ok(CheckoutOutcome::StashPopConflict)
        } else {
          // Clean apply: drop the now-redundant stash entry.
          repo.stash_drop(0)?;
          Ok(CheckoutOutcome::Stashed)
        }
      }
    }
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
