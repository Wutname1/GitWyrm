//! A clean apply finishes the job: cherry-pick, revert and merge all commit
//! immediately and leave no operation state, so nothing asks the user for a
//! second click. Only a conflicting apply leaves state behind.
//!
//! These drive the same git2 sequences the merge commands use. They exist
//! because the clean paths are exactly what a stale "ready to commit" banner
//! used to describe -- an operation that had already committed.

use std::fs;
use std::path::PathBuf;

use git2::{build::CheckoutBuilder, MergeOptions, Repository, Signature};
use gitwyrm_lib::git_merge_ops::merge_state;

fn scratch_repo(tag: &str) -> (PathBuf, Repository) {
  let dir = std::env::temp_dir().join(format!("gitwyrm-cleanapply-{}-{}", tag, std::process::id()));
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

fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
  let mut index = repo.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.update_all(["*"], None).unwrap();
  index.write().unwrap();
  let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
  let parents: Vec<git2::Commit> = repo
    .head()
    .ok()
    .and_then(|h| h.peel_to_commit().ok())
    .into_iter()
    .collect();
  let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
  repo
    .commit(Some("HEAD"), &sig(), &sig(), message, &tree, &parent_refs)
    .unwrap()
}

fn checkout_branch(repo: &Repository, name: &str) {
  repo.set_head(&format!("refs/heads/{name}")).unwrap();
  repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
}

/// Commit whatever the operation left in the index, mirroring the tail of the
/// clean paths in commands::merge (single parent for a pick, two for a merge).
fn finish(repo: &Repository, author: &Signature<'_>, message: &str, other_parent: Option<git2::Oid>) {
  let mut index = repo.index().unwrap();
  let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
  let head = repo.head().unwrap().peel_to_commit().unwrap();
  let extra = other_parent.map(|oid| repo.find_commit(oid).unwrap());
  let mut parents: Vec<&git2::Commit> = vec![&head];
  if let Some(ref c) = extra {
    parents.push(c);
  }
  repo
    .commit(Some("HEAD"), author, &sig(), message, &tree, &parents)
    .unwrap();
  repo.cleanup_state().unwrap();
}

/// Base commit on the default branch, plus a `feature` branch holding one
/// commit that adds `b.txt`. Returns (dir, repo, default branch, feature tip).
fn repo_with_feature(tag: &str) -> (PathBuf, Repository, String, git2::Oid) {
  let (dir, repo) = scratch_repo(tag);
  fs::write(dir.join("a.txt"), "base\n").unwrap();
  let base = commit_all(&repo, "base");
  let main = repo.head().unwrap().shorthand().unwrap().to_string();
  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  checkout_branch(&repo, "feature");
  fs::write(dir.join("b.txt"), "other\n").unwrap();
  let feat = commit_all(&repo, "add b");
  checkout_branch(&repo, &main);
  (dir, repo, main, feat)
}

#[test]
fn clean_cherry_pick_leaves_no_operation_state() {
  let (_dir, repo, _main, feat) = repo_with_feature("pick");
  let commit = repo.find_commit(feat).unwrap();

  repo.cherrypick(&commit, None).unwrap();
  assert!(
    !repo.index().unwrap().has_conflicts(),
    "this pick should apply cleanly"
  );

  finish(&repo, &commit.author(), commit.message().unwrap_or(""), None);

  let state = merge_state(&repo).unwrap();
  assert!(!state.merging, "a committed pick must not report an operation");
  assert_eq!(repo.state(), git2::RepositoryState::Clean);
  assert!(!repo.path().join("CHERRY_PICK_HEAD").exists());
  assert!(!repo.path().join("MERGE_MSG").exists());
}

/// A pick that has to merge (both branches touched the same file, in different
/// places) still applies without conflicts, and still must not leave state.
/// This is the shape that reads as "in progress, zero conflicts" mid-operation.
#[test]
fn overlapping_but_clean_pick_leaves_no_operation_state() {
  let (dir, repo) = scratch_repo("overlap");
  fs::write(dir.join("a.txt"), "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\n").unwrap();
  let base = commit_all(&repo, "base");
  let main = repo.head().unwrap().shorthand().unwrap().to_string();

  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  checkout_branch(&repo, "feature");
  fs::write(dir.join("a.txt"), "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nCHANGED9\n").unwrap();
  let feat = commit_all(&repo, "change last line");
  checkout_branch(&repo, &main);
  fs::write(dir.join("a.txt"), "CHANGED1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\n").unwrap();
  commit_all(&repo, "change first line");

  let commit = repo.find_commit(feat).unwrap();
  repo.cherrypick(&commit, None).unwrap();
  assert!(!repo.index().unwrap().has_conflicts());
  // Mid-operation this is precisely the misleading state: an operation is in
  // progress and nothing is conflicted.
  assert!(merge_state(&repo).unwrap().merging);

  finish(&repo, &commit.author(), commit.message().unwrap_or(""), None);

  assert!(!merge_state(&repo).unwrap().merging);
  // Compared with CRLF normalised away: core.autocrlf on Windows rewrites the
  // checkout, which says nothing about whether the merge kept both edits.
  let merged = fs::read_to_string(dir.join("a.txt")).unwrap().replace("\r\n", "\n");
  assert_eq!(
    merged, "CHANGED1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nCHANGED9\n",
    "both edits should survive the pick"
  );
}

/// The clean merge path added alongside the banner change: a non-fast-forward
/// merge that applies without conflicts commits itself, rather than staging the
/// result and waiting to be asked again.
#[test]
fn clean_merge_commits_itself() {
  let (dir, repo) = scratch_repo("merge");
  fs::write(dir.join("a.txt"), "base\n").unwrap();
  let base = commit_all(&repo, "base");
  let main = repo.head().unwrap().shorthand().unwrap().to_string();

  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  checkout_branch(&repo, "feature");
  fs::write(dir.join("b.txt"), "from feature\n").unwrap();
  let feat = commit_all(&repo, "add b");
  checkout_branch(&repo, &main);
  // A separate change on main so this is a real merge, not a fast-forward.
  fs::write(dir.join("c.txt"), "from main\n").unwrap();
  commit_all(&repo, "add c");

  let annotated = repo.find_annotated_commit(feat).unwrap();
  let (analysis, _) = repo.merge_analysis(&[&annotated]).unwrap();
  assert!(analysis.is_normal() && !analysis.is_fast_forward());

  let mut checkout = CheckoutBuilder::new();
  checkout.allow_conflicts(true).conflict_style_merge(true);
  repo
    .merge(&[&annotated], Some(&mut MergeOptions::new()), Some(&mut checkout))
    .unwrap();
  assert!(!repo.index().unwrap().has_conflicts(), "this merge is clean");

  finish(&repo, &sig(), "Merge branch 'feature'", Some(feat));

  let state = merge_state(&repo).unwrap();
  assert!(!state.merging, "a clean merge must not wait to be committed");
  assert_eq!(repo.state(), git2::RepositoryState::Clean);

  let head = repo.head().unwrap().peel_to_commit().unwrap();
  assert_eq!(head.parent_count(), 2, "should be a real merge commit");
  assert!(dir.join("b.txt").exists() && dir.join("c.txt").exists());
}

/// The counterpart: a conflicting pick DOES leave state, because that is the one
/// case the banner is for.
#[test]
fn conflicting_pick_keeps_operation_state() {
  let (dir, repo) = scratch_repo("conflict");
  fs::write(dir.join("a.txt"), "base\n").unwrap();
  let base = commit_all(&repo, "base");
  let main = repo.head().unwrap().shorthand().unwrap().to_string();

  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  checkout_branch(&repo, "feature");
  fs::write(dir.join("a.txt"), "feature version\n").unwrap();
  let feat = commit_all(&repo, "feature edit");
  checkout_branch(&repo, &main);
  fs::write(dir.join("a.txt"), "main version\n").unwrap();
  commit_all(&repo, "main edit");

  let commit = repo.find_commit(feat).unwrap();
  repo.cherrypick(&commit, None).unwrap();

  let state = merge_state(&repo).unwrap();
  assert!(state.merging, "a conflicting pick stays in progress");
  assert!(!state.conflicts.is_empty(), "and reports its conflicts");
}
