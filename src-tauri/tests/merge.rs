//! End-to-end merge + conflict resolution against throwaway repos, mirroring the
//! exact git2 operations the merge commands perform (merge, read conflict stages,
//! resolve, commit the merge).

use std::fs;
use std::path::{Path, PathBuf};

use git2::{build::CheckoutBuilder, MergeOptions, Repository, Signature};

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

/// Stage every change and commit; returns the new commit oid.
fn commit_all(repo: &Repository, dir: &Path, message: &str) -> git2::Oid {
  let mut index = repo.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.write().unwrap();
  let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
  let parents: Vec<git2::Commit> = repo
    .head()
    .ok()
    .and_then(|h| h.peel_to_commit().ok())
    .into_iter()
    .collect();
  let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
  let _ = dir; // dir already reflected in index via add_all
  repo
    .commit(Some("HEAD"), &sig(), &sig(), message, &tree, &parent_refs)
    .unwrap()
}

/// Read one index conflict stage's blob text, matching stage_text() in merge.rs.
fn stage_text(repo: &Repository, index: &git2::Index, path: &str, stage: i32) -> String {
  match index.get_path(Path::new(path), stage) {
    Some(entry) => {
      let blob = repo.find_blob(entry.id).unwrap();
      String::from_utf8_lossy(blob.content()).into_owned()
    }
    None => String::new(),
  }
}

#[test]
fn fast_forward_merge_advances_head() {
  let (dir, repo) = scratch_repo("ff");

  fs::write(dir.join("f.txt"), "base\n").unwrap();
  let base = commit_all(&repo, &dir, "base");
  let main = default_branch(&repo);

  // Branch off, add a commit on feature only -> main can fast-forward to it.
  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  repo.set_head("refs/heads/feature").unwrap();
  repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
  fs::write(dir.join("f.txt"), "base\nfeature line\n").unwrap();
  let feat = commit_all(&repo, &dir, "feature work");

  // Back on main.
  repo.set_head(&format!("refs/heads/{main}")).unwrap();
  repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();

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

  // Shared base.
  fs::write(dir.join("f.txt"), "line1\nSHARED\nline3\n").unwrap();
  let base = commit_all(&repo, &dir, "base");
  let main = default_branch(&repo);

  // feature branch changes the middle line one way.
  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  repo.set_head("refs/heads/feature").unwrap();
  repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
  fs::write(dir.join("f.txt"), "line1\nTHEIRS\nline3\n").unwrap();
  let feat = commit_all(&repo, &dir, "feature change");

  // main changes the same line the other way.
  repo.set_head(&format!("refs/heads/{main}")).unwrap();
  repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
  fs::write(dir.join("f.txt"), "line1\nOURS\nline3\n").unwrap();
  commit_all(&repo, &dir, "main change");

  // Merge feature -> expect a conflict (normal, not ff).
  let annotated = repo.find_annotated_commit(feat).unwrap();
  let (analysis, _) = repo.merge_analysis(&[&annotated]).unwrap();
  assert!(analysis.is_normal() && !analysis.is_fast_forward());

  let mut checkout = CheckoutBuilder::new();
  checkout.allow_conflicts(true).conflict_style_merge(true);
  repo.merge(&[&annotated], Some(&mut MergeOptions::new()), Some(&mut checkout)).unwrap();

  assert_eq!(repo.state(), git2::RepositoryState::Merge, "merge should be in progress");
  assert!(repo.index().unwrap().has_conflicts(), "f.txt should conflict");

  // Read the three sides the way get_conflict does.
  {
    let index = repo.index().unwrap();
    let ours = stage_text(&repo, &index, "f.txt", 2);
    let theirs = stage_text(&repo, &index, "f.txt", 3);
    let base_txt = stage_text(&repo, &index, "f.txt", 1);
    assert!(ours.contains("OURS"), "stage 2 = ours");
    assert!(theirs.contains("THEIRS"), "stage 3 = theirs");
    assert!(base_txt.contains("SHARED"), "stage 1 = common ancestor");
  }

  // Resolve manually (mirror resolve_conflict: write text, remove_path, add_path).
  let resolved = "line1\nMERGED BY HAND\nline3\n";
  {
    fs::write(dir.join("f.txt"), resolved).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("f.txt")).unwrap();
    index.add_path(Path::new("f.txt")).unwrap();
    index.write().unwrap();
    assert!(!index.has_conflicts(), "conflict cleared after resolve");
  }

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

  // Verify: two parents, clean state, resolved content on disk.
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
fn abort_merge_restores_pre_merge_state() {
  let (dir, repo) = scratch_repo("abort");

  fs::write(dir.join("f.txt"), "line1\nSHARED\nline3\n").unwrap();
  let base = commit_all(&repo, &dir, "base");
  let main = default_branch(&repo);

  repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
  repo.set_head("refs/heads/feature").unwrap();
  repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
  fs::write(dir.join("f.txt"), "line1\nTHEIRS\nline3\n").unwrap();
  let feat = commit_all(&repo, &dir, "feature change");

  repo.set_head(&format!("refs/heads/{main}")).unwrap();
  repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
  fs::write(dir.join("f.txt"), "line1\nOURS\nline3\n").unwrap();
  let ours_head = commit_all(&repo, &dir, "main change");

  let annotated = repo.find_annotated_commit(feat).unwrap();
  let mut checkout = CheckoutBuilder::new();
  checkout.allow_conflicts(true).conflict_style_merge(true);
  repo.merge(&[&annotated], Some(&mut MergeOptions::new()), Some(&mut checkout)).unwrap();
  assert!(repo.index().unwrap().has_conflicts());

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
