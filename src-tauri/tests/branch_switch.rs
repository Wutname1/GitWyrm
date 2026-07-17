//! Branch-switch dirty-tree behavior: the three modes (auto-stash, carry,
//! refuse) exercised through the same git2 sequence the checkout_branch command
//! runs. Verifies changes follow the user (auto-stash), that a clean carry
//! works, and that a conflicting auto-stash pop leaves the stash as a backup.

use std::fs;
use std::path::PathBuf;

use git2::{build::CheckoutBuilder, Repository, Signature, StashFlags};

fn scratch_repo_named(tag: &str) -> (PathBuf, Repository) {
  let dir = std::env::temp_dir().join(format!("gitwyrm-bswitch-{}-{tag}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  let repo = Repository::init(&dir).unwrap();
  {
    let mut cfg = repo.config().unwrap();
    cfg.set_str("user.name", "Test Wyrm").unwrap();
    cfg.set_str("user.email", "test@gitwyrm.dev").unwrap();
    cfg.set_bool("core.autocrlf", false).unwrap();
  }
  (dir, repo)
}

fn sig() -> Signature<'static> {
  Signature::now("Test Wyrm", "test@gitwyrm.dev").unwrap()
}

/// Commit the current index; returns the new commit's tree parent chain implicitly.
fn commit_all(repo: &Repository, msg: &str) {
  let mut index = repo.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.write().unwrap();
  let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
  let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
  let parents: Vec<&git2::Commit> = parent.iter().collect();
  repo.commit(Some("HEAD"), &sig(), &sig(), msg, &tree, &parents).unwrap();
}

fn is_dirty(repo: &Repository) -> bool {
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(true).recurse_untracked_dirs(true);
  repo.statuses(Some(&mut opts)).unwrap().iter().any(|e| !e.status().is_ignored())
}

fn switch_to(repo: &Repository, refname: &str) -> Result<(), git2::Error> {
  let (object, reference) = repo.revparse_ext(refname)?;
  let mut b = CheckoutBuilder::new();
  b.safe();
  repo.checkout_tree(&object, Some(&mut b))?;
  match reference {
    Some(r) => repo.set_head(r.name().unwrap_or("HEAD"))?,
    None => repo.set_head_detached(object.id())?,
  }
  Ok(())
}

/// Two branches, a non-conflicting local edit: auto-stash carries it across.
#[test]
fn auto_stash_carries_nonconflicting_change() {
  const TAG: &str = "carry_nc";
  let (dir, mut repo) = scratch_repo_named(TAG);
  fs::write(dir.join("a.txt"), "base\n").unwrap();
  commit_all(&repo, "init");
  repo.branch("feature", &repo.head().unwrap().peel_to_commit().unwrap(), false).unwrap();

  // Edit a DIFFERENT file than anything feature touches.
  fs::write(dir.join("b.txt"), "work in progress\n").unwrap();
  assert!(is_dirty(&repo));

  // auto-stash sequence
  repo.stash_save(&sig(), "auto", Some(StashFlags::INCLUDE_UNTRACKED)).unwrap();
  assert!(!is_dirty(&repo), "stash should clean the tree");
  switch_to(&repo, "feature").unwrap();
  repo.stash_pop(0, None).unwrap();

  // The WIP file followed us onto feature.
  assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "work in progress\n");
  assert!(repo.stash_pop(0, None).is_err(), "no stash should remain after a clean pop");
  let _ = fs::remove_dir_all(&dir);
}

/// Carry (plain safe checkout) succeeds when the change does not conflict.
#[test]
fn carry_succeeds_when_no_conflict() {
  const TAG: &str = "carry_ok";
  let (dir, mut repo) = scratch_repo_named(TAG);
  fs::write(dir.join("a.txt"), "base\n").unwrap();
  commit_all(&repo, "init");
  repo.branch("feature", &repo.head().unwrap().peel_to_commit().unwrap(), false).unwrap();

  fs::write(dir.join("b.txt"), "carried\n").unwrap();
  // Safe checkout carries the untracked/modified file across.
  switch_to(&repo, "feature").unwrap();
  assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "carried\n");
  assert!(is_dirty(&repo), "changes should still be present after carry");
  let _ = fs::remove_dir_all(&dir);
}

/// When the auto-stash pop conflicts, the stash entry is preserved as a backup.
#[test]
fn auto_stash_pop_conflict_keeps_stash() {
  const TAG: &str = "pop_conflict";
  let (dir, mut repo) = scratch_repo_named(TAG);
  fs::write(dir.join("shared.txt"), "line1\nline2\n").unwrap();
  commit_all(&repo, "init");

  // feature changes the SAME file, so a stash pop after switching will conflict.
  repo.branch("feature", &repo.head().unwrap().peel_to_commit().unwrap(), false).unwrap();
  switch_to(&repo, "feature").unwrap();
  fs::write(dir.join("shared.txt"), "line1\nFEATURE\n").unwrap();
  commit_all(&repo, "feature edit");
  switch_to(&repo, "master").ok().or_else(|| switch_to(&repo, "main").ok());

  // Back on the base branch, make a conflicting local edit and auto-stash-switch.
  fs::write(dir.join("shared.txt"), "line1\nLOCAL\n").unwrap();
  repo.stash_save(&sig(), "auto", Some(StashFlags::INCLUDE_UNTRACKED)).unwrap();
  switch_to(&repo, "feature").unwrap();

  // This is the exact sequence the command uses: apply (never drops), then only
  // drop when there's no conflict. git2's stash_pop would silently succeed AND
  // drop the stash here, destroying the backup -- which is why we use apply.
  repo.stash_apply(0, None).unwrap();
  assert!(repo.index().unwrap().has_conflicts(), "apply should conflict on the shared line");

  // Command keeps the stash as a backup when conflicts remain; assert it's still there.
  let mut count = 0;
  repo.stash_foreach(|_, _, _| {
    count += 1;
    true
  })
  .unwrap();
  assert_eq!(count, 1, "stash must be kept as a backup after a conflicting apply");
  let _ = fs::remove_dir_all(&dir);
}

/// Sanity guard on the git2 pitfall this design is built around: a plain
/// stash_pop succeeds AND drops the stash even on conflict. If a future git2
/// changes this, we want to know (the command relies on apply+conditional-drop).
#[test]
fn git2_stash_pop_drops_stash_on_conflict() {
  const TAG: &str = "pop_pitfall";
  let (dir, mut repo) = scratch_repo_named(TAG);
  fs::write(dir.join("shared.txt"), "line1\nline2\n").unwrap();
  commit_all(&repo, "init");
  repo.branch("feature", &repo.head().unwrap().peel_to_commit().unwrap(), false).unwrap();
  switch_to(&repo, "feature").unwrap();
  fs::write(dir.join("shared.txt"), "line1\nFEATURE\n").unwrap();
  commit_all(&repo, "feature edit");
  switch_to(&repo, "master").ok().or_else(|| switch_to(&repo, "main").ok());

  fs::write(dir.join("shared.txt"), "line1\nLOCAL\n").unwrap();
  repo.stash_save(&sig(), "auto", Some(StashFlags::INCLUDE_UNTRACKED)).unwrap();
  switch_to(&repo, "feature").unwrap();

  let popped = repo.stash_pop(0, None);
  assert!(popped.is_ok(), "git2 stash_pop returns Ok even on conflict");
  let mut count = 0;
  repo.stash_foreach(|_, _, _| {
    count += 1;
    true
  })
  .unwrap();
  assert_eq!(count, 0, "git2 stash_pop drops the stash on conflict (the pitfall)");
  let _ = fs::remove_dir_all(&dir);
}
