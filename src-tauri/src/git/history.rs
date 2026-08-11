//! Linear history rewrites over several commits at once: squash (combine
//! into one) and multi-drop. Both share the same shape as the single-commit
//! reword/drop commands: collect the affected stretch of history, rebuild it
//! by cherry-picking onto a new base, and abort wholesale on any conflict so
//! the branch is never left half-rebased.

use git2::{build::CheckoutBuilder, Oid, ResetType};
use std::collections::HashSet;

use crate::error::AppError;
use crate::git::commit_write::{self, CommitIdentity};
use crate::git::refs;
use crate::git::types::RefMove;

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

/// Walk from HEAD down the current branch until every target sha has been
/// seen, collecting the whole stretch (targets included) oldest-first. Errors
/// if a merge commit sits anywhere in the stretch or a target isn't on the
/// branch, since the replay that follows only handles a linear run.
fn collect_linear_span<'r>(
  repo: &'r git2::Repository,
  targets: &HashSet<Oid>,
) -> Result<Vec<git2::Commit<'r>>, AppError> {
  let head_oid = repo.head()?.peel_to_commit()?.id();
  let mut span = Vec::new();
  let mut remaining = targets.len();
  let mut walk = repo.revwalk()?;
  walk.push(head_oid)?;
  for step in walk {
    let step_oid = step?;
    let c = repo.find_commit(step_oid)?;
    if c.parent_count() > 1 {
      return Err(AppError::Other(
        "there is a merge commit in this stretch of history; can't rewrite it safely".into(),
      ));
    }
    let is_target = targets.contains(&step_oid);
    span.push(c);
    if is_target {
      remaining -= 1;
      if remaining == 0 {
        break;
      }
    }
  }
  if remaining > 0 {
    return Err(AppError::Other(
      "some of those commits are not on the current branch".into(),
    ));
  }
  span.reverse();
  Ok(span)
}

/// Walk from HEAD down to (but NOT including) `target`, oldest-first. This is
/// the single-target shape used by the reword and drop-one commands, where the
/// target itself is replaced rather than replayed.
pub fn collect_span_above<'r>(
  repo: &'r git2::Repository,
  target_oid: Oid,
  merge_msg: &str,
  missing_msg: &str,
) -> Result<Vec<git2::Commit<'r>>, AppError> {
  let head_oid = repo.head()?.peel_to_commit()?.id();
  let mut span = Vec::new();
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
      return Err(AppError::Other(merge_msg.into()));
    }
    span.push(c);
  }
  if !found {
    return Err(AppError::Other(missing_msg.into()));
  }
  span.reverse();
  Ok(span)
}

/// Hard-reset the branch back to `previous_sha`, undoing a partial replay.
fn rewind_to(repo: &git2::Repository, previous_sha: &str) -> Result<(), AppError> {
  let start_oid = Oid::from_str(previous_sha).map_err(AppError::Git)?;
  let head_obj = repo.find_object(start_oid, None)?;
  repo.reset(&head_obj, ResetType::Hard, Some(CheckoutBuilder::new().force()))?;
  Ok(())
}

/// Cherry-pick `commits` (oldest-first) one by one onto `new_tip`.
///
/// ANY failure - a conflict or an I/O error partway through - rewinds the branch
/// to `previous_sha` before returning, so a rewrite is all-or-nothing. Only the
/// conflict case gets `conflict_msg`; other failures keep their own error so the
/// cause isn't hidden behind a generic message.
pub fn replay_onto<'r>(
  repo: &'r git2::Repository,
  commits: &[git2::Commit<'r>],
  new_tip: git2::Commit<'r>,
  signature: &git2::Signature<'_>,
  previous_sha: &str,
  conflict_msg: &str,
) -> Result<git2::Commit<'r>, AppError> {
  match replay_steps(repo, commits, new_tip, signature, conflict_msg) {
    Ok(tip) => Ok(tip),
    Err(e) => {
      // Best-effort rewind: report the original failure even if it also fails.
      let _ = rewind_to(repo, previous_sha);
      Err(e)
    }
  }
}

/// The replay itself, with no cleanup. `replay_onto` owns the rewind so every
/// early exit here is covered by exactly one recovery path.
fn replay_steps<'r>(
  repo: &'r git2::Repository,
  commits: &[git2::Commit<'r>],
  mut new_tip: git2::Commit<'r>,
  signature: &git2::Signature<'_>,
  conflict_msg: &str,
) -> Result<git2::Commit<'r>, AppError> {
  for commit in commits {
    let mut index = repo.cherrypick_commit(commit, &new_tip, 0, None).map_err(AppError::Git)?;
    if index.has_conflicts() {
      return Err(AppError::Other(conflict_msg.into()));
    }
    let tree_oid = index.write_tree_to(repo)?;
    let tree = repo.find_tree(tree_oid)?;
    let message = commit.message().unwrap_or("");
    // A replayed commit keeps its original author and takes the rewriter as
    // committer, so a signature here is the rewriter's, over the new commit.
    let identity = CommitIdentity {
      author: commit.author().to_owned(),
      committer: signature.to_owned(),
    };
    let new_oid = commit_write::create(
      repo,
      &commit_write::workdir_of(repo),
      None,
      &identity,
      message,
      &tree,
      &[&new_tip],
    )?;
    new_tip = repo.find_commit(new_oid)?;
  }
  Ok(new_tip)
}

/// Shared guards for the multi-commit rewrites: a real branch checked out, a
/// clean tree, and the selected shas parsed. Returns (branch, previous head
/// sha, target oid set).
fn prepare_rewrite(
  repo: &git2::Repository,
  shas: &[String],
) -> Result<(String, String, HashSet<Oid>), AppError> {
  let branch = current_branch_name(repo)?;
  if refs::tracked_changes_present(repo)? {
    return Err(AppError::Other(
      "working tree has changes; commit or stash before rewriting history".into(),
    ));
  }
  let mut targets = HashSet::new();
  for sha in shas {
    targets.insert(Oid::from_str(sha.trim()).map_err(AppError::Git)?);
  }
  let previous_sha = repo.head()?.peel_to_commit()?.id().to_string();
  Ok((branch, previous_sha, targets))
}

/// Combine several commits into one. The selected commits are applied in order
/// onto the oldest one's parent and committed once with `message`; any other
/// commits in the same stretch of history (between and above the selected
/// ones) are replayed on top, keeping their own messages and authors. Only
/// works on a linear stretch; any conflict aborts and restores the branch.
pub fn squash_commits(
  repo: &git2::Repository,
  shas: &[String],
  message: &str,
) -> Result<RefMove, AppError> {
  if shas.len() < 2 {
    return Err(AppError::Other("select at least two commits to combine".into()));
  }
  let message = message.trim();
  if message.is_empty() {
    return Err(AppError::Other("a commit message is required".into()));
  }

  let (branch, previous_sha, targets) = prepare_rewrite(repo, shas)?;
  let span = collect_linear_span(repo, &targets)?;

  // The span starts at the oldest selected commit; the combined commit takes
  // its parent as its own.
  let oldest = &span[0];
  if oldest.parent_count() != 1 {
    return Err(AppError::Other(
      "the very first commit in the repository can't be combined".into(),
    ));
  }
  let base = oldest.parent(0)?;
  let signature = repo.signature()?;

  let (selected, rest): (Vec<_>, Vec<_>) =
    span.into_iter().partition(|c| targets.contains(&c.id()));

  // Apply the selected commits in order onto the base to build the combined
  // tree, then record it as one commit. The intermediate commits are only
  // stepping stones and end up unreferenced.
  let author = selected[0].author();
  let combined_tip = replay_onto(
    repo,
    &selected,
    repo.find_commit(base.id())?,
    &signature,
    &previous_sha,
    "these commits can't be combined cleanly; nothing was changed",
  )?;
  let identity = CommitIdentity {
    author: author.to_owned(),
    committer: signature.to_owned(),
  };
  let squash_oid = commit_write::create(
    repo,
    &commit_write::workdir_of(repo),
    None,
    &identity,
    message,
    &combined_tip.tree()?,
    &[&base],
  )?;
  let squash_tip = repo.find_commit(squash_oid)?;

  // Replay everything else in the stretch on top of the combined commit.
  let new_tip = replay_onto(
    repo,
    &rest,
    squash_tip,
    &signature,
    &previous_sha,
    "combining these commits causes conflicts in a later commit; nothing was changed",
  )?;

  repo.reset(new_tip.as_object(), ResetType::Hard, Some(CheckoutBuilder::new().force()))?;
  crate::git::submodule::sync_submodule_workdirs(repo);
  Ok(RefMove { branch, previous_sha, stashed: false })
}

/// Drop several commits from the current branch in one rewrite: replay the
/// stretch of history from the oldest selected commit's parent up to HEAD,
/// skipping every selected commit. Only works on a linear stretch; any
/// conflict aborts and restores the branch.
pub fn drop_commits(repo: &git2::Repository, shas: &[String]) -> Result<RefMove, AppError> {
  if shas.is_empty() {
    return Err(AppError::Other("select at least one commit to drop".into()));
  }

  let (branch, previous_sha, targets) = prepare_rewrite(repo, shas)?;
  let span = collect_linear_span(repo, &targets)?;

  let oldest = &span[0];
  if oldest.parent_count() != 1 {
    return Err(AppError::Other(
      "the very first commit in the repository can't be dropped".into(),
    ));
  }
  let base = oldest.parent(0)?;
  let signature = repo.signature()?;

  let rest: Vec<_> = span.into_iter().filter(|c| !targets.contains(&c.id())).collect();

  let new_tip = replay_onto(
    repo,
    &rest,
    base,
    &signature,
    &previous_sha,
    "dropping these commits causes conflicts in a later commit; nothing was changed",
  )?;

  repo.reset(new_tip.as_object(), ResetType::Hard, Some(CheckoutBuilder::new().force()))?;
  crate::git::submodule::sync_submodule_workdirs(repo);
  Ok(RefMove { branch, previous_sha, stashed: false })
}
