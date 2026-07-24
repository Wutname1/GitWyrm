use git2::{build::CheckoutBuilder, BranchType, Oid, ResetType};
use tauri::State;

use crate::error::AppError;
use crate::git::refs;
use crate::git::types::{
  BranchInfo, BranchList, BranchRelation, CheckoutOutcome, RefMove, ResetMode, SyncState, TagInfo,
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

/// Reject a ref name git would refuse, before handing it to git2.
///
/// The frontend checks the common mistakes so it can explain them, but it
/// cannot be the boundary: commands are callable regardless, and git's rules
/// (control characters, reserved sequences) are wider than a regex worth
/// maintaining twice. `is_valid_name` wants the full refname, not the
/// shorthand.
fn validate_ref_name(name: &str, prefix: &str, kind: &str) -> Result<(), AppError> {
  if name.is_empty() {
    return Err(AppError::Other(format!("Enter a name for the {kind}.")));
  }
  if !git2::Reference::is_valid_name(&format!("{prefix}{name}")) {
    return Err(AppError::Other(format!(
      "\"{name}\" isn't a name git accepts. Try letters, numbers, dashes and slashes."
    )));
  }
  Ok(())
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
    let records = refs::walk_branches(&repo)?;
    let tips = refs::remote_tips(&records);

    let mut local = Vec::new();
    let mut remote = Vec::new();

    for rec in &records {
      if rec.is_remote {
        remote.push(rec.name.clone());
        continue;
      }

      // Three distinct outcomes, where the old code returned (0, 0) for all of
      // them: no upstream at all, an upstream whose ref is gone, and a real
      // comparison.
      let sync = match (&rec.upstream, rec.tip) {
        (Some(up), Some(local_oid)) => match tips.get(up.as_str()) {
          Some(&up_oid) => refs::ahead_behind(&repo, local_oid, up_oid)
            .map(|(a, b)| SyncState::from_counts(a, b))
            .unwrap_or(SyncState::UpstreamGone),
          None => SyncState::UpstreamGone,
        },
        (Some(_), None) => SyncState::UpstreamGone,
        (None, _) => SyncState::NeverPushed,
      };
      let (ahead, behind) = sync.counts();

      local.push(BranchInfo {
        name: rec.name.clone(),
        is_head: rec.is_head,
        upstream: rec.upstream.clone(),
        ahead,
        behind,
        sync,
        time: rec.time.map(|t| t as f64),
        tip: rec.tip.map(|oid| format!("{:.7}", oid)),
      });
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
  // Best-effort: git can only resolve the upstream when the remote is
  // configured with a matching refspec. If it can't (stale remote refs, an
  // unusual refspec), the branch still exists at the right commit -- landing on
  // it matters more than the tracking link, so don't fail the whole switch.
  let _ = created.set_upstream(Some(name));
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

    if !refs::any_changes_present(&repo)? {
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
    let name = name.trim();
    validate_ref_name(name, "refs/heads/", "branch")?;
    let target = if sha.trim().is_empty() {
      repo
        .head()?
        .peel_to_commit()
        .map_err(|_| AppError::Other("repository has no commits yet".into()))?
    } else {
      let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
      repo.find_commit(oid)?
    };
    repo.branch(name, &target, false)?;
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
      .filter_map(|n| {
        // Peel to the commit so annotated and lightweight tags both report the
        // commit they mark, not the intermediate tag object.
        let reference = repo.find_reference(&format!("refs/tags/{n}")).ok()?;
        let target_sha = reference.peel_to_commit().ok()?.id().to_string();
        let annotated = reference.peel_to_tag().is_ok();
        Some(TagInfo { name: n.to_string(), target_sha, annotated })
      })
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
    validate_ref_name(name, "refs/tags/", "tag")?;

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

/// Rename a local branch. Safe on the branch you are currently on, unlike
/// delete: git moves HEAD along with the ref. The upstream link is preserved,
/// so a renamed branch still pushes where it did before.
#[tauri::command]
#[specta::specta]
pub async fn rename_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  new_name: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let new_name = new_name.trim();
    validate_ref_name(new_name, "refs/heads/", "branch")?;
    if repo.find_branch(new_name, BranchType::Local).is_ok() {
      return Err(AppError::Other(format!("A branch named {new_name} already exists.")));
    }
    let mut branch = repo.find_branch(name.trim(), BranchType::Local)?;
    // `force = false`: never clobber an existing ref, checked above for a
    // clearer message than git2's.
    branch.rename(new_name, false)?;
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
    let target_oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
    let commit = repo.find_commit(target_oid)?;
    reset_current_to_commit(&repo, &commit, mode)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Reset the current branch to another ref (a branch name or any revspec),
/// resolving it to its tip commit. This backs "reset this branch to that
/// branch": while `<current>` is checked out, right-clicking or dropping onto
/// `<other>` rewinds `<current>` to wherever `<other>` points. Same discard
/// rules as [`reset_current`] - a hard reset is refused over a dirty tree.
#[tauri::command]
#[specta::specta]
pub async fn reset_current_to_ref(
  manager: State<'_, RepoManager>,
  repo_id: String,
  target_ref: String,
  mode: ResetMode,
) -> Result<RefMove, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let object = repo.revparse_single(target_ref.trim())?;
    let commit = object.peel_to_commit()?;
    reset_current_to_commit(&repo, &commit, mode)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Shared core for the reset commands: rewind the checked-out branch to a
/// resolved commit and report where it pointed before. Refuses a hard reset
/// over a dirty tree so committed-but-not-yet-saved work is never silently lost.
fn reset_current_to_commit(
  repo: &git2::Repository,
  commit: &git2::Commit,
  mode: ResetMode,
) -> Result<RefMove, AppError> {
  let branch = current_branch_name(repo)?;

  if mode == ResetMode::Hard && refs::tracked_changes_present(repo)? {
    return Err(AppError::Other(
      "working tree has changes; a hard reset would discard them - commit or stash first".into(),
    ));
  }

  let previous_sha = repo.head()?.peel_to_commit()?.id().to_string();

  let kind = match mode {
    ResetMode::Soft => ResetType::Soft,
    ResetMode::Mixed => ResetType::Mixed,
    ResetMode::Hard => ResetType::Hard,
  };
  let mut checkout = CheckoutBuilder::new();
  checkout.force();
  let checkout = if mode == ResetMode::Hard { Some(&mut checkout) } else { None };
  repo.reset(commit.as_object(), kind, checkout)?;

  Ok(RefMove { branch, previous_sha })
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

    if refs::tracked_changes_present(&repo)? {
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

/// Fast-forward a branch to another ref, without merging or a merge commit.
///
/// This is `git branch -f <branch> <target>` restricted to the safe case: it
/// only runs when `target` is a descendant of `branch`, so no work is ever
/// discarded. Any other relationship (diverged, or `branch` already ahead) is
/// refused with a plain message, since that would need a real merge or a reset.
///
/// The key difference from the reset/merge commands: `branch` need NOT be the
/// checked-out one. Moving a non-HEAD branch forward is a pure ref update - it
/// doesn't touch the working tree and doesn't switch branches, which is what
/// "bring main up to my current branch, without leaving it" needs. When the
/// branch IS checked out, the working tree is updated to match and a dirty tree
/// is refused so nothing is clobbered.
#[tauri::command]
#[specta::specta]
pub async fn fast_forward_branch(
  manager: State<'_, RepoManager>,
  repo_id: String,
  branch: String,
  target: String,
) -> Result<RefMove, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let mut branch_ref = repo.find_branch(branch.trim(), BranchType::Local)?;
    let branch_oid = branch_ref
      .get()
      .peel_to_commit()
      .map_err(|_| AppError::Other(format!("{} has no commits to move.", branch.trim())))?
      .id();
    let target_oid = repo.revparse_single(target.trim())?.peel_to_commit()?.id();

    if branch_oid == target_oid {
      return Err(AppError::Other(format!(
        "{} is already at {}. Nothing to do.",
        branch.trim(),
        target.trim()
      )));
    }
    // A clean fast-forward means the target is strictly ahead of the branch.
    // Anything else (branch ahead, or the two diverged) can't move by just
    // sliding the ref forward - it would need a merge or a reset instead.
    if !repo.graph_descendant_of(target_oid, branch_oid).unwrap_or(false) {
      return Err(AppError::Other(format!(
        "{} can't just catch up to {} - their histories have split. Merge or rebase instead.",
        branch.trim(),
        target.trim()
      )));
    }

    let is_head = repo
      .head()
      .ok()
      .and_then(|h| h.shorthand().map(str::to_string))
      .as_deref()
      == Some(branch.trim());

    let previous_sha = branch_oid.to_string();

    if is_head {
      // The branch is checked out, so the working tree must move with it. Refuse
      // over uncommitted changes - a fast-forward checkout would overwrite them.
      if refs::tracked_changes_present(&repo)? {
        return Err(AppError::Other(
          "working tree has changes; commit or stash before moving this branch".into(),
        ));
      }
      let object = repo.find_object(target_oid, None)?;
      repo.checkout_tree(&object, Some(CheckoutBuilder::new().safe()))?;
      let head_ref = repo.head()?;
      let name = head_ref.name().map(str::to_string);
      drop(head_ref);
      match name {
        Some(head_ref_name) => {
          repo.reference(&head_ref_name, target_oid, true, "fast-forward")?;
        }
        None => repo.set_head_detached(target_oid)?,
      }
    } else {
      // Not checked out: a pure ref move. The working tree and HEAD are
      // untouched, so the user stays on their current branch.
      let refname =
        branch_ref.get().name().map(str::to_string).ok_or_else(|| {
          AppError::Other("could not read the branch reference".into())
        })?;
      repo.reference(&refname, target_oid, true, "fast-forward")?;
    }

    Ok(RefMove { branch: branch.trim().to_string(), previous_sha })
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

    if refs::any_changes_present(&repo)? {
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

/// Reword a commit's message. The tip commit (HEAD) is amended in place. An
/// older commit is rebuilt with the new message and every commit after it is
/// replayed on top, which rewrites those commits' SHAs (the same history
/// rewrite as dropping a commit). Returns the new SHA of the reworded commit.
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

    let message = message.trim();
    if message.is_empty() {
      return Err(AppError::Other("a commit message is required".into()));
    }

    // Fast path: the tip commit amends in place, keeping tree and parent.
    if head.id() == target_oid {
      let new_oid =
        head.amend(Some("HEAD"), Some(&head.author()), None, None, Some(message), None)?;
      return Ok(new_oid.to_string());
    }

    // Older commit: rebuild it with the new message and replay the commits
    // above it. This needs a clean tree, the same as dropping a commit.
    if refs::tracked_changes_present(&repo)? {
      return Err(AppError::Other(
        "working tree has changes; commit or stash before editing an older message".into(),
      ));
    }

    let target = repo.find_commit(target_oid)?;
    if target.parent_count() != 1 {
      return Err(AppError::Other(
        "only commits with a single parent can be edited right now".into(),
      ));
    }
    let parent = target.parent(0)?;
    let previous_sha = head.id().to_string();

    // Collect the commits from HEAD down to (not including) the target, newest
    // first, so we can replay them oldest-first onto the reworded commit.
    let mut to_replay = Vec::new();
    let mut walk = repo.revwalk()?;
    walk.push(head.id())?;
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
          "there is a merge commit above this one; can't edit it safely".into(),
        ));
      }
      to_replay.push(c);
    }
    if !found {
      return Err(AppError::Other("that commit is not on the current branch".into()));
    }
    to_replay.reverse();

    // Rebuild the target with the new message, same tree, parent, and author.
    let signature = repo.signature()?;
    let new_target_oid = repo.commit(
      None,
      &target.author(),
      &signature,
      message,
      &target.tree()?,
      &[&parent],
    )?;
    let mut new_tip = repo.find_commit(new_target_oid)?;
    let reworded_sha = new_target_oid.to_string();

    // Cherry-pick each later commit onto the growing new tip. Any conflict
    // aborts: reset back to where we started so nothing is left half-done.
    for commit in to_replay {
      let mut index = repo
        .cherrypick_commit(&commit, &new_tip, 0, None)
        .map_err(AppError::Git)?;
      if index.has_conflicts() {
        let start_oid = Oid::from_str(&previous_sha).map_err(AppError::Git)?;
        let head_obj = repo.find_object(start_oid, None)?;
        repo.reset(&head_obj, ResetType::Hard, Some(CheckoutBuilder::new().force()))?;
        return Err(AppError::Other(
          "editing this message causes conflicts in a later commit; nothing was changed".into(),
        ));
      }
      let tree_oid = index.write_tree_to(&repo)?;
      let tree = repo.find_tree(tree_oid)?;
      let msg = commit.message().unwrap_or("");
      let new_oid = repo.commit(None, &commit.author(), &signature, msg, &tree, &[&new_tip])?;
      new_tip = repo.find_commit(new_oid)?;
    }

    // Point the branch at the rebuilt tip and sync the working tree.
    repo.reset(new_tip.as_object(), ResetType::Hard, Some(CheckoutBuilder::new().force()))?;

    Ok(reworded_sha)
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

    if refs::tracked_changes_present(&repo)? {
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

    let conflicts = refs::conflicted_paths(&repo)?;
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

    if refs::tracked_changes_present(&repo)? {
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
