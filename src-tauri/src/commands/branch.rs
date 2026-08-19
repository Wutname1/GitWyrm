use git2::{build::CheckoutBuilder, BranchType, Oid, ResetType};
use tauri::State;

use crate::error::AppError;
use crate::git::commit_write::{self, CommitIdentity};
use crate::git::history;
use crate::git::refs;
use crate::git::remote_url;
use crate::git::submodule;
use crate::git::types::{
    BranchDeleteOutcome, BranchInfo, BranchList, BranchRelation, CheckoutOutcome, CheckoutReport,
    RefMove, ResetMode, SubmoduleFollowed, SyncState, TagInfo,
};
use crate::git::version_sort::compare_tag_names;
use crate::git::worktree;
use crate::settings::BranchSwitchMode;
use crate::state::RepoManager;

/// Name of the branch HEAD points at, or an error if HEAD is detached.
fn current_branch_name(repo: &git2::Repository) -> Result<String, AppError> {
    let head = repo.head()?;
    if !head.is_branch() {
        return Err(AppError::Other(
            "HEAD is detached; check out a branch first".into(),
        ));
    }
    head.shorthand()
        .ok()
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
        let _timing = crate::perf::CommandTiming::start("list_branches", "git.branches");
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
        Ok(BranchRelation {
            ahead: ahead as u32,
            behind: behind as u32,
        })
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
///
/// Deliberately does NOT touch submodules: this is called on recovery paths too
/// (restoring a stash after a failed switch), where moving nested checkouts
/// would be wrong. Callers that landed on the new branch for real follow up
/// with [`follow_switched_pins`].
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

/// Move nested checkouts to the commits the branch we just switched to records.
///
/// Switching branches writes the new gitlink into the parent's index but leaves
/// the submodule folder on its old commit, so the parent reports a pending
/// change the user never made. Best-effort: the switch itself has already
/// succeeded, and a submodule whose commit is unreachable must not turn that
/// into an error.
///
/// Only call this once the switch has actually landed -- never on a path that
/// is unwinding a failed switch.
fn follow_switched_pins(repo: &git2::Repository, repo_path: &str) -> Vec<SubmoduleFollowed> {
    submodule::follow_and_report(repo, repo_path, "switching branches")
}

#[tauri::command]
#[specta::specta]
pub async fn checkout_branch(
    manager: State<'_, RepoManager>,
    repo_id: String,
    name: String,
    mode: BranchSwitchMode,
) -> Result<CheckoutReport, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo_path = open.path.to_string_lossy().into_owned();
        let mut repo = open.repo.lock().unwrap();

        // Picking a remote branch lands on a local tracking branch, not detached HEAD.
        let name = resolve_switch_target(&repo, &name)?;

        // A branch can only be checked out in one folder at a time. Git refuses
        // this with text that names neither the folder nor a way forward, so catch
        // it here and hand back the worktree that holds it -- the UI's offer to
        // open that folder is the only useful thing to do about it.
        if let Some(holder) = worktree::holder_of_branch(&repo, &name) {
            if !holder.is_current {
                // Nothing was switched, so nothing could have moved a pin.
                return Ok(CheckoutReport {
                    outcome: CheckoutOutcome::HeldByWorktree,
                    submodules: Vec::new(),
                });
            }
        }

        if !refs::any_changes_present(&repo)? {
            switch_to(&repo, &name)?;
            let submodules = follow_switched_pins(&repo, &repo_path);
            return Ok(CheckoutReport {
                outcome: CheckoutOutcome::Clean,
                submodules,
            });
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
                let submodules = follow_switched_pins(&repo, &repo_path);
                Ok(CheckoutReport {
                    outcome: CheckoutOutcome::Clean,
                    submodules,
                })
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
                    let submodules = follow_switched_pins(&repo, &repo_path);
                    return Ok(CheckoutReport {
                        outcome: CheckoutOutcome::Clean,
                        submodules,
                    });
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
                    if e.class() == git2::ErrorClass::Stash && e.code() == git2::ErrorCode::NotFound
                    {
                        return Err(AppError::Other(SUBMODULE_SWITCH_HINT.into()));
                    }
                    return Err(e.into());
                }

                // If the switch itself fails, restore the stash so nothing is lost.
                // Apply-then-conditionally-drop for the same reason as below: a pop here
                // would discard the backup on a conflicted apply, which is exactly the
                // moment the user needs it most.
                if let Err(e) = switch_to(&repo, &name) {
                    match repo.stash_apply(0, None) {
                        // Restored cleanly, so the stash entry is redundant.
                        Ok(()) if !repo.index().map(|i| i.has_conflicts()).unwrap_or(true) => {
                            let _ = repo.stash_drop(0);
                        }
                        // Conflicted or failed: keep the stash and say so, since the
                        // original switch error alone would not explain the tree state.
                        _ => {
                            return Err(AppError::Other(format!(
                "could not switch to {name}, and your changes could not be put back \
                 automatically - they are safe in the most recent stash: {e}"
              )));
                        }
                    }
                    return Err(e);
                }

                // The switch landed. Move nested checkouts now, BEFORE restoring the
                // stash -- the stash can carry the user's own submodule pointer move,
                // and re-applying it after this would put their choice back on top
                // rather than have it overwritten.
                let submodules = follow_switched_pins(&repo, &repo_path);

                // A failed re-apply leaves the user on the new branch WITHOUT their
                // changes. Report where the work went instead of propagating a raw error.
                if repo.stash_apply(0, None).is_err() {
                    return Ok(CheckoutReport {
                        outcome: CheckoutOutcome::StashNotReapplied,
                        submodules,
                    });
                }

                if repo.index()?.has_conflicts() {
                    // Leave stash@{0} in place as a backup; the working tree has markers.
                    Ok(CheckoutReport {
                        outcome: CheckoutOutcome::StashPopConflict,
                        submodules,
                    })
                } else {
                    // Clean apply: drop the now-redundant stash entry.
                    repo.stash_drop(0)?;
                    Ok(CheckoutReport {
                        outcome: CheckoutOutcome::Stashed,
                        submodules,
                    })
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
        let repo_path = open.path.to_string_lossy().into_owned();
        let repo = open.repo.lock().unwrap();
        let name = name.trim();
        validate_ref_name(name, "refs/heads/", "branch")?;
        let target = if sha.trim().is_empty() {
            repo.head()?
                .peel_to_commit()
                .map_err(|_| AppError::Other("repository has no commits yet".into()))?
        } else {
            // revparse, not `Oid::from_str`: callers pass whatever sha they have on
            // hand, and several carry the abbreviated form (remote branch tips are
            // stored 7 chars wide). revparse resolves both, and a ref name too.
            repo.revparse_single(sha.trim())
                .map_err(|_| AppError::Other(format!("could not find commit {}", sha.trim())))?
                .peel_to_commit()
                .map_err(|_| AppError::Other(format!("{} is not a commit", sha.trim())))?
        };
        repo.branch(name, &target, false)?;
        if checkout {
            let refname = format!("refs/heads/{name}");
            let object = repo.revparse_single(&refname)?;
            repo.checkout_tree(&object, None)?;
            repo.set_head(&refname)?;
            // Branching from an older commit can pin a submodule elsewhere, and the
            // checkout moves only the parent's pointer.
            follow_switched_pins(&repo, &repo_path);
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
            .flatten()
            .filter_map(|n| {
                // Peel to the commit so annotated and lightweight tags both report the
                // commit they mark, not the intermediate tag object.
                let reference = repo.find_reference(&format!("refs/tags/{n}")).ok()?;
                let commit = reference.peel_to_commit().ok()?;
                let target_sha = commit.id().to_string();
                let tag = reference.peel_to_tag().ok();
                // An annotated tag knows when it was made; a lightweight one doesn't, so
                // fall back to the commit it marks.
                let time = tag
                    .as_ref()
                    .and_then(|t| t.tagger())
                    .map(|sig| sig.when().seconds())
                    .unwrap_or_else(|| commit.time().seconds()) as f64;
                Some(TagInfo {
                    name: n.to_string(),
                    target_sha,
                    annotated: tag.is_some(),
                    time,
                })
            })
            .collect();
        // Newest first, comparing numeric runs as numbers so 0.0.11 lands above
        // 0.0.2 the way a person reading version numbers expects.
        tags.sort_by(|a, b| compare_tag_names(&b.name, &a.name));
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
            repo.head()?
                .peel_to_commit()
                .map_err(|_| AppError::Other("repository has no commits yet".into()))?
                .into_object()
        } else {
            let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
            repo.find_object(oid, None)?
        };

        // If a tag by this name already points at the same commit, treat the call
        // as a no-op success. A duplicate create for the identical tag is never a
        // user error worth surfacing -- it only happens when the same request
        // reaches the backend twice, and libgit2's "tag already exists" would
        // otherwise flash as a spurious warning right after the tag is made.
        if let Ok(existing) = repo.find_reference(&format!("refs/tags/{name}")) {
            if let (Ok(existing_oid), Some(target_oid)) = (
                existing.peel_to_commit().map(|c| c.id()),
                target.peel_to_commit().ok().map(|c| c.id()),
            ) {
                if existing_oid == target_oid {
                    return Ok(());
                }
            }
        }

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
///
/// A branch a worktree holds comes back as a typed refusal naming that folder,
/// not as an error. "The branch is in use" is the report that leaves people
/// hunting for where -- and the way out (remove the worktree first) is
/// something GitWyrm can just offer.
#[tauri::command]
#[specta::specta]
pub async fn delete_branch(
    manager: State<'_, RepoManager>,
    repo_id: String,
    name: String,
) -> Result<BranchDeleteOutcome, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        let name = name.trim();
        let mut branch = repo.find_branch(name, BranchType::Local)?;
        if branch.is_head() {
            return Err(AppError::Other(
                "that is the branch you're on; switch to another branch first".into(),
            ));
        }

        // Another checkout has it. Report which one so the caller can offer to
        // remove that worktree and then finish the deletion the user asked for.
        if let Some(holder) = worktree::holder_of_branch(&repo, name) {
            if !holder.is_current {
                return Ok(BranchDeleteOutcome::HeldByWorktree {
                    folder_name: holder.folder_name,
                    path: holder.path,
                });
            }
        }

        branch.delete()?;
        Ok(BranchDeleteOutcome::Deleted)
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
            return Err(AppError::Other(format!(
                "A branch named {new_name} already exists."
            )));
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

/// Set aside any uncommitted tracked changes in a stash so a ref move can go
/// through instead of failing. The stash is kept in the list -- nothing is
/// lost, and the caller reports it via [`RefMove::stashed`]. Returns true when
/// a stash was created.
fn stash_changes_aside(repo: &mut git2::Repository, why: &str) -> Result<bool, AppError> {
    if !refs::tracked_changes_present(repo)? {
        return Ok(false);
    }
    let signature = repo.signature()?;
    match repo.stash_save(
        &signature,
        &format!("gitwyrm: changes set aside before {why}"),
        None,
    ) {
        Ok(_) => Ok(true),
        // Nothing git can stash (e.g. only a submodule pointer move). The move
        // itself won't touch it, so proceed rather than error.
        Err(e) if e.class() == git2::ErrorClass::Stash && e.code() == git2::ErrorCode::NotFound => {
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

/// Reset the current branch to a commit. A hard reset over a dirty tree sets
/// the uncommitted work aside in a stash first, so the rewind always goes
/// through and nothing is silently lost. Returns where the branch pointed
/// before the reset, so the caller can offer an undo.
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
        let mut repo = open.repo.lock().unwrap();
        let target_oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
        reset_current_to_commit(&mut repo, target_oid, mode)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Reset the current branch to another ref (a branch name or any revspec),
/// resolving it to its tip commit. This backs "reset this branch to that
/// branch": while `<current>` is checked out, right-clicking or dropping onto
/// `<other>` rewinds `<current>` to wherever `<other>` points. Same stash-aside
/// rules as [`reset_current`].
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
        let mut repo = open.repo.lock().unwrap();
        let target_oid = repo
            .revparse_single(target_ref.trim())?
            .peel_to_commit()?
            .id();
        reset_current_to_commit(&mut repo, target_oid, mode)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Shared core for the reset commands: rewind the checked-out branch to a
/// resolved commit and report where it pointed before. A hard reset over a
/// dirty tree stashes the uncommitted work first instead of refusing, so
/// not-yet-saved work is never silently lost and the move never errors.
fn reset_current_to_commit(
    repo: &mut git2::Repository,
    target_oid: Oid,
    mode: ResetMode,
) -> Result<RefMove, AppError> {
    let branch = current_branch_name(repo)?;

    let stashed = if mode == ResetMode::Hard {
        stash_changes_aside(repo, &format!("rewinding {branch}"))?
    } else {
        false
    };

    let previous_sha = repo.head()?.peel_to_commit()?.id().to_string();

    let kind = match mode {
        ResetMode::Soft => ResetType::Soft,
        ResetMode::Mixed => ResetType::Mixed,
        ResetMode::Hard => ResetType::Hard,
    };
    let commit = repo.find_commit(target_oid)?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    let checkout = if mode == ResetMode::Hard {
        Some(&mut checkout)
    } else {
        None
    };
    repo.reset(commit.as_object(), kind, checkout)?;

    // A hard reset rewrites the working tree, so a submodule pinned at a
    // different commit by the target needs its nested checkout moved too --
    // otherwise the parent reports a pending change asking to undo the move it
    // just made. Soft/Mixed leave the tree alone and must not touch submodules.
    if mode == ResetMode::Hard {
        submodule::sync_submodule_workdirs(repo);
    }

    Ok(RefMove {
        branch,
        previous_sha,
        stashed,
        submodules: Vec::new(),
    })
}

/// Move the current branch to a commit, leaving the working tree exactly as
/// that commit has it (`git reset --hard <sha>` on HEAD's branch). A dirty tree
/// no longer blocks the move: the changes are set aside in a stash first, so
/// nothing is lost and the tree never silently diverges from the new tip.
#[tauri::command]
#[specta::specta]
pub async fn move_current_branch(
    manager: State<'_, RepoManager>,
    repo_id: String,
    sha: String,
) -> Result<RefMove, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut repo = open.repo.lock().unwrap();
        let target_oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
        move_current_branch_to_commit(&mut repo, target_oid)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Shared core for [`move_current_branch`], split out so it can be tested
/// without a running app.
fn move_current_branch_to_commit(
    repo: &mut git2::Repository,
    target_oid: Oid,
) -> Result<RefMove, AppError> {
    let branch = current_branch_name(repo)?;

    let stashed = stash_changes_aside(repo, &format!("moving {branch}"))?;

    let previous_sha = repo.head()?.peel_to_commit()?.id().to_string();
    // Hard, not soft: a soft reset moves only the branch ref and leaves the index
    // at the old tip, so every difference between the two commits turns up as
    // staged changes. The user asked to *be at* this commit, so send the index
    // and working tree there too.
    let target = repo.find_object(target_oid, None)?;
    repo.reset(
        &target,
        ResetType::Hard,
        Some(CheckoutBuilder::new().force()),
    )?;

    // The reset rewrote the working tree, so a submodule the target pins at a
    // different commit needs its nested checkout moved too -- otherwise the
    // parent reports a pending change asking to undo the move it just made.
    submodule::sync_submodule_workdirs(repo);

    Ok(RefMove {
        branch,
        previous_sha,
        stashed,
        submodules: Vec::new(),
    })
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
        let repo_path = open.path.to_string_lossy().into_owned();
        let repo = open.repo.lock().unwrap();
        fast_forward_branch_to(&repo, &repo_path, &branch, &target)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Shared core for [`fast_forward_branch`], split out so it can be tested
/// without a running app.
fn fast_forward_branch_to(
    repo: &git2::Repository,
    repo_path: &str,
    branch: &str,
    target: &str,
) -> Result<RefMove, AppError> {
    {
        let branch_ref = repo.find_branch(branch.trim(), BranchType::Local)?;
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
        if !repo
            .graph_descendant_of(target_oid, branch_oid)
            .unwrap_or(false)
        {
            return Err(AppError::Other(format!(
        "{} can't just catch up to {} - their histories have split. Merge or rebase instead.",
        branch.trim(),
        target.trim()
      )));
        }

        let is_head = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().ok().map(str::to_string))
            .as_deref()
            == Some(branch.trim());

        let previous_sha = branch_oid.to_string();
        // Only the checked-out arm can move a nested checkout; a pure ref move
        // leaves the working tree alone and has nothing to report.
        let mut followed = Vec::new();

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
            let name = head_ref.name().ok().map(str::to_string);
            drop(head_ref);
            match name {
                Some(head_ref_name) => {
                    repo.reference(&head_ref_name, target_oid, true, "fast-forward")?;
                }
                None => repo.set_head_detached(target_oid)?,
            }

            // The commits we just moved onto can pin a submodule at a different
            // commit. `checkout_tree` writes the new pointer into the parent's index
            // but never touches the nested checkout, so the submodule stays where it
            // was and the parent reports a pending change the user never made --
            // pointing the wrong way, at rolling the pin BACK. Same follow-up pull
            // and rebase already do, and best-effort for the same reason: the
            // fast-forward itself has already landed.
            followed = submodule::follow_and_report(repo, repo_path, "fast-forward");
        } else {
            // Not checked out: a pure ref move. The working tree and HEAD are
            // untouched, so the user stays on their current branch.
            let refname = branch_ref
                .get()
                .name()
                .ok()
                .map(str::to_string)
                .ok_or_else(|| AppError::Other("could not read the branch reference".into()))?;
            repo.reference(&refname, target_oid, true, "fast-forward")?;
        }

        Ok(RefMove {
            branch: branch.trim().to_string(),
            previous_sha,
            stashed: false,
            submodules: followed,
        })
    }
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
        let repo_path = open.path.to_string_lossy().into_owned();
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
        follow_switched_pins(&repo, &repo_path);
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
            // Reword keeps the original author. The committer is whoever is
            // rewording now, which is also who any signature belongs to.
            let identity = CommitIdentity {
                author: head.author().to_owned(),
                committer: repo.signature()?,
            };
            let new_oid = commit_write::amend_head(
                &repo,
                &commit_write::workdir_of(&repo),
                &head,
                &identity,
                message,
                &head.tree()?,
            )?;
            return Ok(new_oid.to_string());
        }

        // Older commit: rebuild it with the new message and rebuild every commit
        // above it on top. Only messages change, so every tree is reused exactly
        // as it was -- no conflict is possible and the working tree and index are
        // never touched, which means pending changes are fine. The new commits
        // are built first and the branch ref moves last (a soft reset), so a
        // failure partway leaves the branch untouched.
        let target = repo.find_commit(target_oid)?;
        if target.parent_count() != 1 {
            return Err(AppError::Other(
                "this is a merge commit, and its message can't be edited yet".into(),
            ));
        }
        let parent = target.parent(0)?;

        let to_replay = history::collect_span_above(
            &repo,
            target_oid,
            "there is a merge commit above this one; can't edit it safely",
            "that commit is not on the current branch",
        )?;

        // Rebuild the target with the new message, same tree, parent, and author.
        let signature = repo.signature()?;
        let identity = CommitIdentity {
            author: target.author().to_owned(),
            committer: signature.clone(),
        };
        let new_target_oid = commit_write::create(
            &repo,
            &commit_write::workdir_of(&repo),
            None,
            &identity,
            message,
            &target.tree()?,
            &[&parent],
        )?;
        let reworded_sha = new_target_oid.to_string();

        // Nothing above the target means it was the branch tip's parent chain
        // end -- the rebuilt commit is itself the new tip.
        let new_tip = if to_replay.is_empty() {
            repo.find_commit(new_target_oid)?
        } else {
            history::rechain_keeping_trees(
                &repo,
                &to_replay,
                Some(repo.find_commit(new_target_oid)?),
                &signature,
                |_| None,
            )?
        };

        repo.reset(new_tip.as_object(), ResetType::Soft, None)?;

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
            return Ok(MergeResult {
                up_to_date: false,
                fast_forwarded: false,
                conflicts,
            });
        }

        // Clean revert: commit the inverse as a single-parent commit.
        let mut index = repo.index()?;
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;
        let signature = repo.signature()?;
        let head_commit = repo.head()?.peel_to_commit()?;
        let summary = commit.summary().ok().flatten().unwrap_or("commit");
        let message = format!("Revert \"{summary}\"\n\nThis reverts commit {oid}.");
        // A revert is authored by whoever is reverting, not by the original author.
        let identity = CommitIdentity {
            author: signature.clone(),
            committer: signature,
        };
        commit_write::create(
            &repo,
            &commit_write::workdir_of(&repo),
            Some("HEAD"),
            &identity,
            &message,
            &tree,
            &[&head_commit],
        )?;
        repo.cleanup_state()?;

        Ok(MergeResult {
            up_to_date: false,
            fast_forwarded: false,
            conflicts: Vec::new(),
        })
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

        let to_replay = history::collect_span_above(
            &repo,
            target_oid,
            "there is a merge commit above this one; can't drop it safely",
            "that commit is not on the current branch",
        )?;

        let signature = repo.signature()?;
        let new_tip = history::replay_onto(
            &repo,
            &to_replay,
            parent,
            &signature,
            &previous_sha,
            "dropping this commit causes conflicts in a later commit; nothing was changed",
        )?;

        // Point the branch at the rebuilt tip and sync the working tree, including
        // any submodule a replayed commit pins somewhere else.
        repo.reset(
            new_tip.as_object(),
            ResetType::Hard,
            Some(CheckoutBuilder::new().force()),
        )?;
        submodule::sync_submodule_workdirs(&repo);

        Ok(RefMove {
            branch,
            previous_sha,
            stashed: false,
            submodules: Vec::new(),
        })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Combine several commits into one, replaying the rest of the stretch on
/// top. See `git::history::squash_commits` for the algorithm; it lives there
/// so the rewrite logic is testable against scratch repos.
#[tauri::command]
#[specta::specta]
pub async fn squash_commits(
    manager: State<'_, RepoManager>,
    repo_id: String,
    shas: Vec<String>,
    message: String,
) -> Result<RefMove, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        crate::git::history::squash_commits(&repo, &shas, &message)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Drop several commits from the current branch in one rewrite. See
/// `git::history::drop_commits` for the algorithm.
#[tauri::command]
#[specta::specta]
pub async fn drop_commits(
    manager: State<'_, RepoManager>,
    repo_id: String,
    shas: Vec<String>,
) -> Result<RefMove, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        crate::git::history::drop_commits(&repo, &shas)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Add a prefix to the front of several commits' messages in one rewrite. See
/// `git::history::prefix_commits` for the algorithm; message-only, so it is
/// safe over pending changes.
#[tauri::command]
#[specta::specta]
pub async fn prefix_commits(
    manager: State<'_, RepoManager>,
    repo_id: String,
    shas: Vec<String>,
    prefix: String,
) -> Result<RefMove, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        crate::git::history::prefix_commits(&repo, &shas, &prefix)
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

/// Which of `shas` this clone actually has an object for.
///
/// Pull request commits come from the host, so the sha is known before the
/// object is: a pull request that has never been fetched -- or one from a fork
/// -- names commits this repository cannot open. Asking first lets the UI offer
/// to fetch rather than opening a commit view that errors.
///
/// Batched because a pull request asks about every commit it lists at once, and
/// one lock and one walk beats a round trip per commit.
#[tauri::command]
#[specta::specta]
pub async fn commits_present(
    manager: State<'_, RepoManager>,
    repo_id: String,
    shas: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        Ok(shas
            .into_iter()
            .filter(|sha| {
                // An unparseable sha is simply absent rather than an error: the list
                // comes from the host, and one odd entry should not fail the batch.
                Oid::from_str(sha.trim())
                    .ok()
                    .is_some_and(|oid| repo.find_commit(oid).is_ok())
            })
            .collect())
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
        let Ok(url) = remote.url() else {
            return Ok(None);
        };
        let Some(parsed) = remote_url::parse(url) else {
            return Ok(None);
        };

        // Only link commits that are reachable from a remote-tracking branch;
        // otherwise the host has never seen the sha and the URL would 404.
        let oid = Oid::from_str(sha.trim()).map_err(AppError::Git)?;
        if !commit_on_any_remote(&repo, oid)? {
            return Ok(None);
        }

        // Route per provider: GitLab uses /-/commit/, Bitbucket /commits/. A host
        // with no known route returns None rather than a guessed URL that 404s.
        Ok(parsed.commit_url(&oid.to_string()))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
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

#[cfg(test)]
mod tests {
    use std::fs;

    use git2::{Repository, Signature};

    use super::*;

    fn commit_file(repo: &Repository, name: &str, contents: &str, message: &str) -> Oid {
        let workdir = repo.workdir().expect("workdir");
        fs::write(workdir.join(name), contents).expect("write fixture");
        let mut index = repo.index().expect("index");
        index
            .add_path(std::path::Path::new(name))
            .expect("add fixture");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("tree id");
        let tree = repo.find_tree(tree_id).expect("tree");
        let signature = Signature::now("Branch Test", "branch@example.com").expect("signature");
        let parents = repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok())
            .into_iter()
            .collect::<Vec<_>>();
        let parent_refs = parents.iter().collect::<Vec<_>>();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .expect("commit")
    }

    fn test_repo() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().expect("temp repo");
        let repo = Repository::init(dir.path()).expect("repo");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Branch Test").expect("name");
        config
            .set_str("user.email", "branch@example.com")
            .expect("email");
        (dir, repo)
    }

    #[test]
    fn hard_reset_over_dirty_tree_stashes_and_moves() {
        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a", "base");
        commit_file(&repo, "a.txt", "b", "second");
        fs::write(dir.path().join("a.txt"), "uncommitted work").expect("dirty tree");

        let result = reset_current_to_commit(&mut repo, base, ResetMode::Hard).expect("reset");

        assert!(result.stashed, "dirty tree must be set aside, not refused");
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), base);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "a",
            "tree must land clean at the target commit"
        );
        let mut stashes = 0;
        repo.stash_foreach(|_, _, _| {
            stashes += 1;
            true
        })
        .expect("stash list");
        assert_eq!(stashes, 1, "the uncommitted work must be recoverable");
    }

    /// A parent repo with a submodule, plus a second branch `ahead` that bumps
    /// the pin. Leaves you on the trailing branch with the nested checkout at the
    /// OLD pin -- the state every "move the working tree" operation starts from.
    ///
    /// Returns (tempdir, parent path, submodule path, trailing branch name, the
    /// commit `ahead` pins the submodule at, the sha `ahead` points to).
    struct PinFixture {
        _dir: tempfile::TempDir,
        parent: String,
        sub: String,
        base_branch: String,
        bumped: String,
        ahead_sha: String,
    }

    fn parent_with_bumped_pin() -> PinFixture {
        use crate::git::shell::run_git;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |at: &str, args: &[&str]| run_git(Some(at), args).unwrap();
        let commit_at = |at: &str, msg: &str| {
            git(
                at,
                &[
                    "-c",
                    "user.name=T",
                    "-c",
                    "user.email=t@e.com",
                    "commit",
                    "-q",
                    "-m",
                    msg,
                ],
            );
        };

        // A submodule source with two commits, so the pin has somewhere to move to.
        let sub_src = root.join("sub-src").to_string_lossy().into_owned();
        std::fs::create_dir_all(&sub_src).unwrap();
        git(&sub_src, &["init", "-q"]);
        std::fs::write(root.join("sub-src/f.txt"), "v1").unwrap();
        git(&sub_src, &["add", "."]);
        commit_at(&sub_src, "v1");

        let parent = root.join("parent").to_string_lossy().into_owned();
        std::fs::create_dir_all(&parent).unwrap();
        git(&parent, &["init", "-q"]);
        std::fs::write(root.join("parent/a.txt"), "one").unwrap();
        git(&parent, &["add", "."]);
        commit_at(&parent, "base");
        // file:// submodules are refused by default since the 2022 CVE fixes.
        git(
            &parent,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                "--",
                &sub_src,
                "sub",
            ],
        );
        commit_at(&parent, "add sub");

        // Whatever `init` named the first branch -- init.defaultBranch varies by
        // machine, so never hardcode `main` here.
        let base_branch = git(&parent, &["rev-parse", "--abbrev-ref", "HEAD"])
            .stdout
            .trim()
            .to_string();

        // A second branch that bumps the pin -- stands in for origin/main sitting
        // ahead of you with a submodule bump in it.
        git(&parent, &["branch", "ahead"]);
        git(&parent, &["checkout", "-q", "ahead"]);
        let sub = root.join("parent/sub").to_string_lossy().into_owned();
        std::fs::write(root.join("parent/sub/f.txt"), "v2").unwrap();
        git(&sub, &["add", "."]);
        commit_at(&sub, "v2");
        let bumped = git(&sub, &["rev-parse", "HEAD"]).stdout.trim().to_string();
        git(&parent, &["add", "sub"]);
        commit_at(&parent, "bump sub");
        let ahead_sha = git(&parent, &["rev-parse", "HEAD"])
            .stdout
            .trim()
            .to_string();

        // Back on the branch that trails, with the submodule checkout where the OLD
        // pin says it belongs. Switching branches does NOT move a nested checkout,
        // so strand it explicitly -- that is the real starting state.
        git(&parent, &["checkout", "-q", &base_branch]);
        git(&sub, &["checkout", "-q", "HEAD~1"]);
        assert_ne!(
            git(&sub, &["rev-parse", "HEAD"]).stdout.trim(),
            bumped,
            "setup: the submodule must start behind the pin being moved to"
        );

        PinFixture {
            _dir: dir,
            parent,
            sub,
            base_branch,
            bumped,
            ahead_sha,
        }
    }

    /// Assert the nested checkout followed the pin and left no phantom change.
    fn assert_pin_followed(f: &PinFixture) {
        use crate::git::shell::run_git;
        let head = run_git(Some(&f.sub), &["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        assert_eq!(
            head, f.bumped,
            "the nested checkout must follow the pin that was landed"
        );
        let repo = git2::Repository::open(&f.parent).unwrap();
        assert!(
            crate::git::submodule::moved_submodules(&repo).is_empty(),
            "no phantom pending change should be left behind"
        );
    }

    /// Regression: fast-forwarding onto commits that move a submodule pin wrote
    /// the new pointer into the parent's index but left the nested checkout
    /// behind, so the user was handed a "pending change" asking to roll the pin
    /// BACK -- a change they never made, pointing the wrong way.
    #[test]
    fn fast_forward_moves_the_submodule_checkout_too() {
        let f = parent_with_bumped_pin();
        let repo = git2::Repository::open(&f.parent).unwrap();

        fast_forward_branch_to(&repo, &f.parent, &f.base_branch, "ahead").expect("fast-forward");

        assert_eq!(
            repo.head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .id()
                .to_string(),
            f.ahead_sha
        );
        assert_pin_followed(&f);
    }

    /// Same bug via "Go to this commit", which hard-resets onto the target. The
    /// reset rewrites the working tree but not the nested checkout.
    #[test]
    fn going_to_a_commit_moves_the_submodule_checkout_too() {
        let f = parent_with_bumped_pin();
        let mut repo = git2::Repository::open(&f.parent).unwrap();
        let target = git2::Oid::from_str(&f.ahead_sha).unwrap();

        move_current_branch_to_commit(&mut repo, target).expect("move");

        assert_eq!(
            repo.head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .id()
                .to_string(),
            f.ahead_sha
        );
        assert_pin_followed(&f);
    }

    /// Switching branches has the same shape: git writes the new gitlink into
    /// the parent's index but leaves the nested checkout on its old commit.
    #[test]
    fn switching_branches_moves_the_submodule_checkout_too() {
        let f = parent_with_bumped_pin();
        let repo = git2::Repository::open(&f.parent).unwrap();

        switch_to(&repo, "ahead").expect("switch");
        follow_switched_pins(&repo, &f.parent);

        assert_eq!(
            repo.head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .id()
                .to_string(),
            f.ahead_sha
        );
        assert_pin_followed(&f);
    }

    /// Checking out a commit directly (detached) moves the tree the same way, so
    /// it needs the same follow-up.
    #[test]
    fn checking_out_a_commit_moves_the_submodule_checkout_too() {
        let f = parent_with_bumped_pin();
        let repo = git2::Repository::open(&f.parent).unwrap();
        let oid = git2::Oid::from_str(&f.ahead_sha).unwrap();

        let object = repo.find_object(oid, None).unwrap();
        repo.checkout_tree(&object, Some(CheckoutBuilder::new().safe()))
            .unwrap();
        repo.set_head_detached(oid).unwrap();
        follow_switched_pins(&repo, &f.parent);

        assert_pin_followed(&f);
    }

    /// And via the hard-reset form ("Undo the commits after this one" -> erase),
    /// which shares `reset_current_to_commit` with every other reset mode.
    #[test]
    fn a_hard_reset_moves_the_submodule_checkout_too() {
        let f = parent_with_bumped_pin();
        let mut repo = git2::Repository::open(&f.parent).unwrap();
        let target = git2::Oid::from_str(&f.ahead_sha).unwrap();

        reset_current_to_commit(&mut repo, target, ResetMode::Hard).expect("reset");

        assert_eq!(
            repo.head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .id()
                .to_string(),
            f.ahead_sha
        );
        assert_pin_followed(&f);
    }

    /// Regression: this used to run a *soft* reset, which moved the branch ref
    /// but left the index at the old tip -- so going back to a commit with a
    /// clean tree handed the user a pile of staged changes they never made.
    #[test]
    fn moving_the_branch_back_leaves_nothing_staged() {
        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a", "base");
        commit_file(&repo, "a.txt", "b", "second");
        commit_file(&repo, "later.txt", "later", "third");

        let result = move_current_branch_to_commit(&mut repo, base).expect("move");

        assert!(!result.stashed, "a clean tree needs no stash");
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), base);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "a",
            "files must land as they were at the target commit"
        );
        assert!(
            !dir.path().join("later.txt").exists(),
            "later files must be gone"
        );
        let statuses = repo.statuses(None).expect("status");
        assert!(
            statuses.is_empty(),
            "moving back must leave a clean tree, not staged changes: {:?}",
            statuses
                .iter()
                .map(|s| (s.path().map(str::to_owned), s.status()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn hard_reset_over_clean_tree_reports_no_stash() {
        let (_dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a", "base");
        commit_file(&repo, "a.txt", "b", "second");

        let result = reset_current_to_commit(&mut repo, base, ResetMode::Hard).expect("reset");

        assert!(!result.stashed, "a clean tree needs no stash");
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), base);
    }
}
