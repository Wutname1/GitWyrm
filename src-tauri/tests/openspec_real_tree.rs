//! Parses GitWyrm's own `openspec/` tree.
//!
//! The unit tests in `openspec::parse` use hand-written fixtures, which prove
//! the rules but not that they match how the OpenSpec CLI actually formats a
//! change. Pointing the parser at a real CLI-valid tree catches drift between
//! our reader and the tool's own conventions.
//!
//! What this must NOT do is assert the *contents* of that tree. The plan is
//! working state: changes are drafted, worked on and archived constantly, so a
//! test naming a particular change, or requiring some number of them, fails the
//! moment someone archives one -- reporting a broken parser when the parser is
//! fine. Everything below is an invariant that holds for any tree the CLI would
//! accept, whatever is in flight at the time.
//!
//! Skipped (not failed) when the folder is absent or empty, so a checkout
//! without the plan still runs the suite green.

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
  if changes.is_empty() {
    // Every change archived, or a checkout without the plan. Nothing to read,
    // and that is a legitimate state rather than a parser failure.
    eprintln!("no active changes in {}; skipping", dir.display());
    return;
  }

  for change in &changes {
    // Our own tree is CLI-valid, so any note means the parser disagrees with
    // the tool about a file the tool accepts.
    assert!(
      change.notes.is_empty(),
      "{} produced parse notes: {:?}",
      change.id,
      change.notes
    );
    assert!(!change.title.is_empty(), "{} has no title", change.id);
    // A title distinct from the id proves the `# Change:` heading was read
    // rather than the id being used as a fallback.
    assert_ne!(change.title, change.id, "{} fell back to its id", change.id);
    assert!(
      !change.proposal.why.is_empty(),
      "{} has no Why section",
      change.id
    );
    assert!(change.updated > 0.0, "{} has no mtime", change.id);

    // Progress must agree with the tasks it was counted from, whatever they are.
    assert_eq!(
      change.progress.total as usize,
      change.tasks.len(),
      "{} progress total disagrees with its task list",
      change.id
    );
    assert!(
      change.progress.done <= change.progress.total,
      "{} reports more tasks done than exist",
      change.id
    );

    // Every task's line must point at a real checkbox: the writer targets these
    // line numbers, so an off-by-one here would tick the wrong box.
    let tasks_md = dir.join("changes").join(&change.id).join("tasks.md");
    if let Ok(body) = std::fs::read_to_string(&tasks_md) {
      let lines: Vec<&str> = body.lines().collect();
      for task in &change.tasks {
        let line = lines
          .get(task.line as usize - 1)
          .unwrap_or_else(|| panic!("{} task {} points past the file", change.id, task.index));
        assert!(
          line.contains("- [ ]") || line.contains("- [x]"),
          "{} task {} points at a non-checkbox line: {line:?}",
          change.id,
          task.index
        );
      }
      // Task groups carry their `## ` heading, not an empty string.
      for task in &change.tasks {
        assert!(
          !task.group.is_empty(),
          "{} task {} has no group heading",
          change.id,
          task.index
        );
      }
    }

    // Deltas name a capability and carry requirements; a delta parsed down to
    // nothing means the requirement reader broke.
    for delta in &change.deltas {
      assert!(
        !delta.capability.is_empty(),
        "{} has a delta with no capability",
        change.id
      );
      assert!(
        !delta.requirements.is_empty(),
        "{} delta {} parsed no requirements",
        change.id,
        delta.file
      );
      // Scenarios are what make a requirement testable, and `openspec validate
      // --strict` requires at least one, so a requirement without any means we
      // dropped it in parsing.
      for req in &delta.requirements {
        assert!(
          !req.name.is_empty(),
          "{} delta {} has an unnamed requirement",
          change.id,
          delta.file
        );
        assert!(
          !req.scenarios.is_empty(),
          "{} requirement {:?} parsed no scenarios",
          change.id,
          req.name
        );
      }
    }
  }

  // At least one change in a real tree should exercise each of the optional
  // parts, otherwise this test is not proving much about the harder paths.
  // Stated over the whole tree rather than about a named change, so archiving
  // any single change cannot break it.
  assert!(
    changes.iter().any(|c| !c.deltas.is_empty()),
    "no change in the tree has a spec delta; the delta parser is untested here"
  );
  assert!(
    changes.iter().any(|c| !c.tasks.is_empty()),
    "no change in the tree has tasks; the task parser is untested here"
  );
}
