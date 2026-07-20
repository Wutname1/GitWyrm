//! End-to-end merge + conflict resolution against throwaway repos. Conflict
//! reading and resolution go through the real gitwyrm_lib::git_merge_ops code
//! the commands use, so these tests catch drift in the actual logic.

use std::fs;
use std::path::{Path, PathBuf};

use git2::{build::CheckoutBuilder, MergeOptions, Repository, Signature};
use gitwyrm_lib::git_merge_ops::{apply_resolution, conflict_content, Resolution};

fn scratch_repo(tag: &str) -> (PathBuf, Repository) {
  let dir = std::env::temp_dir().join(format!("gitwyrm-merge-{}-{}", tag, std::process::id()));
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

/// The default branch name after the first commit (init.defaultBranch varies).
fn default_branch(repo: &Repository) -> String {
  repo.head().unwrap().shorthand().unwrap().to_string()
}

/// Stage every change (including deletions) and commit; returns the new oid.
fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
  let mut index = repo.index().unwrap();
  index
    .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
    .unwrap();
  index
    .update_all(["*"], None)
    .unwrap();
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

/// Branch off `base`, run `mutate` there and commit, then return to `main` and
/// run `mutate_main` and commit. Returns the feature tip for merging.
fn diverge(
  repo: &Repository,
  base: git2::Oid,
  main: &str,
  mutate_feature: impl FnOnce(),
  mutate_main: impl FnOnce(),
) -> git2::Oid {
  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  checkout_branch(repo, "feature");
  mutate_feature();
  let feat = commit_all(repo, "feature change");
  checkout_branch(repo, main);
  mutate_main();
  commit_all(repo, "main change");
  feat
}

/// Merge `feat` into HEAD expecting conflicts, as merge_branch's normal path does.
fn merge_expect_conflict(repo: &Repository, feat: git2::Oid) {
  let annotated = repo.find_annotated_commit(feat).unwrap();
  let mut checkout = CheckoutBuilder::new();
  checkout.allow_conflicts(true).conflict_style_merge(true);
  repo
    .merge(&[&annotated], Some(&mut MergeOptions::new()), Some(&mut checkout))
    .unwrap();
  assert_eq!(repo.state(), git2::RepositoryState::Merge, "merge should be in progress");
  assert!(repo.index().unwrap().has_conflicts(), "merge should conflict");
}

#[test]
fn fast_forward_merge_advances_head() {
  let (dir, repo) = scratch_repo("ff");

  fs::write(dir.join("f.txt"), "base\n").unwrap();
  let base = commit_all(&repo, "base");
  let main = default_branch(&repo);

  // Branch off, add a commit on feature only -> main can fast-forward to it.
  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  checkout_branch(&repo, "feature");
  fs::write(dir.join("f.txt"), "base\nfeature line\n").unwrap();
  let feat = commit_all(&repo, "feature work");
  checkout_branch(&repo, &main);

  let annotated = repo.find_annotated_commit(feat).unwrap();
  let (analysis, _) = repo.merge_analysis(&[&annotated]).unwrap();
  assert!(analysis.is_fast_forward(), "should be fast-forwardable");

  // Perform the fast-forward as merge_branch does.
  let target = repo.find_object(feat, None).unwrap();
  repo.checkout_tree(&target, Some(CheckoutBuilder::new().safe())).unwrap();
  repo.reference(&format!("refs/heads/{main}"), feat, true, "ff").unwrap();

  assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), feat);
  assert!(!repo.index().unwrap().has_conflicts());

  let _ = fs::remove_dir_all(&dir);
}

#[test]
fn conflicting_merge_resolves_and_commits() {
  let (dir, repo) = scratch_repo("conflict");

  fs::write(dir.join("f.txt"), "line1\nSHARED\nline3\n").unwrap();
  let base = commit_all(&repo, "base");
  let main = default_branch(&repo);

  let feat = diverge(
    &repo,
    base,
    &main,
    || fs::write(dir.join("f.txt"), "line1\nTHEIRS\nline3\n").unwrap(),
    || fs::write(dir.join("f.txt"), "line1\nOURS\nline3\n").unwrap(),
  );
  merge_expect_conflict(&repo, feat);

  // Read the three sides through the real conflict_content.
  let content = conflict_content(&repo, &dir, "f.txt").unwrap();
  assert!(content.ours.contains("OURS"), "stage 2 = ours");
  assert!(content.theirs.contains("THEIRS"), "stage 3 = theirs");
  assert!(content.base.contains("SHARED"), "stage 1 = common ancestor");
  assert!(content.merged.contains("<<<<<<<"), "working tree has markers");
  assert!(!content.binary);
  assert!(!content.ours_deleted && !content.theirs_deleted);

  // Resolve manually through the real apply_resolution.
  let resolved = "line1\nMERGED BY HAND\nline3\n";
  apply_resolution(&repo, &dir, "f.txt", &Resolution::Manual { text: resolved.into() }).unwrap();
  assert!(!repo.index().unwrap().has_conflicts(), "conflict cleared after resolve");

  // Commit the merge (mirror commit_merge: two parents, cleanup state).
  let merge_head = {
    let content = fs::read_to_string(repo.path().join("MERGE_HEAD")).unwrap();
    git2::Oid::from_str(content.trim()).unwrap()
  };
  assert_eq!(merge_head, feat, "MERGE_HEAD points at the merged commit");

  let merge_oid = {
    let mut index = repo.index().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let merge_commit = repo.find_commit(merge_head).unwrap();
    repo
      .commit(Some("HEAD"), &sig(), &sig(), "Merge feature", &tree, &[&head_commit, &merge_commit])
      .unwrap()
  };
  repo.cleanup_state().unwrap();

  let merge_commit = repo.find_commit(merge_oid).unwrap();
  assert_eq!(merge_commit.parent_count(), 2, "merge commit has two parents");
  assert_eq!(repo.state(), git2::RepositoryState::Clean, "state cleaned up");
  assert_eq!(
    fs::read_to_string(dir.join("f.txt")).unwrap().replace("\r\n", "\n"),
    resolved
  );

  let _ = fs::remove_dir_all(&dir);
}

#[test]
fn binary_conflict_resolution_keeps_chosen_bytes() {
  let (dir, repo) = scratch_repo("binary");

  let base_bytes: &[u8] = b"BIN\x00base\x01\x02";
  let ours_bytes: &[u8] = b"BIN\x00ours\x03\x04\x05";
  let theirs_bytes: &[u8] = b"BIN\x00theirs\x06";

  fs::write(dir.join("img.bin"), base_bytes).unwrap();
  let base = commit_all(&repo, "base");
  let main = default_branch(&repo);

  let feat = diverge(
    &repo,
    base,
    &main,
    || fs::write(dir.join("img.bin"), theirs_bytes).unwrap(),
    || fs::write(dir.join("img.bin"), ours_bytes).unwrap(),
  );
  merge_expect_conflict(&repo, feat);

  let content = conflict_content(&repo, &dir, "img.bin").unwrap();
  assert!(content.binary, "null bytes should read as binary");

  // Choosing a side must reproduce that side's exact bytes, not text.
  apply_resolution(&repo, &dir, "img.bin", &Resolution::Ours).unwrap();
  assert_eq!(fs::read(dir.join("img.bin")).unwrap(), ours_bytes, "ours bytes intact");
  assert!(!repo.index().unwrap().has_conflicts());

  let _ = fs::remove_dir_all(&dir);
}

#[test]
fn modify_delete_conflict_can_keep_or_delete() {
  let (dir, repo) = scratch_repo("moddel");

  // Two files sharing the same fate: ours modifies both, theirs deletes both.
  fs::write(dir.join("keep.txt"), "original\n").unwrap();
  fs::write(dir.join("drop.txt"), "original\n").unwrap();
  let base = commit_all(&repo, "base");
  let main = default_branch(&repo);

  let feat = diverge(
    &repo,
    base,
    &main,
    || {
      fs::remove_file(dir.join("keep.txt")).unwrap();
      fs::remove_file(dir.join("drop.txt")).unwrap();
    },
    || {
      fs::write(dir.join("keep.txt"), "ours edit\n").unwrap();
      fs::write(dir.join("drop.txt"), "ours edit\n").unwrap();
    },
  );
  merge_expect_conflict(&repo, feat);

  let content = conflict_content(&repo, &dir, "keep.txt").unwrap();
  assert!(content.theirs_deleted, "their side deleted the file");
  assert!(!content.ours_deleted);
  assert!(content.ours.contains("ours edit"));

  // Keep our modified copy for one file.
  apply_resolution(&repo, &dir, "keep.txt", &Resolution::Ours).unwrap();
  assert_eq!(fs::read_to_string(dir.join("keep.txt")).unwrap(), "ours edit\n");

  // Accept their deletion for the other: file gone, deletion staged.
  apply_resolution(&repo, &dir, "drop.txt", &Resolution::Theirs).unwrap();
  assert!(!dir.join("drop.txt").exists(), "choosing the deleting side removes the file");

  let index = repo.index().unwrap();
  assert!(!index.has_conflicts(), "both conflicts cleared");
  assert!(index.get_path(Path::new("keep.txt"), 0).is_some(), "kept file staged");
  assert!(index.get_path(Path::new("drop.txt"), 0).is_none(), "deletion staged");

  let _ = fs::remove_dir_all(&dir);
}

#[test]
fn abort_merge_restores_pre_merge_state() {
  let (dir, repo) = scratch_repo("abort");

  fs::write(dir.join("f.txt"), "line1\nSHARED\nline3\n").unwrap();
  let base = commit_all(&repo, "base");
  let main = default_branch(&repo);

  let feat = diverge(
    &repo,
    base,
    &main,
    || fs::write(dir.join("f.txt"), "line1\nTHEIRS\nline3\n").unwrap(),
    || fs::write(dir.join("f.txt"), "line1\nOURS\nline3\n").unwrap(),
  );
  let ours_head = repo.head().unwrap().peel_to_commit().unwrap().id();
  merge_expect_conflict(&repo, feat);

  // Abort: hard reset to HEAD + cleanup, as abort_merge does.
  let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
  repo
    .reset(head_commit.as_object(), git2::ResetType::Hard, Some(CheckoutBuilder::new().force()))
    .unwrap();
  repo.cleanup_state().unwrap();

  assert_eq!(repo.state(), git2::RepositoryState::Clean);
  assert!(!repo.index().unwrap().has_conflicts());
  assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), ours_head);
  assert_eq!(
    fs::read_to_string(dir.join("f.txt")).unwrap().replace("\r\n", "\n"),
    "line1\nOURS\nline3\n",
    "our version restored"
  );

  let _ = fs::remove_dir_all(&dir);
}
