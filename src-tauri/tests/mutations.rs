//! End-to-end mutation flow against a throwaway repo in the temp dir:
//! init -> write file -> stage -> commit -> modify -> stash -> pop -> branch -> checkout.

use std::fs;
use std::path::PathBuf;

use git2::{Repository, Signature};

fn scratch_repo() -> (PathBuf, Repository) {
  scratch_repo_named("default")
}

fn scratch_repo_named(label: &str) -> (PathBuf, Repository) {
  let dir =
    std::env::temp_dir().join(format!("gitwyrm-test-{}-{label}", std::process::id()));
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

/// Mirrors the tag + branch-delete command logic: lightweight/annotated tag
/// creation, tag deletion, and deleting a non-current local branch.
#[test]
fn tag_and_branch_delete_cycle() {
  let (dir, repo) = scratch_repo_named("tags");

  fs::write(dir.join("readme.md"), "hello\n").unwrap();
  let mut index = repo.index().unwrap();
  index.add_path(std::path::Path::new("readme.md")).unwrap();
  index.write().unwrap();
  let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
  repo
    .commit(Some("HEAD"), &sig(), &sig(), "initial", &tree, &[])
    .unwrap();

  let head = repo.head().unwrap().peel_to_commit().unwrap().into_object();

  // Lightweight tag (empty message path).
  repo.tag_lightweight("v0.1.0", &head, false).unwrap();
  // Annotated tag (non-empty message path).
  repo.tag("v0.2.0", &head, &sig(), "second release", false).unwrap();

  let names: Vec<String> = repo
    .tag_names(None)
    .unwrap()
    .iter()
    .flatten()
    .map(str::to_string)
    .collect();
  assert!(names.contains(&"v0.1.0".to_string()));
  assert!(names.contains(&"v0.2.0".to_string()));

  // Delete one tag.
  repo.tag_delete("v0.1.0").unwrap();
  let after: Vec<String> = repo
    .tag_names(None)
    .unwrap()
    .iter()
    .flatten()
    .map(str::to_string)
    .collect();
  assert!(!after.contains(&"v0.1.0".to_string()));
  assert!(after.contains(&"v0.2.0".to_string()));

  // Create a branch, confirm HEAD branch can't be deleted, then delete the other.
  let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
  repo.branch("feature/y", &head_commit, false).unwrap();

  let current = repo.head().unwrap().shorthand().unwrap().to_string();
  let current_branch = repo.find_branch(&current, git2::BranchType::Local).unwrap();
  assert!(current_branch.is_head(), "HEAD guard relies on is_head");
  // The command refuses this; here we just confirm the flag is what it checks.

  let mut feature = repo.find_branch("feature/y", git2::BranchType::Local).unwrap();
  assert!(!feature.is_head());
  feature.delete().unwrap();
  assert!(repo.find_branch("feature/y", git2::BranchType::Local).is_err());

  let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rename_branch_moves_the_ref_and_refuses_an_existing_name() {
  let (dir, repo) = scratch_repo_named("rename");

  // A fresh repo has no HEAD ref until something is committed.
  fs::write(dir.join("f.txt"), "one\n").unwrap();
  let head_commit = {
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("f.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let oid = repo.commit(Some("HEAD"), &sig(), &sig(), "initial", &tree, &[]).unwrap();
    repo.find_commit(oid).unwrap()
  };
  repo.branch("old-name", &head_commit, false).unwrap();
  repo.branch("taken", &head_commit, false).unwrap();

  let mut branch = repo.find_branch("old-name", git2::BranchType::Local).unwrap();
  branch.rename("new-name", false).unwrap();

  assert!(repo.find_branch("new-name", git2::BranchType::Local).is_ok());
  assert!(repo.find_branch("old-name", git2::BranchType::Local).is_err());

  // `force = false` is what stops a rename from clobbering another branch;
  // the command checks for the collision first to give a clearer message.
  let mut again = repo.find_branch("new-name", git2::BranchType::Local).unwrap();
  assert!(again.rename("taken", false).is_err(), "rename must not overwrite an existing branch");

  // Renaming the checked-out branch is allowed: git carries HEAD along.
  let current = repo.head().unwrap().shorthand().unwrap().to_string();
  let mut current_branch = repo.find_branch(&current, git2::BranchType::Local).unwrap();
  current_branch.rename("renamed-head", false).unwrap();
  assert_eq!(repo.head().unwrap().shorthand().unwrap(), "renamed-head");

  let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ref_name_validation_matches_git_rules() {
  // The command builds the full refname before checking: `is_valid_name`
  // rejects a bare shorthand like "main", so passing the short name would
  // refuse every branch.
  assert!(git2::Reference::is_valid_name("refs/heads/main"));
  assert!(!git2::Reference::is_valid_name("main"), "shorthand is not a valid refname");
  assert!(git2::Reference::is_valid_name("refs/heads/@"), "a lone @ is legal once prefixed");

  let valid = ["feature/x", "release-1.2", "a_b", "fix.thing"];
  for name in valid {
    assert!(
      git2::Reference::is_valid_name(&format!("refs/heads/{name}")),
      "{name} should be accepted"
    );
  }

  // The cases the frontend regex screens, confirmed against git's own rules.
  // Probed against git2 directly: every one of these is refused. A lone `@` is
  // NOT here -- git accepts refs/heads/@, so the frontend regex must not
  // reject it either.
  let invalid =
    ["my branch", "a..b", "a~b", "a^b", "a:b", "a?b", "a*b", "a[b", "a//b", "a/", ".hidden", "a.lock"];
  for name in invalid {
    assert!(
      !git2::Reference::is_valid_name(&format!("refs/heads/{name}")),
      "{name} should be rejected"
    );
  }
}
