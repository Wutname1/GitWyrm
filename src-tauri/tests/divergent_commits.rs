//! The commit ladder behind the sync modal's tree preview. The preview tells
//! users which commits a force-push would destroy, so the two arms must stay
//! exactly right -- an over-reported arm would strike through commits that are
//! actually safe, and an under-reported one would hide real data loss.

use std::fs;
use std::path::PathBuf;

use git2::{Repository, Signature};
use gitwyrm_lib::git_refs::divergence_between;

fn scratch_repo(label: &str) -> (PathBuf, Repository) {
  let dir =
    std::env::temp_dir().join(format!("gitwyrm-test-divergent-{}-{label}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  let repo = Repository::init(&dir).unwrap();
  {
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test Wyrm").unwrap();
    config.set_str("user.email", "test@gitwyrm.dev").unwrap();
  }
  (dir, repo)
}

fn sig() -> Signature<'static> {
  Signature::now("Test Wyrm", "test@gitwyrm.dev").unwrap()
}

fn commit_file(dir: &PathBuf, repo: &Repository, file: &str, content: &str, msg: &str) -> git2::Oid {
  fs::write(dir.join(file), content).unwrap();
  let mut index = repo.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.write().unwrap();
  let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
  let parents = match repo.head() {
    Ok(h) => vec![h.peel_to_commit().unwrap()],
    Err(_) => vec![],
  };
  let parent_refs: Vec<_> = parents.iter().collect();
  repo.commit(Some("HEAD"), &sig(), &sig(), msg, &tree, &parent_refs).unwrap()
}

/// The shape that drove this work: HearthShelf sitting ahead 1, behind 4.
/// Each arm must contain only that side's own commits -- never the shared base.
#[test]
fn splits_a_two_sided_divergence_into_its_two_arms() {
  let (dir, repo) = scratch_repo("two-sided");
  commit_file(&dir, &repo, "base.txt", "base", "base commit");
  let base = repo.head().unwrap().peel_to_commit().unwrap().id();

  // Their side: four commits on top of the base.
  for n in 1..=4 {
    commit_file(&dir, &repo, "theirs.txt", &format!("v{n}"), &format!("theirs {n}"));
  }
  let theirs = repo.head().unwrap().peel_to_commit().unwrap().id();

  // Our side: one commit from the same base.
  repo.branch("mine", &repo.find_commit(base).unwrap(), false).unwrap();
  repo.set_head("refs/heads/mine").unwrap();
  repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
  commit_file(&dir, &repo, "mine.txt", "mine", "mine 1");
  let ours = repo.head().unwrap().peel_to_commit().unwrap().id();

  let d = divergence_between(&repo, ours, theirs, 10).unwrap();

  assert_eq!(d.ours.len(), 1, "only our own commit is on our arm");
  assert_eq!(d.ours[0].summary, "mine 1");
  assert_eq!(d.theirs.len(), 4, "all four of theirs, and not the shared base");
  assert_eq!(d.theirs[0].summary, "theirs 4", "newest first, matching the graph");
  assert_eq!(d.theirs[3].summary, "theirs 1");
  assert!(!d.truncated);
}

/// A pure fast-forward has nothing on our side, so the preview must show no
/// commits at risk -- this is the case that should read as completely safe.
#[test]
fn reports_an_empty_arm_when_only_one_side_moved() {
  let (dir, repo) = scratch_repo("ff");
  commit_file(&dir, &repo, "base.txt", "base", "base commit");
  let ours = repo.head().unwrap().peel_to_commit().unwrap().id();
  commit_file(&dir, &repo, "next.txt", "next", "their new work");
  let theirs = repo.head().unwrap().peel_to_commit().unwrap().id();

  let d = divergence_between(&repo, ours, theirs, 10).unwrap();

  assert!(d.ours.is_empty(), "nothing of ours is at risk in a fast-forward");
  assert_eq!(d.theirs.len(), 1);
  assert_eq!(d.theirs[0].summary, "their new work");
}

/// Identical refs mean there is nothing to draw at all.
#[test]
fn reports_both_arms_empty_when_the_refs_match() {
  let (dir, repo) = scratch_repo("same");
  commit_file(&dir, &repo, "base.txt", "base", "base commit");
  let tip = repo.head().unwrap().peel_to_commit().unwrap().id();

  let d = divergence_between(&repo, tip, tip, 10).unwrap();

  assert!(d.ours.is_empty());
  assert!(d.theirs.is_empty());
}

/// The preview draws a handful of rows, so long arms are capped -- but the cap
/// must announce itself rather than silently pretending the rest do not exist.
#[test]
fn flags_truncation_when_an_arm_exceeds_the_limit() {
  let (dir, repo) = scratch_repo("limit");
  commit_file(&dir, &repo, "base.txt", "base", "base commit");
  let ours = repo.head().unwrap().peel_to_commit().unwrap().id();
  for n in 1..=5 {
    commit_file(&dir, &repo, "theirs.txt", &format!("v{n}"), &format!("theirs {n}"));
  }
  let theirs = repo.head().unwrap().peel_to_commit().unwrap().id();

  let d = divergence_between(&repo, ours, theirs, 2).unwrap();

  assert_eq!(d.theirs.len(), 2, "capped at the limit");
  assert!(d.truncated, "and says so, so the UI can show 'and older changes'");
}
