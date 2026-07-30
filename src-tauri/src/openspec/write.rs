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
}
