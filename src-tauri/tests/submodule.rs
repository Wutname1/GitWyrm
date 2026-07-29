//! Hermetic submodule tests: build a parent repo pinning a submodule, move the
//! submodule's checkout, and verify detection + reset behave correctly. This is
//! the scenario that left branch switching stuck (a moved submodule pointer that
//! stash/discard could not touch).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git2::Repository;

/// Run a git command in `dir`, asserting success. `protocol.file.allow=always`
/// is needed so `submodule add` accepts a local-path submodule under modern git.
fn git(dir: &Path, args: &[&str]) {
  let status = Command::new("git")
    .arg("-c")
    .arg("protocol.file.allow=always")
    .args(args)
    .current_dir(dir)
    .output()
    .expect("git should be installed");
  assert!(
    status.status.success(),
    "git {:?} failed: {}",
    args,
    String::from_utf8_lossy(&status.stderr)
  );
}

fn git_out(dir: &Path, args: &[&str]) -> String {
  let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
  String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn identity(dir: &Path) {
  git(dir, &["config", "user.email", "t@gitwyrm.dev"]);
  git(dir, &["config", "user.name", "Test Wyrm"]);
}

/// Returns (parent_dir, recorded_sha, second_sha). The parent pins the submodule
/// at `recorded_sha` but its checkout is moved forward to `second_sha`, so the
/// parent sees `packages/core` as modified.
fn fixture(label: &str) -> Option<(PathBuf, String, String)> {
  // Skip gracefully if git isn't on PATH.
  Command::new("git").arg("--version").output().ok()?;

  let root = std::env::temp_dir().join(format!("gitwyrm-sub-{}-{label}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(&root).unwrap();

  // Upstream submodule repo with two commits.
  let sub = root.join("sub");
  fs::create_dir_all(&sub).unwrap();
  git(&sub, &["init", "-q"]);
  identity(&sub);
  fs::write(sub.join("f.txt"), "v1").unwrap();
  git(&sub, &["add", "."]);
  git(&sub, &["commit", "-qm", "c1"]);
  let c1 = git_out(&sub, &["rev-parse", "HEAD"]);
  fs::write(sub.join("f.txt"), "v2").unwrap();
  git(&sub, &["add", "."]);
  git(&sub, &["commit", "-qm", "c2"]);
  let c2 = git_out(&sub, &["rev-parse", "HEAD"]);

  // Parent repo pins the submodule at c1.
  let parent = root.join("parent");
  fs::create_dir_all(&parent).unwrap();
  git(&parent, &["init", "-q"]);
  identity(&parent);
  git(&parent, &["submodule", "add", "-q", "../sub", "packages/core"]);
  let core = parent.join("packages/core");
  git(&core, &["checkout", "-q", &c1]);
  git(&parent, &["add", "."]);
  git(&parent, &["commit", "-qm", "pin core at c1"]);

  // Move the submodule checkout forward to c2 -> parent shows it modified.
  git(&core, &["checkout", "-q", &c2]);

  Some((parent, c1, c2))
}

#[test]
fn detects_moved_submodule_with_from_to_sha() {
  let Some((parent, recorded, workdir)) = fixture("detect") else { return };
  let repo = Repository::open(&parent).unwrap();

  let moves = gitwyrm_lib::git_submodule::moved_submodules(&repo);
  let mv = moves.get("packages/core").expect("moved submodule must be detected");

  assert_eq!(mv.recorded_sha, recorded, "recorded sha is the pinned commit");
  assert_eq!(mv.workdir_sha.as_deref(), Some(workdir.as_str()), "workdir sha is the moved-to commit");
  assert!(mv.initialized);
  assert_eq!(mv.ahead, 1, "workdir is one commit ahead of the recorded commit");
  assert_eq!(mv.behind, 0);
}

#[test]
fn in_sync_submodule_is_not_reported() {
  let Some((parent, recorded, _workdir)) = fixture("insync") else { return };
  let repo = Repository::open(&parent).unwrap();

  // Snap the submodule back to the recorded commit.
  let core = parent.join("packages/core");
  git(&core, &["checkout", "-q", &recorded]);

  let moves = gitwyrm_lib::git_submodule::moved_submodules(&repo);
  assert!(moves.is_empty(), "an in-sync submodule must not be reported as moved");
}

#[test]
fn is_submodule_recognizes_the_path() {
  let Some((parent, _r, _w)) = fixture("ispath") else { return };
  let repo = Repository::open(&parent).unwrap();
  assert!(gitwyrm_lib::git_submodule::is_submodule(&repo, "packages/core"));
  assert!(!gitwyrm_lib::git_submodule::is_submodule(&repo, "packages/core/f.txt"));
  assert!(!gitwyrm_lib::git_submodule::is_submodule(&repo, "README.md"));
}

/// The stuck case: a moved submodule pointer, switching to a branch that pins
/// the SAME submodule commit. A plain safe checkout must carry it across -- this
/// is what GitKraken does and what checkout_branch's AutoStash arm now tries
/// first, instead of stashing (which fails with "nothing to stash").
#[test]
fn safe_checkout_carries_a_moved_submodule() {
  let Some((parent, _recorded, workdir)) = fixture("carry") else { return };

  // Create a second branch that also pins the submodule -- at the moved-to
  // commit, so switching to it is compatible with the current checkout.
  let core = parent.join("packages/core");
  git(&parent, &["checkout", "-qb", "feature"]);
  git(&core, &["checkout", "-q", &workdir]);
  git(&parent, &["add", "packages/core"]);
  git(&parent, &["commit", "-qm", "feature pins core at c2"]);

  // Back on the original branch, move the submodule forward again (dirty state).
  git(&parent, &["checkout", "-q", "master"]);
  git(&core, &["checkout", "-q", &workdir]);
  assert!(!git_out(&parent, &["status", "--short"]).is_empty(), "submodule move should be dirty");

  // A safe checkout to the feature branch should succeed and carry the pointer,
  // exactly what the command relies on -- no stash involved.
  let repo = Repository::open(&parent).unwrap();
  let (object, reference) = repo.revparse_ext("feature").unwrap();
  let mut builder = git2::build::CheckoutBuilder::new();
  builder.safe();
  repo.checkout_tree(&object, Some(&mut builder)).expect("safe checkout should carry the submodule");
  repo.set_head(reference.unwrap().name().unwrap()).unwrap();

  assert_eq!(git_out(&parent, &["rev-parse", "--abbrev-ref", "HEAD"]), "feature");
}


/// Build a parent at c1 plus a `bump` branch whose tip moves the submodule to
/// c2. Returns (parent, core_path, bump_sha, c1, c2) with master checked out
/// clean at c1 -- the setup for picking a submodule bump onto master.
fn bump_fixture(label: &str) -> Option<(PathBuf, PathBuf, String, String, String)> {
  let (parent, c1, c2) = fixture(label)?;
  let core = parent.join("packages/core");

  git(&core, &["checkout", "-q", &c1]);
  git(&parent, &["checkout", "-qb", "bump"]);
  git(&core, &["checkout", "-q", &c2]);
  git(&parent, &["add", "packages/core"]);
  git(&parent, &["commit", "-qm", "bump core to c2"]);
  let bump_sha = git_out(&parent, &["rev-parse", "HEAD"]);

  git(&parent, &["checkout", "-q", "master"]);
  git(&core, &["checkout", "-q", &c1]);
  assert!(
    git_out(&parent, &["status", "--short"]).is_empty(),
    "fixture must start clean"
  );

  Some((parent, core, bump_sha, c1, c2))
}

/// Cherry-picking a commit that bumps a submodule pointer must leave the repo
/// CLEAN. git2 writes the new gitlink into the index but never moves the nested
/// checkout, so without an explicit sync the pick "succeeds" and immediately
/// leaves `packages/core` modified -- which then blocks the next operation.
#[test]
fn cherry_picked_submodule_bump_leaves_repo_clean() {
  let Some((parent, core, bump_sha, _c1, c2)) = bump_fixture("cpclean") else { return };
  let repo = Repository::open(&parent).unwrap();

  let commit = repo.find_commit(git2::Oid::from_str(&bump_sha).unwrap()).unwrap();
  repo.cherrypick(&commit, None).unwrap();
  assert!(!repo.index().unwrap().has_conflicts(), "bump onto an ancestor should apply cleanly");

  let mut index = repo.index().unwrap();
  let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
  let committer = repo.signature().unwrap();
  let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
  repo
    .commit(Some("HEAD"), &commit.author(), &committer, "pick", &tree, &[&head_commit])
    .unwrap();
  repo.cleanup_state().unwrap();

  // The committed pointer is correct even before the fix; the bug is that the
  // working tree disagrees with it.
  assert_eq!(git_out(&parent, &["rev-parse", "HEAD:packages/core"]), c2);

  gitwyrm_lib::git_submodule::sync_submodule_workdirs(&repo);

  assert_eq!(
    git_out(&core, &["rev-parse", "HEAD"]),
    c2,
    "the submodule checkout must follow the pointer the pick committed"
  );
  assert!(
    git_out(&parent, &["status", "--short"]).is_empty(),
    "a cherry-picked submodule bump must not leave the repo dirty, got: {:?}",
    git_out(&parent, &["status", "--short"])
  );
  assert!(
    gitwyrm_lib::git_submodule::moved_submodules(&repo).is_empty(),
    "no submodule should report as moved after the sync"
  );
}

/// The sync must not touch an uninitialized submodule: there is no checkout to
/// move, and downloading one is a separate explicit action.
#[test]
fn sync_skips_uninitialized_submodules() {
  let Some((parent, core, _sha, _c1, _c2)) = bump_fixture("cpuninit") else { return };

  git(&parent, &["submodule", "deinit", "-f", "packages/core"]);
  let repo = Repository::open(&parent).unwrap();

  gitwyrm_lib::git_submodule::sync_submodule_workdirs(&repo);

  assert!(
    !core.join(".git").exists() && !core.join("f.txt").exists(),
    "a deinitialized submodule must stay uninitialized"
  );
}
