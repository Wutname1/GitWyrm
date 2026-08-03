//! Scenario-level checks that walk the sequences the UI drives, against real
//! repositories.
//!
//! `tests/worktree.rs` covers each operation on its own. These cover the
//! multi-step traps the feature exists to remove -- the chains where each raw
//! git refusal names a symptom and no cause: a branch that will not delete
//! because a worktree holds it, a removal refused for dirt and then resolved,
//! an externally deleted folder, and an externally moved project.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git2::Repository;
use gitwyrm_lib::git_worktree::{self as worktree, DirtyChoice, RemoveOutcome, WorktreeState};

fn git(dir: &Path, args: &[&str]) {
  let out = Command::new("git")
    .args(args)
    .current_dir(dir)
    .output()
    .expect("git should be installed");
  assert!(
    out.status.success(),
    "git {:?} failed: {}",
    args,
    String::from_utf8_lossy(&out.stderr)
  );
}

fn identity(dir: &Path) {
  git(dir, &["config", "user.email", "t@gitwyrm.dev"]);
  git(dir, &["config", "user.name", "Test Wyrm"]);
}

fn fixture(label: &str) -> Option<(PathBuf, PathBuf)> {
  Command::new("git").arg("--version").output().ok()?;
  let root = std::env::temp_dir().join(format!("gitwyrm-wtc-{}-{label}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(&root).unwrap();

  let repo = root.join("project");
  fs::create_dir_all(&repo).unwrap();
  git(&repo, &["init", "-q", "-b", "main"]);
  identity(&repo);
  fs::write(repo.join("README.md"), "# demo").unwrap();
  git(&repo, &["add", "."]);
  git(&repo, &["commit", "-qm", "init"]);
  git(&repo, &["branch", "feature"]);
  Some((root, repo))
}

fn s(p: &Path) -> String {
  p.to_string_lossy().into_owned()
}

/// The chain that makes branches feel undeletable: git refuses the delete
/// because a worktree holds the branch, and says nothing about which one or
/// what to do. GitWyrm has to name the folder, and the deletion has to go
/// through once that folder is gone.
#[test]
fn the_branch_that_will_not_delete_resolves_in_one_flow() {
  let Some((root, repo_path)) = fixture("chain") else { return };
  let wt_path = root.join("project-feature");
  worktree::add(&s(&repo_path), &s(&wt_path), "feature", false, None).unwrap();

  let repo = Repository::open(&repo_path).unwrap();

  // Step 1: the branch is held, and we can say by which folder.
  let holder = worktree::holder_of_branch(&repo, "feature").expect("held");
  assert_eq!(holder.folder_name, "project-feature");

  // Step 2: removing that worktree is offered from there, and goes through.
  let outcome =
    worktree::remove(&repo, &s(&repo_path), &s(&wt_path), DirtyChoice::Refuse).unwrap();
  assert!(matches!(outcome, RemoveOutcome::Removed { .. }));

  // Step 3: nothing holds it now, so the deletion the user asked for can run.
  let repo = Repository::open(&repo_path).unwrap();
  assert!(worktree::holder_of_branch(&repo, "feature").is_none());
  let mut branch = repo.find_branch("feature", git2::BranchType::Local).unwrap();
  branch.delete().expect("the branch deletes once nothing holds it");

  let _ = fs::remove_dir_all(&root);
}

/// The dirty-worktree chain: refused first with counts, then resolved by the
/// user's choice, without them ever learning a force flag.
#[test]
fn a_dirty_worktree_is_refused_then_removed_by_choice() {
  let Some((root, repo_path)) = fixture("dirtychain") else { return };
  let wt_path = root.join("project-feature");
  worktree::add(&s(&repo_path), &s(&wt_path), "feature", false, None).unwrap();
  identity(&wt_path);
  fs::write(wt_path.join("README.md"), "# edited").unwrap();
  fs::write(wt_path.join("new.txt"), "never saved").unwrap();

  let repo = Repository::open(&repo_path).unwrap();

  // First ask: refused, with the counts the confirm states.
  match worktree::remove(&repo, &s(&repo_path), &s(&wt_path), DirtyChoice::Refuse).unwrap() {
    RemoveOutcome::RefusedDirty { modified, untracked } => {
      assert_eq!((modified, untracked), (1, 1));
    }
    other => panic!("expected a refusal first: {other:?}"),
  }
  assert!(wt_path.exists(), "the first ask never destroys anything");

  // Second ask, after the confirm: keep the work and remove.
  let outcome = worktree::remove(&repo, &s(&repo_path), &s(&wt_path), DirtyChoice::Keep).unwrap();
  assert!(matches!(outcome, RemoveOutcome::Removed { .. }));
  assert!(!wt_path.exists());

  // The kept work is where GitWyrm said it went.
  let mut repo = Repository::open(&repo_path).unwrap();
  let mut stashes = 0;
  repo.stash_foreach(|_, _, _| {
    stashes += 1;
    true
  })
  .unwrap();
  assert_eq!(stashes, 1, "keeping the work must leave it recoverable");

  let _ = fs::remove_dir_all(&root);
}

/// Deleting a worktree folder by hand is a normal thing to have done. The
/// leftover reference is tidying up, not an error, and pruning is one step.
#[test]
fn a_folder_deleted_by_hand_prunes_cleanly() {
  let Some((root, repo_path)) = fixture("handdel") else { return };
  let wt_path = root.join("project-feature");
  worktree::add(&s(&repo_path), &s(&wt_path), "feature", false, None).unwrap();
  fs::remove_dir_all(&wt_path).unwrap();

  let repo = Repository::open(&repo_path).unwrap();
  let broken = worktree::list(&repo, None)
    .into_iter()
    .find(|w| !w.is_main)
    .expect("the row survives so the user can tidy it");
  assert_eq!(broken.state, WorktreeState::Missing);

  // Removing it takes the prune path -- there is no folder left to delete.
  let outcome =
    worktree::remove(&repo, &s(&repo_path), &s(&wt_path), DirtyChoice::Refuse).unwrap();
  assert!(matches!(outcome, RemoveOutcome::Removed { .. }));

  let repo = Repository::open(&repo_path).unwrap();
  assert_eq!(worktree::list(&repo, None).len(), 1);
  // The branch it had is untouched, as always.
  assert!(repo.find_branch("feature", git2::BranchType::Local).is_ok());

  let _ = fs::remove_dir_all(&root);
}

/// Moving the whole project breaks every worktree at once. Repair from the
/// project fixes them together rather than one row at a time.
#[test]
fn moving_the_whole_project_is_repaired_in_one_step() {
  let Some((root, repo_path)) = fixture("moverepo") else { return };
  let a = root.join("project-a");
  let b = root.join("project-b");
  worktree::add(&s(&repo_path), &s(&a), "feature", false, None).unwrap();
  worktree::add(&s(&repo_path), &s(&b), "second", true, Some("main")).unwrap();

  // Move the project itself. Each worktree's `.git` file still points at the
  // old location, so all of them break together.
  let moved = root.join("project-moved");
  fs::rename(&repo_path, &moved).unwrap();

  // Repair with no path is the whole-project case.
  worktree::repair(&s(&moved), None).unwrap();

  let repo = Repository::open(&moved).unwrap();
  let list = worktree::list(&repo, None);
  let linked: Vec<_> = list.iter().filter(|w| !w.is_main).collect();
  assert_eq!(linked.len(), 2);
  for w in linked {
    assert_eq!(w.state, WorktreeState::Ok, "{} should be healthy again", w.folder_name);
  }

  let _ = fs::remove_dir_all(&root);
}

/// A worktree created outside GitWyrm is listed like any other: nothing about
/// the list depends on GitWyrm having made it.
#[test]
fn a_worktree_made_in_a_terminal_is_listed_and_removable() {
  let Some((root, repo_path)) = fixture("external") else { return };
  let wt_path = root.join("made-elsewhere");

  // Exactly what a user would type in a terminal beside an open window.
  git(&repo_path, &["worktree", "add", &s(&wt_path), "feature"]);

  let repo = Repository::open(&repo_path).unwrap();
  let found = worktree::list(&repo, None)
    .into_iter()
    .find(|w| w.folder_name == "made-elsewhere")
    .expect("an externally made worktree is listed");
  assert_eq!(found.state, WorktreeState::Ok);
  assert_eq!(found.branch.as_deref(), Some("feature"));
  assert!(!found.is_run_worktree, "not ours, so not marked as a run's");

  // And it is manageable, not merely visible.
  let outcome =
    worktree::remove(&repo, &s(&repo_path), &s(&wt_path), DirtyChoice::Refuse).unwrap();
  assert!(matches!(outcome, RemoveOutcome::Removed { .. }));
  assert!(!wt_path.exists());

  let _ = fs::remove_dir_all(&root);
}

/// Committing in one checkout must leave the other alone. That separation is
/// the entire reason to have a second folder.
#[test]
fn committing_in_a_worktree_leaves_the_main_checkout_untouched() {
  let Some((root, repo_path)) = fixture("isolate") else { return };
  let wt_path = root.join("project-feature");
  worktree::add(&s(&repo_path), &s(&wt_path), "feature", false, None).unwrap();
  identity(&wt_path);

  // Leave uncommitted work in the main checkout, as a user mid-task would.
  fs::write(repo_path.join("my-work-in-progress.txt"), "half done").unwrap();

  fs::write(wt_path.join("from-the-worktree.txt"), "work").unwrap();
  git(&wt_path, &["add", "."]);
  git(&wt_path, &["commit", "-qm", "work done in the other folder"]);

  // The main checkout's own file is untouched and its branch has not moved.
  assert_eq!(
    fs::read_to_string(repo_path.join("my-work-in-progress.txt")).unwrap(),
    "half done"
  );
  assert!(!repo_path.join("from-the-worktree.txt").exists());

  let repo = Repository::open(&repo_path).unwrap();
  let main_tip = repo.find_branch("main", git2::BranchType::Local).unwrap().get().target();
  let feature_tip = repo
    .find_branch("feature", git2::BranchType::Local)
    .unwrap()
    .get()
    .target();
  assert_ne!(main_tip, feature_tip, "only the worktree's branch moved");

  let _ = fs::remove_dir_all(&root);
}

/// The suggested folder never lands inside the project, whatever the branch is
/// called -- a checkout committed into its own repository is unrecoverable
/// tidiness-wise and trivially avoided.
#[test]
fn suggestions_never_land_inside_the_project() {
  let Some((root, repo_path)) = fixture("suggestsafe") else { return };
  for branch in ["feature/login", "fix", "release/2.0", "weird//name", "..", "-dashy"] {
    let suggested = worktree::suggest_path(&repo_path, branch);
    let p = Path::new(&suggested);
    assert!(
      !p.starts_with(&repo_path),
      "{branch} suggested {suggested}, which is inside the project"
    );
    assert!(worktree::path_problem(&repo_path, p).is_none(), "{suggested} should be usable");
  }
  let _ = fs::remove_dir_all(&root);
}
