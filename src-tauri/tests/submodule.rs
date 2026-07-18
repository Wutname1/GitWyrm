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
