//! End-to-end exercise of the openspec write path against a real folder.
//!
//! The unit tests cover the writer's byte-preservation guarantee in isolation.
//! This drives the round trip the UI actually performs -- parse, toggle, re-parse
//! -- so a change to either half that breaks the other shows up here.

use std::path::Path;

use gitwyrm_lib::{openspec_parse as parse, openspec_write as write};

/// Builds a minimal but realistic openspec tree in a temp dir.
fn fixture() -> tempfile::TempDir {
  let dir = tempfile::tempdir().unwrap();
  let change = dir.path().join("openspec").join("changes").join("do-a-thing");
  std::fs::create_dir_all(change.join("specs").join("core")).unwrap();
  std::fs::write(
    change.join("proposal.md"),
    "# Change: Do a thing\n\n## Why\n\nBecause it helps.\n\n## What Changes\n\n- The thing\n\n## Impact\n\n- Affected specs: core\n",
  )
  .unwrap();
  // CRLF on purpose: this is what a Windows editor writes, and the toggle must
  // not quietly convert the file.
  std::fs::write(
    change.join("tasks.md"),
    "# Tasks\r\n\r\n## 1. Build\r\n\r\n- [ ] 1.1 First\r\n- [ ] 1.2 Second\r\n\r\n## 2. Verify\r\n\r\n- [ ] 2.1 Check\r\n",
  )
  .unwrap();
  std::fs::write(
    change.join("specs").join("core").join("spec.md"),
    "# core Spec Delta\n\n## ADDED Requirements\n\n### Requirement: A thing\n\nIt SHALL work.\n\n#### Scenario: Basic\n\n- WHEN used\n- THEN it works\n",
  )
  .unwrap();
  dir
}

fn openspec(dir: &Path) -> std::path::PathBuf {
  gitwyrm_lib::openspec_dir(dir).expect("fixture has openspec/")
}

#[test]
fn parse_toggle_reparse_round_trip() {
  let tmp = fixture();
  let dir = openspec(tmp.path());

  let changes = parse::parse_changes_dir(&dir);
  assert_eq!(changes.len(), 1);
  let change = &changes[0];
  assert_eq!(change.id, "do-a-thing");
  assert_eq!(change.title, "Do a thing");
  assert_eq!(change.progress.done, 0);
  assert_eq!(change.progress.total, 3);
  assert_eq!(change.status, parse::ChangeStatus::Draft);

  // Tick the first task the way the UI does: by the line the parser reported.
  let first = &change.tasks[0];
  let tasks_md = write::tasks_path(&dir, &change.id);
  let before = std::fs::read_to_string(&tasks_md).unwrap();
  assert_eq!(
    write::toggle_task_line(&tasks_md, first.line, true).unwrap(),
    write::ToggleOutcome::Toggled
  );

  // Re-parse: progress moved, and nothing else about the file did.
  let after_change = parse::parse_change_dir(&dir.join("changes").join("do-a-thing")).unwrap();
  assert_eq!(after_change.progress.done, 1);
  assert_eq!(after_change.progress.percent, 33);
  assert_eq!(after_change.status, parse::ChangeStatus::InBuild);

  let after = std::fs::read_to_string(&tasks_md).unwrap();
  assert_eq!(after.len(), before.len(), "only a marker should change");
  assert_eq!(
    after.matches("\r\n").count(),
    before.matches("\r\n").count(),
    "CRLF line endings must survive the write"
  );

  // Finish the build group: the change waits on review, not on more building.
  write::toggle_task_line(&tasks_md, change.tasks[1].line, true).unwrap();
  let mid = parse::parse_change_dir(&dir.join("changes").join("do-a-thing")).unwrap();
  assert_eq!(mid.status, parse::ChangeStatus::NeedsReview);

  // Everything done, and there is a delta to merge: ready to archive.
  write::toggle_task_line(&tasks_md, change.tasks[2].line, true).unwrap();
  let done = parse::parse_change_dir(&dir.join("changes").join("do-a-thing")).unwrap();
  assert_eq!(done.progress.done, 3);
  assert_eq!(done.progress.percent, 100);
  assert_eq!(done.status, parse::ChangeStatus::ReadyToArchive);

  // Unticking rolls the state back.
  write::toggle_task_line(&tasks_md, change.tasks[2].line, false).unwrap();
  let rolled = parse::parse_change_dir(&dir.join("changes").join("do-a-thing")).unwrap();
  assert_eq!(rolled.progress.done, 2);
  assert_eq!(rolled.status, parse::ChangeStatus::NeedsReview);
}

#[test]
fn scaffolded_change_is_immediately_visible_and_a_draft() {
  let tmp = fixture();
  let dir = openspec(tmp.path());

  let result = write::scaffold_change(&dir, "Warn Before Delete", "Ask before losing work.").unwrap();
  assert_eq!(result.id, "warn-before-delete");

  let changes = parse::parse_changes_dir(&dir);
  assert_eq!(changes.len(), 2, "the new change shows up in the list");
  let new = changes
    .iter()
    .find(|c| c.id == "warn-before-delete")
    .expect("scaffolded change parses");
  // A fresh scaffold is a draft with no tasks -- never "0 of N done".
  assert!(new.progress.is_draft);
  assert_eq!(new.status, parse::ChangeStatus::Draft);
  assert!(new.proposal.why.contains("Ask before losing work."));

  // A second attempt at the same name is refused, so the UI can ask for another
  // while the name field is still on screen.
  assert!(write::scaffold_change(&dir, "warn-before-delete", "").is_err());
}

#[test]
fn external_edits_are_picked_up_on_reparse() {
  let tmp = fixture();
  let dir = openspec(tmp.path());
  let change_dir = dir.join("changes").join("do-a-thing");

  // Simulate an agent appending a task group and ticking one, as would happen
  // during an AI run or a terminal edit.
  let tasks_md = change_dir.join("tasks.md");
  let mut text = std::fs::read_to_string(&tasks_md).unwrap();
  text.push_str("\r\n## 3. Ship\r\n\r\n- [x] 3.1 Release notes\r\n");
  std::fs::write(&tasks_md, text).unwrap();

  let reparsed = parse::parse_change_dir(&change_dir).unwrap();
  assert_eq!(reparsed.progress.total, 4);
  assert_eq!(reparsed.progress.done, 1);
  let last = reparsed.tasks.last().unwrap();
  assert_eq!(last.group, "3. Ship");
  assert!(last.done);

  // The parser's line numbers still point at real checkboxes after the edit.
  let lines: Vec<String> = std::fs::read_to_string(&tasks_md)
    .unwrap()
    .lines()
    .map(str::to_string)
    .collect();
  for task in &reparsed.tasks {
    let line = &lines[task.line as usize - 1];
    assert!(line.contains("[ ]") || line.contains("[x]"), "bad line: {line:?}");
  }
}

#[test]
fn a_repo_without_openspec_reports_nothing() {
  let tmp = tempfile::tempdir().unwrap();
  assert!(gitwyrm_lib::openspec_dir(tmp.path()).is_none());
  // And the list path degrades to empty rather than erroring, which is what the
  // command relies on to show no Specs UI at all.
  assert!(parse::parse_changes_dir(tmp.path()).is_empty());
}
