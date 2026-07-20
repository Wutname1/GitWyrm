//! Push/pull report their outcome from the branch's ahead/behind against its
//! upstream, measured before and after. These check the load-bearing
//! assumption: a long-lived git2 handle must observe ref updates made by the
//! shell `git` process, otherwise the "after" read is stale and every push
//! would report zero commits moved.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git2::{BranchType, Repository};

fn git(dir: &Path, args: &[&str]) {
  let status = Command::new("git").current_dir(dir).args(args).status().unwrap();
  assert!(status.success(), "git {args:?} failed");
}

/// Ahead/behind of the current branch vs its upstream, matching the calculation
/// in `commands::remote::tracking_state`.
fn ahead_behind(repo: &Repository) -> (u32, u32) {
  let head = repo.head().unwrap();
  let name = head.shorthand().unwrap().to_string();
  let branch = repo.find_branch(&name, BranchType::Local).unwrap();
  let upstream = branch.upstream().unwrap();
  repo
    .graph_ahead_behind(branch.get().target().unwrap(), upstream.get().target().unwrap())
    .map(|(a, b)| (a as u32, b as u32))
    .unwrap()
}

/// A work repo tracking a bare upstream, with one commit already pushed.
/// Returns the root (for cleanup) and the working tree path.
fn scratch(tag: &str) -> (PathBuf, PathBuf) {
  let root = std::env::temp_dir().join(format!("gitwyrm-push-{}-{}", tag, std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let remote = root.join("remote.git");
  let work = root.join("work");
  fs::create_dir_all(&work).unwrap();

  Command::new("git").args(["init", "-q", "--bare"]).arg(&remote).status().unwrap();
  git(&work, &["init", "-q", "-b", "main"]);
  git(&work, &["config", "user.email", "test@gitwyrm.dev"]);
  git(&work, &["config", "user.name", "Test Wyrm"]);
  fs::write(work.join("f.txt"), "one\n").unwrap();
  git(&work, &["add", "."]);
  git(&work, &["commit", "-qm", "one"]);
  git(&work, &["remote", "add", "origin", remote.to_str().unwrap()]);
  git(&work, &["push", "-q", "-u", "origin", "main"]);

  (root, work)
}

#[test]
fn push_of_new_commits_is_observed_by_a_long_lived_handle() {
  let (root, work) = scratch("newcommits");

  // Opened before the push and held across it, as RepoManager does.
  let repo = Repository::open(&work).unwrap();
  assert_eq!(ahead_behind(&repo), (0, 0), "fresh checkout should be in sync");

  fs::write(work.join("f.txt"), "two\n").unwrap();
  git(&work, &["commit", "-qam", "two"]);
  fs::write(work.join("f.txt"), "three\n").unwrap();
  git(&work, &["commit", "-qam", "three"]);

  let (before, _) = ahead_behind(&repo);
  assert_eq!(before, 2, "two local commits should read as ahead by 2");

  git(&work, &["push", "-q"]);

  let (after, _) = ahead_behind(&repo);
  assert_eq!(after, 0, "handle must see the pushed ref, not a cached value");
  assert_eq!(before - after, 2, "reported push count should be 2");

  let _ = fs::remove_dir_all(&root);
}

#[test]
fn up_to_date_push_reports_nothing_moved() {
  let (root, work) = scratch("uptodate");
  let repo = Repository::open(&work).unwrap();

  let (before, _) = ahead_behind(&repo);
  git(&work, &["push", "-q"]);
  let (after, _) = ahead_behind(&repo);

  // The reported bug: this path must yield 0 so the toast says nothing moved
  // instead of claiming a push happened.
  assert_eq!(before, 0);
  assert_eq!(before.saturating_sub(after), 0, "no-op push must report zero commits");

  let _ = fs::remove_dir_all(&root);
}

/// Ahead/behind for a named branch, matching `branch_tracking_state`. Returns
/// `None` when the branch has no upstream, which is the never-pushed case.
fn branch_ahead_behind(repo: &Repository, name: &str) -> Option<(u32, u32)> {
  let branch = repo.find_branch(name, BranchType::Local).unwrap();
  let upstream = branch.upstream().ok()?;
  repo
    .graph_ahead_behind(branch.get().target().unwrap(), upstream.get().target().unwrap())
    .map(|(a, b)| (a as u32, b as u32))
    .ok()
}

/// Commits unique to `branch` against every other remote-tracking ref,
/// mirroring `commands::remote::published_count`.
fn published_count(repo: &Repository, branch: &str) -> u32 {
  let local = repo.find_branch(branch, BranchType::Local).unwrap();
  let tip = local.get().target().unwrap();
  let upstream = local.upstream().unwrap().get().target().unwrap();

  let mut walk = repo.revwalk().unwrap();
  walk.push(tip).unwrap();
  for (remote_branch, _) in repo.branches(Some(BranchType::Remote)).unwrap().flatten() {
    if let Some(oid) = remote_branch.get().target() {
      if oid != upstream {
        let _ = walk.hide(oid);
      }
    }
  }
  walk.count() as u32
}

#[test]
fn push_of_a_branch_that_is_not_checked_out_reports_its_own_count() {
  let (root, work) = scratch("otherbranch");
  let repo = Repository::open(&work).unwrap();

  // A tracked branch two commits ahead, with main checked out instead.
  git(&work, &["checkout", "-qb", "feature"]);
  git(&work, &["push", "-q", "-u", "origin", "feature"]);
  fs::write(work.join("b.txt"), "b\n").unwrap();
  git(&work, &["add", "."]);
  git(&work, &["commit", "-qm", "b"]);
  fs::write(work.join("c.txt"), "c\n").unwrap();
  git(&work, &["add", "."]);
  git(&work, &["commit", "-qm", "c"]);
  git(&work, &["checkout", "-q", "main"]);

  let (before, _) = branch_ahead_behind(&repo, "feature").unwrap();
  assert_eq!(before, 2, "feature should read as ahead by 2 while main is checked out");

  // The remote+refspec form the command uses. Naming the remote is what makes
  // this work: `git push -- <branch>` reads the branch as a repository name.
  git(&work, &["push", "-q", "--progress", "origin", "refs/heads/feature"]);

  let (after, _) = branch_ahead_behind(&repo, "feature").unwrap();
  assert_eq!(after, 0, "feature must be in sync after its own push");
  assert_eq!(before - after, 2, "reported push count should be 2");
  assert_eq!(
    repo.head().unwrap().shorthand().unwrap(),
    "main",
    "pushing another branch must not move HEAD"
  );

  let _ = fs::remove_dir_all(&root);
}

#[test]
fn publishing_a_never_pushed_branch_sets_upstream_and_counts_its_commits() {
  let (root, work) = scratch("publish");
  let repo = Repository::open(&work).unwrap();

  git(&work, &["checkout", "-qb", "fresh"]);
  for n in ["n1", "n2", "n3"] {
    fs::write(work.join(format!("{n}.txt")), format!("{n}\n")).unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-qm", n]);
  }
  git(&work, &["checkout", "-q", "main"]);

  // No upstream, so there is no ahead count to read beforehand -- the reason
  // the command measures this case after the push instead.
  assert!(branch_ahead_behind(&repo, "fresh").is_none(), "fresh should have no upstream");

  git(&work, &["push", "-q", "--set-upstream", "--progress", "origin", "refs/heads/fresh"]);

  let upstream = repo
    .find_branch("fresh", BranchType::Local)
    .unwrap()
    .upstream()
    .expect("push --set-upstream must leave the branch tracking");
  assert_eq!(upstream.name().unwrap().unwrap(), "origin/fresh");

  assert_eq!(branch_ahead_behind(&repo, "fresh").unwrap(), (0, 0));
  assert_eq!(published_count(&repo, "fresh"), 3, "three commits went to the remote");

  let _ = fs::remove_dir_all(&root);
}

/// A second clone of the same bare remote, used to make commits that arrive
/// from "somewhere else".
fn other_clone(root: &Path, branch: &str) -> PathBuf {
  let other = root.join(format!("other-{branch}"));
  Command::new("git")
    .args(["clone", "-q", "-b", branch])
    .arg(root.join("remote.git"))
    .arg(&other)
    .status()
    .unwrap();
  git(&other, &["config", "user.email", "other@gitwyrm.dev"]);
  git(&other, &["config", "user.name", "Other Wyrm"]);
  other
}

#[test]
fn behind_branch_fast_forwards_without_being_checked_out() {
  let (root, work) = scratch("ffbranch");

  git(&work, &["checkout", "-qb", "feature"]);
  git(&work, &["push", "-q", "-u", "origin", "feature"]);
  git(&work, &["checkout", "-q", "main"]);

  let other = other_clone(&root, "feature");
  fs::write(other.join("remote.txt"), "from elsewhere\n").unwrap();
  git(&other, &["add", "."]);
  git(&other, &["commit", "-qm", "remote work"]);
  git(&other, &["push", "-q"]);

  let repo = Repository::open(&work).unwrap();
  git(&work, &["fetch", "-q"]);
  let (ahead, behind) = branch_ahead_behind(&repo, "feature").unwrap();
  assert_eq!((ahead, behind), (0, 1), "feature should be purely behind");

  // The refspec form the command uses to move a non-HEAD branch.
  git(&work, &["fetch", "--progress", "origin", "feature:feature"]);

  let (_, behind_after) = branch_ahead_behind(&repo, "feature").unwrap();
  assert_eq!(behind_after, 0, "feature should be up to date");
  assert_eq!(behind - behind_after, 1, "should report 1 commit received");
  assert_eq!(
    repo.head().unwrap().shorthand().unwrap(),
    "main",
    "updating another branch must not move HEAD"
  );

  let _ = fs::remove_dir_all(&root);
}

#[test]
fn diverged_branch_refuses_to_fast_forward() {
  let (root, work) = scratch("divergedbranch");

  git(&work, &["checkout", "-qb", "feature"]);
  git(&work, &["push", "-q", "-u", "origin", "feature"]);

  let other = other_clone(&root, "feature");
  fs::write(other.join("remote.txt"), "theirs\n").unwrap();
  git(&other, &["add", "."]);
  git(&other, &["commit", "-qm", "theirs"]);
  git(&other, &["push", "-q"]);

  // Local commit on the same branch, so it is both ahead and behind.
  fs::write(work.join("local.txt"), "ours\n").unwrap();
  git(&work, &["add", "."]);
  git(&work, &["commit", "-qm", "ours"]);
  git(&work, &["checkout", "-q", "main"]);
  git(&work, &["fetch", "-q"]);

  let repo = Repository::open(&work).unwrap();
  let (ahead, behind) = branch_ahead_behind(&repo, "feature").unwrap();
  assert_eq!((ahead, behind), (1, 1), "feature should have diverged");

  // The command refuses this on the ahead count before shelling out; git
  // itself would also reject it, which is the backstop being checked here.
  let status = Command::new("git")
    .current_dir(&work)
    .args(["fetch", "origin", "feature:feature"])
    .status()
    .unwrap();
  assert!(!status.success(), "a non-fast-forward update must be rejected, not silently applied");

  let _ = fs::remove_dir_all(&root);
}

#[test]
fn pruned_upstream_is_distinguishable_from_being_in_sync() {
  let (root, work) = scratch("prunedupstream");

  git(&work, &["checkout", "-qb", "feature"]);
  git(&work, &["push", "-q", "-u", "origin", "feature"]);
  fs::write(work.join("b.txt"), "b\n").unwrap();
  git(&work, &["add", "."]);
  git(&work, &["commit", "-qm", "b"]);
  git(&work, &["push", "-q"]);
  git(&work, &["checkout", "-q", "main"]);

  let repo = Repository::open(&work).unwrap();
  // In sync: upstream resolves, counts are zero.
  assert_eq!(branch_ahead_behind(&repo, "feature"), Some((0, 0)));
  assert!(
    repo.find_branch("origin/feature", BranchType::Remote).is_ok(),
    "upstream ref should resolve while it exists"
  );

  // Someone deletes the branch on the remote; we prune the stale tracking ref.
  let other = other_clone(&root, "main");
  git(&other, &["push", "-q", "origin", "--delete", "feature"]);
  git(&work, &["fetch", "-q", "--prune"]);

  // The config still names the upstream...
  let local = repo.find_branch("feature", BranchType::Local).unwrap();
  assert!(
    local.upstream().is_err(),
    "the tracking ref is gone, so upstream() no longer resolves"
  );
  // ...but the remote-tracking ref is gone. This is the state that used to be
  // indistinguishable from "in sync", making push report zero commits.
  assert!(repo.find_branch("origin/feature", BranchType::Remote).is_err());
  assert_eq!(
    branch_ahead_behind(&repo, "feature"),
    None,
    "a pruned upstream must not report (0, 0) like an in-sync branch"
  );

  let _ = fs::remove_dir_all(&root);
}

#[test]
fn pull_reports_commits_actually_received() {
  let (root, work) = scratch("pull");
  let repo = Repository::open(&work).unwrap();

  // A second clone pushes a commit, so `work` has something real to pull.
  // `-b main` matters: the bare repo's HEAD still points at `master`, so a
  // plain clone would check out the wrong branch and push somewhere `work`
  // isn't tracking.
  let other = root.join("other");
  Command::new("git")
    .args(["clone", "-q", "-b", "main"])
    .arg(root.join("remote.git"))
    .arg(&other)
    .status()
    .unwrap();
  git(&other, &["config", "user.email", "other@gitwyrm.dev"]);
  git(&other, &["config", "user.name", "Other Wyrm"]);
  fs::write(other.join("g.txt"), "remote work\n").unwrap();
  git(&other, &["add", "."]);
  git(&other, &["commit", "-qm", "from other"]);
  git(&other, &["push", "-q"]);

  git(&work, &["fetch", "-q"]);
  let (_, behind_before) = ahead_behind(&repo);
  assert_eq!(behind_before, 1, "should see one incoming commit");

  git(&work, &["pull", "-q", "--ff-only"]);
  let (_, behind_after) = ahead_behind(&repo);

  assert_eq!(behind_after, 0);
  assert_eq!(behind_before - behind_after, 1, "pull should report 1 commit received");

  let _ = fs::remove_dir_all(&root);
}
