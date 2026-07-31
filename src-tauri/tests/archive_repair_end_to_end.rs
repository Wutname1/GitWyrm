//! The leftover-archive failure, start to finish, against the real CLI.
//!
//! Reproduces the state a half-finished archive leaves behind: specs merged,
//! an empty archive folder created, the change still sitting in `changes/`.
//! Then checks that GitWyrm recognises it, repairs it, and archives -- which is
//! the whole promise of the repair flow.
//!
//! Skipped when the OpenSpec CLI is not installed, so it stays green on a
//! machine that has never seen it.

use std::path::Path;
use std::process::Command;

use gitwyrm_lib::archive_repair::{diagnose, repair, ArchiveProblem};

fn cli_available() -> bool {
  let program = if cfg!(windows) { "openspec.cmd" } else { "openspec" };
  Command::new(program)
    .arg("--version")
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

fn run_archive(root: &Path, id: &str) -> (bool, String) {
  let program = if cfg!(windows) { "openspec.cmd" } else { "openspec" };
  let out = Command::new(program)
    .args(["archive", id, "--yes"])
    .current_dir(root)
    .env("OPENSPEC_TELEMETRY", "0")
    .output()
    .expect("archive should run");
  let mut text = String::from_utf8_lossy(&out.stdout).to_string();
  text.push_str(&String::from_utf8_lossy(&out.stderr));
  (out.status.success(), text)
}

/// A minimal but valid change: one requirement, one scenario, all tasks done.
fn seed_change(root: &Path, id: &str) {
  let change = root.join("openspec/changes").join(id);
  std::fs::create_dir_all(change.join("specs/demo-cap")).unwrap();
  std::fs::create_dir_all(root.join("openspec/specs")).unwrap();
  std::fs::write(
    change.join("proposal.md"),
    "# Change: Demo\n\n## Why\n\nBecause.\n\n## What Changes\n\n- A thing\n\n## Impact\n\n- Affected specs: demo-cap\n",
  )
  .unwrap();
  std::fs::write(change.join("tasks.md"), "# Tasks\n\n## 1. Build\n\n- [x] 1.1 Do it\n").unwrap();
  std::fs::write(
    change.join("specs/demo-cap/spec.md"),
    "# demo-cap Spec Delta\n\n## ADDED Requirements\n\n### Requirement: A thing\n\n\
The system SHALL do a thing.\n\n#### Scenario: It happens\n\n- WHEN asked\n- THEN it happens\n",
  )
  .unwrap();
}

#[test]
fn a_leftover_archive_folder_is_recognised_repaired_and_archived() {
  if !cli_available() {
    eprintln!("skipping: the openspec CLI is not installed");
    return;
  }

  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  seed_change(root, "add-demo-thing");

  // The wreckage a half-finished archive leaves: an empty folder at the exact
  // name the CLI would choose. That name carries today's date, so it is learned
  // from a first run rather than hardcoded -- a fixed date simply would not
  // collide, which is what made an earlier version of this test pass wrongly.
  let (ok, output) = run_archive(root, "add-demo-thing");
  assert!(ok, "the first archive should succeed:\n{output}");
  let archived_name = std::fs::read_dir(root.join("openspec/changes/archive"))
    .unwrap()
    .flatten()
    .map(|e| e.file_name().to_string_lossy().into_owned())
    .find(|n| n.ends_with("add-demo-thing"))
    .expect("the change should have been archived somewhere");

  // Put it back the way a half-finished run leaves it: change restored to
  // changes/, an empty skeleton squatting on the destination.
  std::fs::remove_dir_all(root.join("openspec/changes/archive").join(&archived_name)).unwrap();
  seed_change(root, "add-demo-thing");
  let leftover = root.join("openspec/changes/archive").join(&archived_name);
  std::fs::create_dir_all(leftover.join("specs/demo-cap")).unwrap();

  let (ok, output) = run_archive(root, "add-demo-thing");
  assert!(!ok, "archive should refuse while the folder is in the way:\n{output}");

  // 1. The app works out what happened, without a human reading the transcript.
  let d = diagnose("add-demo-thing", &output);
  assert!(
    matches!(d.problem, ArchiveProblem::LeftoverArchive { .. }),
    "expected a leftover-archive diagnosis, got {:?} from:\n{output}",
    d.problem
  );
  assert!(d.fix_label.is_some(), "this failure is meant to be fixable in-app");

  // 2. The repair the user accepted.
  repair(root, &d.problem).expect("repair should succeed");
  assert!(!leftover.exists(), "the empty folder should be gone");

  // 3. And the archive it was blocking now goes through.
  let (ok, output) = run_archive(root, "add-demo-thing");
  assert!(ok, "archive should succeed after the repair:\n{output}");
  assert!(
    !root.join("openspec/changes/add-demo-thing").exists(),
    "the change should have moved out of changes/"
  );
}

/// The guard that matters most: a real archived change must never be deleted by
/// a repair, even when its name is what the CLI is complaining about.
#[test]
fn a_repair_refuses_to_touch_an_archive_holding_real_files() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  let real = root
    .join("openspec/changes/archive")
    .join("2026-01-01-add-demo-thing");
  std::fs::create_dir_all(&real).unwrap();
  std::fs::write(real.join("proposal.md"), "# somebody's work\n").unwrap();

  let problem = ArchiveProblem::LeftoverArchive {
    path: "openspec/changes/archive/2026-01-01-add-demo-thing".into(),
  };
  assert!(
    repair(root, &problem).is_err(),
    "a non-empty archive must be refused"
  );
  assert!(
    real.join("proposal.md").exists(),
    "the archived work must still be there"
  );
}
