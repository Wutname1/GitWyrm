//! Parses GitWyrm's own `openspec/` tree.
//!
//! The unit tests in `openspec::parse` use hand-written fixtures, which prove
//! the rules but not that they match how the OpenSpec CLI actually formats a
//! change. This repo carries a real 12-change plan that `openspec validate`
//! passes, so pointing the parser at it catches drift between our reader and the
//! tool's own conventions.
//!
//! Skipped (not failed) when the folder is absent, so a checkout without the
//! plan still runs the suite green.

use std::path::{Path, PathBuf};

/// The repository root, from the manifest dir (`src-tauri`).
fn repo_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .expect("src-tauri has a parent")
    .to_path_buf()
}

#[test]
fn parses_our_own_openspec_tree() {
  let root = repo_root();
  let Some(dir) = gitwyrm_lib::openspec_dir(&root) else {
    eprintln!("no openspec/ folder in {}; skipping", root.display());
    return;
  };

  let changes = gitwyrm_lib::openspec_parse::parse_changes_dir(&dir);
  assert!(
    changes.len() >= 10,
    "expected the full plan, parsed {} changes",
    changes.len()
  );

  // Every change should have parsed cleanly: our own tree is CLI-valid, so any
  // note here means the parser disagrees with the tool.
  for change in &changes {
    assert!(
      change.notes.is_empty(),
      "{} produced parse notes: {:?}",
      change.id,
      change.notes
    );
    assert!(!change.title.is_empty(), "{} has no title", change.id);
    // A title distinct from the id proves the `# Change:` heading was read.
    assert_ne!(change.title, change.id, "{} fell back to its id", change.id);
    assert!(
      !change.proposal.why.is_empty(),
      "{} has no Why section",
      change.id
    );
    assert!(change.updated > 0.0, "{} has no mtime", change.id);
  }

  // The foundation change (this one) is a known quantity: tasks in three
  // groups, one delta capability, and a design doc.
  let foundation = changes
    .iter()
    .find(|c| c.id == "add-openspec-foundation")
    .expect("add-openspec-foundation is in the tree");
  assert!(foundation.has_design);
  assert!(
    foundation.progress.total >= 10,
    "expected the full task list, got {}",
    foundation.progress.total
  );
  assert_eq!(foundation.deltas.len(), 1);
  assert_eq!(foundation.deltas[0].capability, "openspec-core");
  assert!(
    foundation.deltas[0].requirements.len() >= 6,
    "expected the core requirements, got {}",
    foundation.deltas[0].requirements.len()
  );
  // Scenarios are what make a requirement testable; they must survive parsing.
  assert!(
    foundation.deltas[0]
      .requirements
      .iter()
      .all(|r| !r.scenarios.is_empty()),
    "every requirement should carry at least one scenario"
  );

  // Task groups come through with their headings intact.
  let groups: Vec<&str> = foundation
    .tasks
    .iter()
    .map(|t| t.group.as_str())
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter()
    .collect();
  assert!(
    groups.iter().any(|g| g.contains("Backend")),
    "expected a Backend group, saw {groups:?}"
  );
  assert!(
    groups.iter().any(|g| g.contains("Verify")),
    "expected a Verify group, saw {groups:?}"
  );

  // A change we deliberately left as a draft (spec phase only) must read as one.
  let agent_room = changes
    .iter()
    .find(|c| c.id == "add-agent-room")
    .expect("add-agent-room is in the tree");
  assert_eq!(
    agent_room.status,
    gitwyrm_lib::openspec_parse::ChangeStatus::Draft,
    "a plan with no completed tasks is a draft"
  );

  // Line numbers must point at real checkbox lines: the writer targets them.
  let tasks_md = std::fs::read_to_string(
    dir.join("changes").join("add-openspec-foundation").join("tasks.md"),
  )
  .expect("tasks.md is readable");
  let lines: Vec<&str> = tasks_md.lines().collect();
  for task in &foundation.tasks {
    let line = lines
      .get(task.line as usize - 1)
      .unwrap_or_else(|| panic!("task {} points past the file", task.index));
    assert!(
      line.contains("- [ ]") || line.contains("- [x]"),
      "task {} points at a non-checkbox line: {line:?}",
      task.index
    );
  }
}
