//! End-to-end mutation flow against a throwaway repo in the temp dir:
//! init -> write file -> stage -> commit -> modify -> stash -> pop -> branch -> checkout.

use std::fs;
use std::path::PathBuf;

use git2::{Repository, Signature};

fn scratch_repo() -> (PathBuf, Repository) {
  let dir = std::env::temp_dir().join(format!("gitwyrm-test-{}", std::process::id()));
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

#[test]
fn full_stage_commit_stash_branch_cycle() {
  let (dir, mut repo) = scratch_repo();

  // -- stage + initial commit
  fs::write(dir.join("readme.md"), "hello wyrm\n").unwrap();
  let first = {
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("readme.md")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    repo
      .commit(Some("HEAD"), &sig(), &sig(), "initial", &tree, &[])
      .unwrap()
  };
  assert!(!first.is_zero());

  // -- second commit through the same flow the command uses
  fs::write(dir.join("code.rs"), "fn main() {}\n").unwrap();
  let second = {
    let mut index = repo.index().unwrap();
    index
      .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
      .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo
      .commit(Some("HEAD"), &sig(), &sig(), "add code", &tree, &[&parent])
      .unwrap()
  };
  assert_ne!(first, second);

  // -- empty-commit guard: staged tree identical to HEAD must be detectable
  {
    let mut index = repo.index().unwrap();
    let tree_oid = index.write_tree().unwrap();
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().tree_id(), tree_oid);
  }

  // -- stash save / pop
  fs::write(dir.join("readme.md"), "hello wyrm changed\n").unwrap();
  repo
    .stash_save(&sig(), "WIP test", Some(git2::StashFlags::INCLUDE_UNTRACKED))
    .unwrap();
  // Workdir is clean after stash.
  {
    let statuses = repo.statuses(None).unwrap();
    assert_eq!(statuses.iter().count(), 0, "stash should clean the workdir");
  }
  repo.stash_pop(0, None).unwrap();
  {
    let statuses = repo.statuses(None).unwrap();
    assert!(statuses.iter().count() > 0, "pop should restore the change");
  }

  // -- discard restores HEAD content
  let mut builder = git2::build::CheckoutBuilder::new();
  builder.path("readme.md").force();
  repo.checkout_head(Some(&mut builder)).unwrap();
  // autocrlf may rewrite line endings on checkout; compare normalized.
  assert_eq!(
    fs::read_to_string(dir.join("readme.md")).unwrap().replace("\r\n", "\n"),
    "hello wyrm\n"
  );

  // -- create branch + checkout (clean tree required, mirroring the command's guard)
  let head = repo.head().unwrap().peel_to_commit().unwrap();
  repo.branch("feature/x", &head, false).unwrap();
  let obj = repo.revparse_single("refs/heads/feature/x").unwrap();
  repo.checkout_tree(&obj, None).unwrap();
  repo.set_head("refs/heads/feature/x").unwrap();
  assert_eq!(repo.head().unwrap().shorthand().unwrap(), "feature/x");

  let _ = fs::remove_dir_all(&dir);
}
