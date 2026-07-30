//! The only code that modifies a user's openspec files.
//!
//! Two operations, both deliberately tiny: flip one checkbox in tasks.md, and
//! scaffold a new change folder. Everything else (validate, archive) is the
//! OpenSpec CLI's job.
//!
//! The rule that matters: a toggle rewrites exactly one bracket pair and leaves
//! every other byte alone -- line endings, indentation, trailing whitespace,
//! comments, the final newline. Users have these files open in editors and
//! agents write to them too; a "helpful" reformat would show up as a spurious
//! diff or clobber someone else's edit.

use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::openspec::draft::DraftedArtifact;

/// Result of asking to toggle a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ToggleOutcome {
  /// The checkbox was flipped and the file written.
  Toggled,
  /// The task was already in the requested state, so nothing was written.
  AlreadyThatWay,
  /// The line no longer holds a checkbox -- the file changed underneath us.
  /// The caller re-reads rather than guessing.
  LineMoved,
}

/// Replaces the checkbox marker on one line, preserving everything else.
/// Returns None when the line has no `[ ]`/`[x]` to change.
fn flip_line(line: &str, done: bool) -> Option<String> {
  // Find the first bracket pair; anything before it is bullet + indentation and
  // is copied verbatim.
  let open = line.find('[')?;
  let rest = &line[open..];
  let marker_len = if rest.starts_with("[ ]") || rest.starts_with("[x]") || rest.starts_with("[X]") {
    3
  } else {
    return None;
  };
  let marker = if done { "[x]" } else { "[ ]" };
  let mut out = String::with_capacity(line.len());
  out.push_str(&line[..open]);
  out.push_str(marker);
  out.push_str(&line[open + marker_len..]);
  Some(out)
}

/// True when the line already carries the requested state.
fn line_is_done(line: &str) -> Option<bool> {
  let open = line.find('[')?;
  let rest = &line[open..];
  if rest.starts_with("[ ]") {
    Some(false)
  } else if rest.starts_with("[x]") || rest.starts_with("[X]") {
    Some(true)
  } else {
    None
  }
}

/// Splits text into lines *with* their original terminators, so a file mixing
/// CRLF and LF (or missing a trailing newline) round-trips byte-for-byte.
fn split_keep_endings(text: &str) -> Vec<&str> {
  let mut out = Vec::new();
  let bytes = text.as_bytes();
  let mut start = 0;
  for (i, b) in bytes.iter().enumerate() {
    if *b == b'\n' {
      out.push(&text[start..=i]);
      start = i + 1;
    }
  }
  if start < text.len() {
    out.push(&text[start..]);
  }
  out
}

/// Toggles the checkbox on `line_number` (1-based) of `tasks_path` to `done`.
///
/// Targeting a line number rather than matching text is deliberate: two tasks
/// can share wording, and the parser already handed the UI the exact line. The
/// guard is that the line must still look like a checkbox; if the file moved on
/// (an agent inserted tasks, say) this reports `LineMoved` instead of writing to
/// the wrong place.
pub fn toggle_task_line(
  tasks_path: &Path,
  line_number: u32,
  done: bool,
) -> Result<ToggleOutcome, AppError> {
  let text = std::fs::read_to_string(tasks_path)?;
  let lines = split_keep_endings(&text);
  let idx = line_number.checked_sub(1).ok_or_else(|| {
    AppError::Other("task line numbers start at 1".to_string())
  })? as usize;
  let Some(line) = lines.get(idx) else {
    return Ok(ToggleOutcome::LineMoved);
  };

  // Separate the terminator so the rewrite cannot change CRLF into LF.
  let (content, ending) = match line.find('\n') {
    Some(pos) => (&line[..pos], &line[pos..]),
    None => (*line, ""),
  };
  let content_trimmed_cr = content.strip_suffix('\r').unwrap_or(content);
  let cr = if content.ends_with('\r') { "\r" } else { "" };

  match line_is_done(content_trimmed_cr) {
    None => return Ok(ToggleOutcome::LineMoved),
    Some(current) if current == done => return Ok(ToggleOutcome::AlreadyThatWay),
    Some(_) => {}
  }

  let Some(flipped) = flip_line(content_trimmed_cr, done) else {
    return Ok(ToggleOutcome::LineMoved);
  };

  let mut out = String::with_capacity(text.len() + 1);
  for (i, l) in lines.iter().enumerate() {
    if i == idx {
      out.push_str(&flipped);
      out.push_str(cr);
      out.push_str(ending);
    } else {
      out.push_str(l);
    }
  }
  std::fs::write(tasks_path, out)?;
  Ok(ToggleOutcome::Toggled)
}

/// A folder name safe to create and to show in a graph chip: lowercase,
/// alphanumeric plus single dashes. Returns None for a name that reduces to
/// nothing, so the caller can ask the user for a better one.
pub fn sanitize_change_id(raw: &str) -> Option<String> {
  let mut out = String::with_capacity(raw.len());
  let mut last_dash = true; // suppresses a leading dash
  for ch in raw.trim().chars() {
    let c = ch.to_ascii_lowercase();
    if c.is_ascii_alphanumeric() {
      out.push(c);
      last_dash = false;
    } else if !last_dash {
      out.push('-');
      last_dash = true;
    }
  }
  let trimmed = out.trim_end_matches('-').to_string();
  if trimmed.is_empty() {
    None
  } else {
    Some(trimmed)
  }
}

/// What a scaffold call produced.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ScaffoldResult {
  /// The id actually used (sanitized).
  pub id: String,
  /// Repo-relative paths written, in creation order.
  pub files: Vec<String>,
}

/// Write a reviewed draft to disk, creating only the artifacts passed in.
///
/// This is the *only* place a drafted change becomes files. Everything the user
/// skipped at review is simply absent from `artifacts`, so "Skip" is enforced by
/// there being nothing to write rather than by a flag this function has to
/// honour.
///
/// A failure part-way through removes the change folder entirely. A half-written
/// change would show up in the sidebar as a real one, and the user would have no
/// way to tell which files the AI actually produced.
pub fn create_drafted_change(
  openspec_dir: &Path,
  id: &str,
  artifacts: &[DraftedArtifact],
) -> Result<Vec<String>, AppError> {
  if artifacts.is_empty() {
    return Err(AppError::Other(
      "Nothing was kept, so there is nothing to create.".into(),
    ));
  }
  let id = sanitize_change_id(id)
    .ok_or_else(|| AppError::Other("that name has no letters or numbers in it".to_string()))?;
  let dir = openspec_dir.join("changes").join(&id);
  if dir.exists() {
    return Err(AppError::Other(format!("a change named {id} already exists")));
  }

  let written = write_artifacts(&dir, &id, artifacts);
  if written.is_err() {
    // Best effort: the folder was ours to begin with, created moments ago.
    let _ = std::fs::remove_dir_all(&dir);
  }
  written
}

/// The write half of `create_drafted_change`, split out so the caller can clean
/// up after any failure without repeating the error path at every `?`.
fn write_artifacts(
  dir: &Path,
  id: &str,
  artifacts: &[DraftedArtifact],
) -> Result<Vec<String>, AppError> {
  std::fs::create_dir_all(dir)?;
  let mut files = Vec::with_capacity(artifacts.len());
  for artifact in artifacts {
    let target = safe_join(dir, &artifact.path).ok_or_else(|| {
      AppError::Other(format!("{} is not a valid place for a file", artifact.path))
    })?;
    if let Some(parent) = target.parent() {
      std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, &artifact.content)?;
    files.push(format!("openspec/changes/{id}/{}", artifact.path.replace('\\', "/")));
  }
  Ok(files)
}

/// Resolve `relative` inside `base`, refusing anything that escapes it.
///
/// The paths come from a model's JSON, so `../` and absolute paths are a real
/// possibility rather than a theoretical one. Checked component by component
/// because the target does not exist yet, which rules out canonicalising it.
fn safe_join(base: &Path, relative: &str) -> Option<std::path::PathBuf> {
  let mut out = base.to_path_buf();
  let mut depth = 0usize;
  for component in Path::new(relative).components() {
    match component {
      std::path::Component::Normal(part) => {
        out.push(part);
        depth += 1;
      }
      // Anything else -- `..`, a root, a drive prefix -- could leave the folder.
      _ => return None,
    }
  }
  (depth > 0).then_some(out)
}

/// Add one delta file to an existing change, for the validate-fix loop.
///
/// Refuses to overwrite an existing delta: the user asked to *add* a missing
/// requirement, and silently replacing a spec file they had already written
/// would be the worst possible reading of that request.
pub fn add_delta(
  openspec_dir: &Path,
  change_id: &str,
  delta: &DraftedArtifact,
) -> Result<String, AppError> {
  let dir = openspec_dir.join("changes").join(change_id);
  if !dir.exists() {
    return Err(AppError::Other(format!("{change_id} is not a change here.")));
  }
  let target = safe_join(&dir, &delta.path)
    .ok_or_else(|| AppError::Other(format!("{} is not a valid place for a file", delta.path)))?;
  if target.exists() {
    return Err(AppError::Other(format!(
      "{} already exists. Edit it instead of adding another.",
      delta.path
    )));
  }
  if let Some(parent) = target.parent() {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::write(&target, &delta.content)?;
  Ok(format!(
    "openspec/changes/{change_id}/{}",
    delta.path.replace('\\', "/")
  ))
}

/// Creates `openspec/changes/<id>/` with template proposal.md and tasks.md.
///
/// Refuses an existing folder rather than merging into it: the caller surfaces
/// that as "pick another name" while the name field is still editable, so the
/// user is never stuck holding a draft with nowhere to put it.
pub fn scaffold_change(
  openspec_dir: &Path,
  raw_id: &str,
  description: &str,
) -> Result<ScaffoldResult, AppError> {
  let id = sanitize_change_id(raw_id)
    .ok_or_else(|| AppError::Other("that name has no letters or numbers in it".to_string()))?;
  let dir = openspec_dir.join("changes").join(&id);
  if dir.exists() {
    return Err(AppError::Other(format!(
      "a change named {id} already exists"
    )));
  }
  std::fs::create_dir_all(&dir)?;

  let desc = description.trim();
  let why = if desc.is_empty() { "Describe the problem this solves." } else { desc };
  let proposal = format!(
    "# Change: {id}\n\n## Why\n\n{why}\n\n## What Changes\n\n- \n\n## Impact\n\n- Affected specs: \n- Affected code: \n"
  );
  // The task template carries headings but no checkboxes. An empty `- [ ] 1.1`
  // line would parse as a real task, so a brand-new change would read "0 of 2
  // done" instead of the draft it actually is.
  let tasks =
    "# Tasks\n\n## 1. Build\n\n<!-- Add tasks as checkboxes: - [ ] 1.1 Do the thing -->\n\n## 2. Verify\n\n"
      .to_string();

  let proposal_path = dir.join("proposal.md");
  let tasks_path = dir.join("tasks.md");
  std::fs::write(&proposal_path, proposal)?;
  std::fs::write(&tasks_path, tasks)?;

  Ok(ScaffoldResult {
    id: id.clone(),
    files: vec![
      format!("openspec/changes/{id}/proposal.md"),
      format!("openspec/changes/{id}/tasks.md"),
    ],
  })
}

/// Path to a change's tasks.md, without checking that it exists.
pub fn tasks_path(openspec_dir: &Path, change_id: &str) -> PathBuf {
  openspec_dir.join("changes").join(change_id).join("tasks.md")
}

#[cfg(test)]
mod tests {
  use super::*;

  fn write_temp(name: &str, content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    (dir, path)
  }

  #[test]
  fn toggle_changes_only_the_one_checkbox() {
    let original = "# Tasks\n\n## 1. Build\n\n- [ ] 1.1 First thing   \n- [ ] 1.2 Second thing\n";
    let (_dir, path) = write_temp("tasks.md", original);

    let outcome = toggle_task_line(&path, 5, true).unwrap();
    assert_eq!(outcome, ToggleOutcome::Toggled);

    let after = std::fs::read_to_string(&path).unwrap();
    let expected = "# Tasks\n\n## 1. Build\n\n- [x] 1.1 First thing   \n- [ ] 1.2 Second thing\n";
    assert_eq!(after, expected);
    // Byte-for-byte apart from the single marker: same length, one difference.
    assert_eq!(after.len(), original.len());
    let diffs = original
      .bytes()
      .zip(after.bytes())
      .filter(|(a, b)| a != b)
      .count();
    assert_eq!(diffs, 1, "exactly one byte should change");
  }

  #[test]
  fn preserves_crlf_and_missing_final_newline() {
    let original = "## 1. Build\r\n- [ ] one\r\n- [ ] two";
    let (_dir, path) = write_temp("tasks.md", original);

    toggle_task_line(&path, 2, true).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, "## 1. Build\r\n- [x] one\r\n- [ ] two");
    // No newline was appended to the last line.
    assert!(!after.ends_with('\n'));

    // And the last line, which has no terminator at all, can be toggled.
    toggle_task_line(&path, 3, true).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, "## 1. Build\r\n- [x] one\r\n- [x] two");
  }

  #[test]
  fn preserves_unusual_indentation_and_bullets() {
    let original = "   * [ ] indented star\n\t+ [ ] tabbed plus\n";
    let (_dir, path) = write_temp("tasks.md", original);
    toggle_task_line(&path, 1, true).unwrap();
    toggle_task_line(&path, 2, true).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, "   * [x] indented star\n\t+ [x] tabbed plus\n");
  }

  #[test]
  fn untick_works_and_accepts_capital_x() {
    let (_dir, path) = write_temp("tasks.md", "- [X] done thing\n");
    assert_eq!(toggle_task_line(&path, 1, false).unwrap(), ToggleOutcome::Toggled);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "- [ ] done thing\n");
  }

  #[test]
  fn no_write_when_already_in_that_state() {
    let original = "- [x] already done\n";
    let (_dir, path) = write_temp("tasks.md", original);
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(
      toggle_task_line(&path, 1, true).unwrap(),
      ToggleOutcome::AlreadyThatWay
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    // Untouched: same mtime, so no editor sees a phantom save.
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
  }

  #[test]
  fn reports_line_moved_instead_of_writing_wrong_line() {
    let original = "# Tasks\n\nprose, not a checkbox\n";
    let (_dir, path) = write_temp("tasks.md", original);
    assert_eq!(toggle_task_line(&path, 3, true).unwrap(), ToggleOutcome::LineMoved);
    // Past the end of the file.
    assert_eq!(toggle_task_line(&path, 99, true).unwrap(), ToggleOutcome::LineMoved);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
  }

  #[test]
  fn sanitizes_change_ids() {
    assert_eq!(sanitize_change_id("Warn Before Delete").unwrap(), "warn-before-delete");
    assert_eq!(sanitize_change_id("  add__openspec  foundation ").unwrap(), "add-openspec-foundation");
    assert_eq!(sanitize_change_id("Fix: the thing!").unwrap(), "fix-the-thing");
    assert_eq!(sanitize_change_id("already-fine").unwrap(), "already-fine");
    assert!(sanitize_change_id("   ").is_none());
    assert!(sanitize_change_id("!!!").is_none());
  }

  #[test]
  fn scaffolds_a_change_and_refuses_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let result = scaffold_change(dir.path(), "Warn Before Delete", "Ask before losing work.").unwrap();
    assert_eq!(result.id, "warn-before-delete");
    assert_eq!(result.files.len(), 2);

    let proposal = std::fs::read_to_string(
      dir.path().join("changes").join("warn-before-delete").join("proposal.md"),
    )
    .unwrap();
    assert!(proposal.contains("Ask before losing work."));
    assert!(proposal.contains("## Why"));

    // The scaffold is immediately parseable by our own parser.
    let parsed = super::super::parse::parse_change_dir(
      &dir.path().join("changes").join("warn-before-delete"),
    )
    .unwrap();
    assert_eq!(parsed.id, "warn-before-delete");
    // A fresh scaffold is a draft, not "0 of N done" -- the template ships
    // headings only, so nothing counts as a task until the user writes one.
    assert_eq!(parsed.progress.total, 0);
    assert!(parsed.progress.is_draft);
    assert_eq!(parsed.status, super::super::parse::ChangeStatus::Draft);

    assert!(scaffold_change(dir.path(), "warn-before-delete", "").is_err());
  }

  fn artifact(path: &str, content: &str) -> DraftedArtifact {
    DraftedArtifact { path: path.into(), content: content.into() }
  }

  #[test]
  fn creates_only_the_artifacts_it_is_given() {
    let dir = tempfile::tempdir().unwrap();
    let change = dir.path().join("changes").join("add-thing");

    // The user kept the proposal and the delta but skipped tasks.
    let files = create_drafted_change(
      dir.path(),
      "add-thing",
      &[
        artifact("proposal.md", "# Change: Add a thing\n"),
        artifact("specs/core/spec.md", "# core Spec Delta\n"),
      ],
    )
    .unwrap();

    assert_eq!(files.len(), 2);
    assert!(change.join("proposal.md").exists());
    assert!(change.join("specs").join("core").join("spec.md").exists());
    assert!(
      !change.join("tasks.md").exists(),
      "a skipped artifact must not be written"
    );
  }

  #[test]
  fn refuses_to_overwrite_an_existing_change() {
    let dir = tempfile::tempdir().unwrap();
    create_drafted_change(dir.path(), "add-thing", &[artifact("proposal.md", "# a\n")]).unwrap();
    assert!(
      create_drafted_change(dir.path(), "add-thing", &[artifact("proposal.md", "# b\n")]).is_err()
    );
    // The original survived the refused second call.
    let body =
      std::fs::read_to_string(dir.path().join("changes").join("add-thing").join("proposal.md"))
        .unwrap();
    assert_eq!(body, "# a\n");
  }

  #[test]
  fn nothing_is_left_behind_when_an_artifact_path_is_unusable() {
    let dir = tempfile::tempdir().unwrap();
    let result = create_drafted_change(
      dir.path(),
      "add-thing",
      &[
        artifact("proposal.md", "# ok\n"),
        // Escaping the change folder must fail the whole create.
        artifact("../../escape.md", "nope\n"),
      ],
    );
    assert!(result.is_err(), "a path outside the change folder must be refused");
    assert!(
      !dir.path().join("changes").join("add-thing").exists(),
      "a failed create must not leave a partial change behind"
    );
    assert!(!dir.path().join("escape.md").exists());
  }

  #[test]
  fn adds_a_delta_to_an_existing_change() {
    let dir = tempfile::tempdir().unwrap();
    create_drafted_change(dir.path(), "add-thing", &[artifact("proposal.md", "# a\n")]).unwrap();

    let written = add_delta(
      dir.path(),
      "add-thing",
      &artifact("specs/core/spec.md", "# core Spec Delta\n"),
    )
    .unwrap();

    assert_eq!(written, "openspec/changes/add-thing/specs/core/spec.md");
    assert!(dir
      .path()
      .join("changes/add-thing/specs/core/spec.md")
      .exists());
  }

  /// The fix loop adds a *missing* requirement. Replacing a spec the user
  /// already wrote would destroy work they never offered up.
  #[test]
  fn adding_a_delta_never_overwrites_one() {
    let dir = tempfile::tempdir().unwrap();
    create_drafted_change(
      dir.path(),
      "add-thing",
      &[
        artifact("proposal.md", "# a\n"),
        artifact("specs/core/spec.md", "# mine\n"),
      ],
    )
    .unwrap();

    assert!(add_delta(
      dir.path(),
      "add-thing",
      &artifact("specs/core/spec.md", "# theirs\n")
    )
    .is_err());

    let body =
      std::fs::read_to_string(dir.path().join("changes/add-thing/specs/core/spec.md")).unwrap();
    assert_eq!(body, "# mine\n", "the user's own delta must survive");
  }

  #[test]
  fn adding_a_delta_to_a_missing_change_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(add_delta(dir.path(), "nope", &artifact("specs/core/spec.md", "# x\n")).is_err());
  }

  #[test]
  fn a_delta_cannot_escape_its_change_folder() {
    let dir = tempfile::tempdir().unwrap();
    create_drafted_change(dir.path(), "add-thing", &[artifact("proposal.md", "# a\n")]).unwrap();
    assert!(add_delta(dir.path(), "add-thing", &artifact("../../escape.md", "x\n")).is_err());
    assert!(!dir.path().join("escape.md").exists());
  }

  #[test]
  fn an_empty_artifact_list_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    // "Create" with everything skipped writes nothing at all, so it should not
    // produce an empty folder either.
    assert!(create_drafted_change(dir.path(), "add-thing", &[]).is_err());
    assert!(!dir.path().join("changes").join("add-thing").exists());
  }
}
