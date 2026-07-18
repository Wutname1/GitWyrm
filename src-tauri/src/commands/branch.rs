use git2::{build::CheckoutBuilder, BranchType, Oid, ResetType};
use tauri::State;

use crate::error::AppError;
use crate::git::types::{
  BranchInfo, BranchList, BranchRelation, CheckoutOutcome, RefMove, ResetMode, TagInfo,
};
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

/// Ahead/behind counts between two arbitrary refs (branch, remote branch, tag,
/// or sha). `ahead` = commits `ours` has that `theirs` doesn't; `behind` = the
/// reverse. Backs the drag-to-sync analysis for non-tracking branch pairs.
#[tauri::command]
#[specta::specta]
pub async fn branch_relation(
  manager: State<'_, RepoManager>,
  repo_id: String,
  ours: String,
  theirs: String,
) -> Result<BranchRelation, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let our_oid = repo.revparse_single(&ours)?.peel_to_commit()?.id();
    let their_oid = repo.revparse_single(&theirs)?.peel_to_commit()?.id();
    let (ahead, behind) = repo.graph_ahead_behind(our_oid, their_oid)?;
    Ok(BranchRelation { ahead: ahead as u32, behind: behind as u32 })
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

/// Guidance shown when a branch switch is blocked by an un-stashable submodule
/// move that also collides with the target branch.
const SUBMODULE_SWITCH_HINT: &str = "a submodule points to a different commit than this branch expects. Commit the submodule change or reset the submodule to its recorded commit, then switch.";

/// Resolve the branch a switch should actually land on.
///
/// Checking out a remote-tracking ref like `origin/feature` directly would
/// detach HEAD, which is never what someone picking a branch from the UI wants.
/// So when `name` names a remote branch (and not a local one), point the switch
/// at a local branch instead: reuse an existing one by the same short name, or
/// create it at the remote tip and set the remote as its upstream.
///
/// Returns the name to switch to. Anything that isn't a remote branch -- local
/// branches, tags, shas -- passes through untouched.
fn resolve_switch_target(repo: &git2::Repository, name: &str) -> Result<String, AppError> {
  if repo.find_branch(name, BranchType::Local).is_ok() {
    return Ok(name.to_string());
  }

  let Ok(remote_branch) = repo.find_branch(name, BranchType::Remote) else {
    return Ok(name.to_string());
  };

  // Strip the remote prefix: `origin/feature/x` -> `feature/x`. A remote branch
  // name always carries one, but guard anyway rather than panic.
  let Some((_, short)) = name.split_once('/') else {
    return Ok(name.to_string());
  };
  if short.is_empty() {
    return Ok(name.to_string());
  }

  // A local branch by that name already exists (it just wasn't what was
  // clicked). Switch to it rather than trying to create a duplicate.
  if repo.find_branch(short, BranchType::Local).is_ok() {
    return Ok(short.to_string());
  }

  let target = remote_branch.get().peel_to_commit()?;
  let mut created = repo.branch(short, &target, false)?;
  created.set_upstream(Some(name))?;
  Ok(short.to_string())
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

    // Picking a remote branch lands on a local tracking branch, not detached HEAD.
    let name = resolve_switch_target(&repo, &name)?;

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

      // Try to carry the changes across with a plain checkout first, exactly
      // like `git checkout <branch>` does. Git happily brings uncommitted
      // changes along when they don't collide with the target branch -- and it
      // handles cases stash can't, like a moved submodule pointer (stashing
      // that fails with "nothing to stash" and used to leave the user stuck).
      // Only when git actually refuses do we fall back to stash -> switch ->
      // reapply.
      BranchSwitchMode::AutoStash => {
        if switch_to(&repo, &name).is_ok() {
          return Ok(CheckoutOutcome::Clean);
        }

        // The plain checkout was refused (a real collision). Stash the changes,
        // switch, and bring them back.
        //
        // We deliberately use stash_APPLY (not pop) and drop the stash ourselves
        // only on a clean apply. git2's stash_pop returns Ok even when the apply
        // conflicts AND drops the stash regardless -- so a naive pop would destroy
        // the user's backup exactly when they need it. Applying and conditionally
        // dropping keeps the stash as a backup whenever conflicts remain.
        let signature = repo.signature()?;
        if let Err(e) = repo.stash_save(
          &signature,
          &format!("gitwyrm: auto-stash before switching to {name}"),
          Some(git2::StashFlags::INCLUDE_UNTRACKED),
        ) {
          // Nothing could be stashed yet the plain switch was refused: the
          // blocker is un-stashable (e.g. a submodule move that also collides).
          // Give actionable guidance instead of the raw git2 error.
          if e.class() == git2::ErrorClass::Stash && e.code() == git2::ErrorCode::NotFound {
            return Err(AppError::Other(SUBMODULE_SWITCH_HINT.into()));
          }
          return Err(e.into());
        }

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

/// Create a branch. `sha` names the commit to branch from; empty means the
/// current HEAD. When `checkout` is set, HEAD moves onto the new branch.
#[tauri::command]
#[specta::specta]
pub async fn create_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  sha: String,
  checkout: bool,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let target = if sha.trim().is_empty() {
      repo
        .head()?
        .peel_to_commit()
        .map_err(|_| AppError::Other("repository has no commits yet".into()))?
    } else {
      let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
      repo.find_commit(oid)?
    };
    repo.branch(&name, &target, false)?;
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

/// Create a tag. `sha` is the commit to tag (empty = current HEAD). When
/// `message` is non-empty the tag is annotated (carries author + message);
/// otherwise it is a lightweight tag pointing straight at the commit.
#[tauri::command]
#[specta::specta]
pub async fn create_tag(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  sha: String,
  message: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let name = name.trim();
    if name.is_empty() {
      return Err(AppError::Other("tag name is required".into()));
    }

    // Resolve the target commit: an explicit sha, or HEAD when none is given.
    let target = if sha.trim().is_empty() {
      repo
        .head()?
        .peel_to_commit()
        .map_err(|_| AppError::Other("repository has no commits yet".into()))?
        .into_object()
    } else {
      let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
      repo.find_object(oid, None)?
    };

    let message = message.trim();
    if message.is_empty() {
      repo.tag_lightweight(name, &target, false)?;
    } else {
      let signature = repo.signature()?;
      repo.tag(name, &target, &signature, message, false)?;
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete a tag by name.
#[tauri::command]
#[specta::specta]
pub async fn delete_tag(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    repo.tag_delete(name.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete a local branch. Refuses to delete the branch HEAD is on.
#[tauri::command]
#[specta::specta]
pub async fn delete_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut branch = repo.find_branch(name.trim(), BranchType::Local)?;
    if branch.is_head() {
      return Err(AppError::Other(
        "that is the branch you're on; switch to another branch first".into(),
      ));
    }
    branch.delete()?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Reset the current branch to a commit. Hard reset discards uncommitted work,
/// so it is refused over a dirty tree. Returns where the branch pointed before
/// the reset, so the caller can offer an undo. Soft/Mixed keep the working tree.
#[tauri::command]
#[specta::specta]
pub async fn reset_current(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
  mode: ResetMode,
) -> Result<RefMove, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let branch = current_branch_name(&repo)?;

    if mode == ResetMode::Hard && tree_dirty(&repo)? {
      return Err(AppError::Other(
        "working tree has changes; a hard reset would discard them - commit or stash first".into(),
      ));
    }

    let previous_sha = repo.head()?.peel_to_commit()?.id().to_string();

    let target_oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    let target = repo.find_object(target_oid, None)?;
    let kind = match mode {
      ResetMode::Soft => ResetType::Soft,
      ResetMode::Mixed => ResetType::Mixed,
      ResetMode::Hard => ResetType::Hard,
    };
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    let checkout = if mode == ResetMode::Hard { Some(&mut checkout) } else { None };
    repo.reset(&target, kind, checkout)?;

    Ok(RefMove { branch, previous_sha })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Move the current branch ref to a commit without touching the working tree
/// (like `git branch -f <current> <sha>` re-pointing HEAD's branch). Refused
/// over a dirty tree so the tree never silently diverges from the new tip.
#[tauri::command]
#[specta::specta]
pub async fn move_current_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
) -> Result<RefMove, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let branch = current_branch_name(&repo)?;

    if tree_dirty(&repo)? {
      return Err(AppError::Other(
        "working tree has changes; commit or stash before moving the branch".into(),
      ));
    }

    let previous_sha = repo.head()?.peel_to_commit()?.id().to_string();
    let target_oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    // A soft reset re-points the branch ref and updates the index/HEAD tree to
    // match, keeping the working tree in sync with the new tip.
    let target = repo.find_object(target_oid, None)?;
    repo.reset(&target, ResetType::Soft, None)?;

    Ok(RefMove { branch, previous_sha })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Check out a commit directly, leaving HEAD detached (not on any branch).
/// Refused over a dirty tree so no uncommitted work is clobbered. The frontend
/// warns that new commits here won't belong to a branch until one is made.
#[tauri::command]
#[specta::specta]
pub async fn checkout_commit(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    if is_dirty(&repo)? {
      return Err(AppError::Other(
        "working tree has changes; commit or stash before checking out a commit".into(),
      ));
    }

    let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    let object = repo.find_object(oid, None)?;
    repo.checkout_tree(&object, Some(CheckoutBuilder::new().safe()))?;
    repo.set_head_detached(oid)?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Reword a commit's message. Only the tip commit (HEAD) is supported: it is
/// amended in place, keeping its tree, parent, and original author. Rewording
/// an older commit would rewrite history below it (a rebase) and is refused.
#[tauri::command]
#[specta::specta]
pub async fn reword_commit(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
  message: String,
) -> Result<String, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let head = repo
      .head()?
      .peel_to_commit()
      .map_err(|_| AppError::Other("repository has no commits yet".into()))?;
    let target_oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    if head.id() != target_oid {
      return Err(AppError::Other(
        "only the latest commit's message can be edited right now".into(),
      ));
    }

    let message = message.trim();
    if message.is_empty() {
      return Err(AppError::Other("a commit message is required".into()));
    }

    // Amend keeps the tree and parent; pass the original author so only the
    // message (and committer) change.
    let new_oid = head.amend(Some("HEAD"), Some(&head.author()), None, None, Some(message), None)?;
    Ok(new_oid.to_string())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Revert a commit: apply the inverse of its changes as a new commit on top of
/// HEAD, so history is preserved. A clean revert commits immediately and
/// returns no conflicts. A conflicting revert leaves REVERT_HEAD and the
/// conflicted index for the shared conflict flow to resolve and finish.
#[tauri::command]
#[specta::specta]
pub async fn revert_commit(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
) -> Result<crate::git::types::MergeResult, AppError> {
  use crate::git::types::MergeResult;
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    if tree_dirty(&repo)? {
      return Err(AppError::Other(
        "working tree has changes; commit or stash before reverting".into(),
      ));
    }

    let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    let commit = repo.find_commit(oid)?;
    if commit.parent_count() > 1 {
      return Err(AppError::Other(
        "that is a merge commit; reverting merges is not supported yet".into(),
      ));
    }

    repo.revert(&commit, None)?;

    let conflicts = conflicted_index_paths(&repo)?;
    if !conflicts.is_empty() {
      return Ok(MergeResult { up_to_date: false, fast_forwarded: false, conflicts });
    }

    // Clean revert: commit the inverse as a single-parent commit.
    let mut index = repo.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;
    let signature = repo.signature()?;
    let head_commit = repo.head()?.peel_to_commit()?;
    let summary = commit.summary().unwrap_or("commit");
    let message = format!("Revert \"{summary}\"\n\nThis reverts commit {oid}.");
    repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &[&head_commit])?;
    repo.cleanup_state()?;

    Ok(MergeResult { up_to_date: false, fast_forwarded: false, conflicts: Vec::new() })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Drop a commit from the current branch: replay every commit after it onto its
/// parent, removing just that one. Only works on a clean linear stretch (no
/// merge commits above the target). A conflict during replay aborts the whole
/// operation and restores the branch, so the branch is never left half-rebased.
#[tauri::command]
#[specta::specta]
pub async fn drop_commit(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
) -> Result<RefMove, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let branch = current_branch_name(&repo)?;

    if tree_dirty(&repo)? {
      return Err(AppError::Other(
        "working tree has changes; commit or stash before dropping a commit".into(),
      ));
    }

    let target_oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    let target = repo.find_commit(target_oid)?;
    if target.parent_count() != 1 {
      return Err(AppError::Other(
        "only commits with a single parent can be dropped".into(),
      ));
    }
    let parent = target.parent(0)?;
    let previous_sha = repo.head()?.peel_to_commit()?.id().to_string();

    // Collect the commits from HEAD down to (not including) the target, newest
    // first, so we can replay them oldest-first onto the target's parent.
    let head_oid = repo.head()?.peel_to_commit()?.id();
    let mut to_replay = Vec::new();
    let mut walk = repo.revwalk()?;
    walk.push(head_oid)?;
    let mut found = false;
    for step in walk {
      let step_oid = step?;
      if step_oid == target_oid {
        found = true;
        break;
      }
      let c = repo.find_commit(step_oid)?;
      if c.parent_count() > 1 {
        return Err(AppError::Other(
          "there is a merge commit above this one; can't drop it safely".into(),
        ));
      }
      to_replay.push(c);
    }
    if !found {
      return Err(AppError::Other("that commit is not on the current branch".into()));
    }
    to_replay.reverse();

    // Cherry-pick each later commit onto the growing new tip. Any conflict
    // aborts: reset back to where we started so nothing is left half-done.
    let signature = repo.signature()?;
    let mut new_tip = parent;
    for commit in to_replay {
      let mut index = repo
        .cherrypick_commit(&commit, &new_tip, 0, None)
        .map_err(AppError::Git)?;
      if index.has_conflicts() {
        let start_oid = Oid::from_str(&previous_sha).map_err(AppError::Git)?;
        let head_obj = repo.find_object(start_oid, None)?;
        repo.reset(&head_obj, ResetType::Hard, Some(CheckoutBuilder::new().force()))?;
        return Err(AppError::Other(
          "dropping this commit causes conflicts in a later commit; nothing was changed".into(),
        ));
      }
      let tree_oid = index.write_tree_to(&repo)?;
      let tree = repo.find_tree(tree_oid)?;
      let message = commit.message().unwrap_or("");
      let new_oid = repo.commit(
        None,
        &commit.author(),
        &signature,
        message,
        &tree,
        &[&new_tip],
      )?;
      new_tip = repo.find_commit(new_oid)?;
    }

    // Point the branch at the rebuilt tip and sync the working tree.
    repo.reset(new_tip.as_object(), ResetType::Hard, Some(CheckoutBuilder::new().force()))?;

    Ok(RefMove { branch, previous_sha })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Paths currently conflicted in the index (used by revert's conflict path).
fn conflicted_index_paths(repo: &git2::Repository) -> Result<Vec<String>, AppError> {
  let index = repo.index()?;
  if !index.has_conflicts() {
    return Ok(Vec::new());
  }
  let mut paths = Vec::new();
  for entry in index.conflicts()? {
    let entry = entry?;
    let raw = entry
      .our
      .as_ref()
      .or(entry.their.as_ref())
      .or(entry.ancestor.as_ref())
      .map(|e| e.path.clone());
    if let Some(bytes) = raw {
      paths.push(String::from_utf8_lossy(&bytes).into_owned());
    }
  }
  paths.sort();
  paths.dedup();
  Ok(paths)
}

/// True when the repo has at least one linked worktree. Backs the auto-enable
/// of the worktree feature so users who already work with worktrees see the UI.
#[tauri::command]
#[specta::specta]
pub async fn has_worktrees(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<bool, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    Ok(!repo.worktrees()?.is_empty())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Resolve the origin remote's web URL for a commit, or None when it can't be
/// built (no origin, unknown host) or the commit isn't on any remote-tracking
/// branch yet (so the link would 404). Supports GitHub, GitLab, Bitbucket.
#[tauri::command]
#[specta::specta]
pub async fn commit_web_url(
  manager: State<'_, RepoManager>,
  repo_id: String,
  sha: String,
) -> Result<Option<String>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let Ok(remote) = repo.find_remote("origin") else {
      return Ok(None);
    };
    let Some(url) = remote.url() else { return Ok(None) };
    let Some(base) = web_base_from_remote(url) else { return Ok(None) };

    // Only link commits that are reachable from a remote-tracking branch;
    // otherwise the host has never seen the sha and the URL would 404.
    let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    if !commit_on_any_remote(&repo, oid)? {
      return Ok(None);
    }

    Ok(Some(format!("{base}/commit/{}", oid)))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Normalize a git remote URL (ssh or https) to its web base, e.g.
/// `git@github.com:o/r.git` / `https://github.com/o/r.git` -> `https://github.com/o/r`.
fn web_base_from_remote(url: &str) -> Option<String> {
  let known = ["github.com", "gitlab.com", "bitbucket.org"];

  let (host, path) = if let Some(rest) = url.strip_prefix("git@") {
    // scp-like: git@host:owner/repo(.git)
    let (host, path) = rest.split_once(':')?;
    (host.to_string(), path.to_string())
  } else if let Some(rest) = url.strip_prefix("ssh://git@") {
    let (host, path) = rest.split_once('/')?;
    (host.to_string(), path.to_string())
  } else if let Some(rest) = url.strip_prefix("https://") {
    let rest = rest.strip_prefix("git@").unwrap_or(rest);
    let (host, path) = rest.split_once('/')?;
    (host.to_string(), path.to_string())
  } else {
    return None;
  };

  if !known.contains(&host.as_str()) {
    return None;
  }
  let path = path.trim_end_matches('/').trim_end_matches(".git");
  Some(format!("https://{host}/{path}"))
}

/// True when `oid` is reachable from any `refs/remotes/*` branch tip.
fn commit_on_any_remote(repo: &git2::Repository, oid: Oid) -> Result<bool, AppError> {
  let refs = repo.references_glob("refs/remotes/*")?;
  for r in refs.flatten() {
    if let Some(tip) = r.target() {
      if tip == oid || repo.graph_descendant_of(tip, oid).unwrap_or(false) {
        return Ok(true);
      }
    }
  }
  Ok(false)
}
