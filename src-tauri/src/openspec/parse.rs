//! Parses an `openspec/` folder into the typed shapes the UI renders.
//!
//! Everything here is pure: it takes text (or a directory) and returns data. No
//! git, no app state, no writes. That keeps the whole parser unit-testable
//! against fixture strings, which matters because the files come from users and
//! other tools and are frequently half-written.
//!
//! The guiding rule is that a malformed file degrades instead of failing. A
//! proposal missing its headings still lists as a change; a tasks.md with odd
//! formatting still contributes whatever checkboxes it has. Nothing in here
//! returns an error for content it cannot understand -- it returns less data and
//! records a note.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

/// A change's lifecycle state, derived from its tasks and deltas rather than
/// stored anywhere. One function owns the rules so both windows agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ChangeStatus {
  /// No tasks yet, or none done: still being written.
  Draft,
  /// Some tasks done, build work outstanding.
  InBuild,
  /// Build groups are complete but a review/verify group is still open.
  NeedsReview,
  /// Every task done and at least one delta: ready for `openspec archive`.
  ReadyToArchive,
}

/// What kind of spec edit a delta describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "UPPERCASE")]
pub enum DeltaKind {
  Added,
  Modified,
  Removed,
  Renamed,
}

impl DeltaKind {
  fn from_heading(word: &str) -> Option<Self> {
    match word.to_ascii_uppercase().as_str() {
      "ADDED" => Some(Self::Added),
      "MODIFIED" => Some(Self::Modified),
      "REMOVED" => Some(Self::Removed),
      "RENAMED" => Some(Self::Renamed),
      _ => None,
    }
  }
}

/// One checkbox line from tasks.md.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SpecTask {
  /// Zero-based index across the whole file, and the handle used to toggle it.
  pub index: u32,
  /// The `## 1. Backend` heading this task sits under, without the `##`.
  /// Empty when the file has no headings.
  pub group: String,
  /// The task text with any leading `1.2` numbering left in place -- it is the
  /// author's own numbering and handoffs quote it.
  pub text: String,
  pub done: bool,
  /// 1-based line number in tasks.md, so the writer can target this exact line.
  pub line: u32,
}

/// One requirement inside a delta, with its scenarios kept as text.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SpecRequirement {
  pub name: String,
  /// Prose under the requirement heading, before the first scenario.
  pub text: String,
  /// Scenario blocks as `(name, body)` -- rendered verbatim, never interpreted.
  pub scenarios: Vec<(String, String)>,
}

/// One spec-delta file: which capability it edits and what it changes.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SpecDelta {
  /// Capability name from the path: `specs/<capability>/spec.md`.
  pub capability: String,
  /// Repo-relative path, shown in the UI so the user can find the file.
  pub file: String,
  pub kind: DeltaKind,
  pub requirements: Vec<SpecRequirement>,
}

/// The three narrative sections of proposal.md. Each is raw markdown; when a
/// heading is missing the field is empty and `raw` carries the whole file so the
/// UI can still show something real.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct Proposal {
  pub why: String,
  pub what_changes: String,
  pub impact: String,
  /// The unparsed file, always populated. The UI falls back to this when the
  /// three sections came up empty.
  pub raw: String,
}

/// Progress for one change, straight from its checkboxes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub struct SpecProgress {
  pub done: u32,
  pub total: u32,
  /// 0-100, rounded. Zero when there are no tasks.
  pub percent: u32,
  /// True when the change has no tasks at all -- report as a draft rather than
  /// "0% of nothing".
  pub is_draft: bool,
}

/// One change folder, fully parsed.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SpecChange {
  /// Folder name under `openspec/changes/`, e.g. `add-openspec-foundation`.
  pub id: String,
  /// First `# ` heading of proposal.md with any `Change: ` prefix removed,
  /// falling back to the id.
  pub title: String,
  pub status: ChangeStatus,
  pub progress: SpecProgress,
  pub proposal: Proposal,
  pub has_design: bool,
  pub tasks: Vec<SpecTask>,
  pub deltas: Vec<SpecDelta>,
  /// Newest mtime across the change's files, as a unix timestamp in seconds.
  /// Drives "updated 4m ago" without a second stat pass in the UI.
  pub updated: f64,
  /// Plain-language notes about what could not be parsed. Surfaced quietly, not
  /// as an error dialog.
  pub notes: Vec<String>,
}

/// Splits markdown into `(heading_text, body)` sections at a given heading
/// level. Content before the first heading is dropped by design -- callers that
/// need it keep the raw text.
fn sections(md: &str, level: usize) -> Vec<(String, String)> {
  let prefix = format!("{} ", "#".repeat(level));
  let mut out: Vec<(String, String)> = Vec::new();
  let mut current: Option<(String, Vec<&str>)> = None;
  let mut in_fence = false;

  for line in md.lines() {
    // Never treat a `#` inside a fenced code block as a heading.
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
      in_fence = !in_fence;
    }
    let is_heading = !in_fence
      && line.starts_with(&prefix)
      // `### x` is not a level-2 heading; require the next char to not be '#'.
      && !line[prefix.len() - 1..].starts_with("##");

    if is_heading {
      if let Some((name, body)) = current.take() {
        out.push((name, body.join("\n").trim().to_string()));
      }
      current = Some((line[prefix.len()..].trim().to_string(), Vec::new()));
    } else if let Some((_, body)) = current.as_mut() {
      body.push(line);
    }
  }
  if let Some((name, body)) = current {
    out.push((name, body.join("\n").trim().to_string()));
  }
  out
}

/// The first `# ` heading, with a `Change:` prefix stripped.
fn first_title(md: &str) -> Option<String> {
  md.lines()
    .find(|l| l.starts_with("# "))
    .map(|l| l[2..].trim())
    .map(|t| t.strip_prefix("Change:").unwrap_or(t).trim().to_string())
    .filter(|t| !t.is_empty())
}

/// Parses proposal.md. Section matching is case-insensitive and prefix-based so
/// "What Changes", "What changes", and "What Changes (draft)" all land.
pub fn parse_proposal(md: &str) -> Proposal {
  let mut proposal = Proposal { raw: md.to_string(), ..Default::default() };
  for (name, body) in sections(md, 2) {
    let key = name.trim().to_ascii_lowercase();
    if key.starts_with("why") {
      proposal.why = body;
    } else if key.starts_with("what") {
      proposal.what_changes = body;
    } else if key.starts_with("impact") {
      proposal.impact = body;
    }
  }
  proposal
}

/// Recognises a task line and returns `(done, text)`. Accepts `-`, `*`, and `+`
/// bullets, any indentation, and `[x]`/`[X]` for done.
fn parse_task_line(line: &str) -> Option<(bool, String)> {
  let trimmed = line.trim_start();
  let rest = trimmed
    .strip_prefix("- ")
    .or_else(|| trimmed.strip_prefix("* "))
    .or_else(|| trimmed.strip_prefix("+ "))?;
  let rest = rest.trim_start();
  let (done, tail) = if let Some(t) = rest.strip_prefix("[ ]") {
    (false, t)
  } else if let Some(t) = rest.strip_prefix("[x]").or_else(|| rest.strip_prefix("[X]")) {
    (true, t)
  } else {
    return None;
  };
  let text = tail.trim();
  // A checkbox with no text is not a task anyone can act on.
  if text.is_empty() {
    return None;
  }
  Some((done, text.to_string()))
}

/// Parses tasks.md into a flat, indexed list. Continuation lines (an indented
/// line after a task that is not itself a checkbox) are appended to that task's
/// text, because authors wrap long tasks and the handoff needs the whole thing.
/// Whether the task at `index` is ticked.
///
/// Takes the file's own text so a caller can re-read it cheaply between turns:
/// an AI run watches for its target task being ticked, and re-parsing one file
/// is far less work than reloading the whole change.
///
/// `index` is the zero-based position used by [`SpecTask::index`], not the
/// author's `1.2` numbering -- those disagree whenever a file skips or repeats
/// a number, which real plans do.
pub fn task_is_ticked(md: &str, index: u32) -> bool {
  parse_tasks(md)
    .into_iter()
    .find(|t| t.index == index)
    .map(|t| t.done)
    .unwrap_or(false)
}

pub fn parse_tasks(md: &str) -> Vec<SpecTask> {
  let mut tasks: Vec<SpecTask> = Vec::new();
  let mut group = String::new();
  let mut in_fence = false;

  for (i, line) in md.lines().enumerate() {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
      in_fence = !in_fence;
      continue;
    }
    if in_fence {
      continue;
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
      group = rest.trim().to_string();
      continue;
    }
    if let Some((done, text)) = parse_task_line(line) {
      tasks.push(SpecTask {
        index: tasks.len() as u32,
        group: group.clone(),
        text,
        done,
        line: (i + 1) as u32,
      });
      continue;
    }
    // Wrapped continuation of the previous task: indented, non-empty, and not a
    // new list item or heading.
    let is_indented = line.starts_with("  ") || line.starts_with('\t');
    if is_indented && !trimmed.is_empty() && !trimmed.starts_with('#') {
      if let Some(last) = tasks.last_mut() {
        last.text.push(' ');
        last.text.push_str(trimmed);
      }
    }
  }
  tasks
}

/// Parses one spec delta file. A single file may carry several
/// `## ADDED Requirements`-style sections; each becomes its own delta so the UI
/// can badge them separately.
pub fn parse_delta_file(capability: &str, file: &str, md: &str) -> Vec<SpecDelta> {
  let mut deltas = Vec::new();
  for (heading, body) in sections(md, 2) {
    let Some(word) = heading.split_whitespace().next() else { continue };
    let Some(kind) = DeltaKind::from_heading(word) else { continue };

    let requirements = sections(&body, 3)
      .into_iter()
      .map(|(name, req_body)| {
        let name = name
          .strip_prefix("Requirement:")
          .unwrap_or(&name)
          .trim()
          .to_string();
        let scenarios: Vec<(String, String)> = sections(&req_body, 4)
          .into_iter()
          .map(|(s_name, s_body)| {
            let s_name = s_name
              .strip_prefix("Scenario:")
              .unwrap_or(&s_name)
              .trim()
              .to_string();
            (s_name, s_body)
          })
          .collect();
        // Prose before the first scenario heading.
        let text = req_body
          .split("\n#### ")
          .next()
          .unwrap_or_default()
          .trim()
          .to_string();
        SpecRequirement { name, text, scenarios }
      })
      .collect();

    deltas.push(SpecDelta {
      capability: capability.to_string(),
      file: file.to_string(),
      kind,
      requirements,
    });
  }
  deltas
}

/// Progress from a task list.
pub fn progress_of(tasks: &[SpecTask]) -> SpecProgress {
  let total = tasks.len() as u32;
  let done = tasks.iter().filter(|t| t.done).count() as u32;
  if total == 0 {
    return SpecProgress { done: 0, total: 0, percent: 0, is_draft: true };
  }
  // Round rather than truncate so 7/10 is 70 and 1/3 is 33.
  let percent = ((done as f64 / total as f64) * 100.0).round() as u32;
  SpecProgress { done, total, percent, is_draft: false }
}

/// True when a group heading names a review/verify phase rather than build work.
fn is_review_group(group: &str) -> bool {
  let g = group.to_ascii_lowercase();
  ["review", "verify", "verification", "test", "qa", "sign-off", "signoff"]
    .iter()
    .any(|word| g.contains(word))
}

/// Derives a change's status. Kept as one function (with tests) because both
/// windows, the sidebar, and the graph chip all display it.
pub fn status_of(tasks: &[SpecTask], delta_count: usize) -> ChangeStatus {
  if tasks.is_empty() {
    return ChangeStatus::Draft;
  }
  let done = tasks.iter().filter(|t| t.done).count();
  if done == 0 {
    return ChangeStatus::Draft;
  }
  if done == tasks.len() {
    // "Ready to archive" promises `openspec archive` will work, and archive
    // needs something to merge. Without a delta it is complete but not ready.
    return if delta_count > 0 { ChangeStatus::ReadyToArchive } else { ChangeStatus::InBuild };
  }
  // Everything except review/verify work is finished: the change is waiting on
  // a human, not on more building.
  let build_open = tasks
    .iter()
    .any(|t| !t.done && !is_review_group(&t.group));
  if build_open {
    ChangeStatus::InBuild
  } else {
    ChangeStatus::NeedsReview
  }
}

/// Newest mtime under `dir` as unix seconds, or 0 when nothing can be read.
/// One shallow-recursive pass; change folders hold a handful of files.
fn newest_mtime(dir: &Path) -> f64 {
  fn walk(dir: &Path, newest: &mut f64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
      let path = entry.path();
      if path.is_dir() {
        walk(&path, newest);
        continue;
      }
      if let Ok(meta) = entry.metadata() {
        if let Ok(modified) = meta.modified() {
          if let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH) {
            let secs = since.as_secs_f64();
            if secs > *newest {
              *newest = secs;
            }
          }
        }
      }
    }
  }
  let mut newest = 0.0;
  walk(dir, &mut newest);
  newest
}

/// Reads a file, returning None for anything unreadable (missing, permissions,
/// non-UTF-8). Callers treat None as "that part isn't there".
fn read(path: &Path) -> Option<String> {
  std::fs::read_to_string(path).ok()
}

/// Collects the spec deltas under a change's `specs/` folder.
/// Layout: `specs/<capability>/spec.md`, one capability per folder.
fn parse_change_deltas(change_dir: &Path, notes: &mut Vec<String>) -> Vec<SpecDelta> {
  let specs_dir = change_dir.join("specs");
  let Ok(entries) = std::fs::read_dir(&specs_dir) else { return Vec::new() };
  let mut deltas = Vec::new();
  let mut capabilities: Vec<(String, std::path::PathBuf)> = entries
    .flatten()
    .filter(|e| e.path().is_dir())
    .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
    .collect();
  // Directory order is filesystem-dependent; sort so the UI is stable.
  capabilities.sort_by(|a, b| a.0.cmp(&b.0));

  for (capability, dir) in capabilities {
    let spec_path = dir.join("spec.md");
    let Some(text) = read(&spec_path) else {
      notes.push(format!("specs/{capability}/spec.md could not be read"));
      continue;
    };
    let rel = format!("specs/{capability}/spec.md");
    let parsed = parse_delta_file(&capability, &rel, &text);
    if parsed.is_empty() {
      notes.push(format!(
        "{rel} has no ADDED, MODIFIED, or REMOVED requirements section"
      ));
    }
    deltas.extend(parsed);
  }
  deltas
}

/// Parses one change folder. Returns None only when the path is not a directory
/// -- every other problem becomes a note on an otherwise-usable change.
pub fn parse_change_dir(change_dir: &Path) -> Option<SpecChange> {
  if !change_dir.is_dir() {
    return None;
  }
  let id = change_dir.file_name()?.to_string_lossy().into_owned();
  let mut notes = Vec::new();

  let proposal_text = read(&change_dir.join("proposal.md"));
  if proposal_text.is_none() {
    notes.push("proposal.md is missing".to_string());
  }
  let proposal = parse_proposal(proposal_text.as_deref().unwrap_or_default());
  if proposal_text.is_some() && proposal.why.is_empty() && proposal.what_changes.is_empty() {
    notes.push("proposal.md has no Why or What Changes section".to_string());
  }

  let tasks_text = read(&change_dir.join("tasks.md"));
  let tasks = parse_tasks(tasks_text.as_deref().unwrap_or_default());

  let deltas = parse_change_deltas(change_dir, &mut notes);

  let title = proposal_text
    .as_deref()
    .and_then(first_title)
    .unwrap_or_else(|| id.clone());

  Some(SpecChange {
    id,
    title,
    status: status_of(&tasks, deltas.len()),
    progress: progress_of(&tasks),
    proposal,
    has_design: change_dir.join("design.md").is_file(),
    tasks,
    deltas,
    updated: newest_mtime(change_dir),
    notes,
  })
}

/// Parses every active change under `openspec/changes/`, newest first.
/// `changes/archive/` is skipped: archived changes are loaded on demand.
pub fn parse_changes_dir(openspec_dir: &Path) -> Vec<SpecChange> {
  let changes_dir = openspec_dir.join("changes");
  let Ok(entries) = std::fs::read_dir(&changes_dir) else { return Vec::new() };
  let mut changes: Vec<SpecChange> = entries
    .flatten()
    .map(|e| e.path())
    .filter(|p| p.is_dir())
    .filter(|p| {
      p.file_name()
        .map(|n| n != "archive" && !n.to_string_lossy().starts_with('.'))
        .unwrap_or(false)
    })
    .filter_map(|p| parse_change_dir(&p))
    .collect();
  // Newest first, with the id as a stable tiebreaker for equal mtimes (common
  // when a whole plan is written in one go).
  changes.sort_by(|a, b| {
    b.updated
      .partial_cmp(&a.updated)
      .unwrap_or(std::cmp::Ordering::Equal)
      .then_with(|| a.id.cmp(&b.id))
  });
  changes
}

/// Ids of archived changes, newest first. Cheap: no file parsing.
pub fn archived_ids(openspec_dir: &Path) -> Vec<String> {
  let archive = openspec_dir.join("changes").join("archive");
  let Ok(entries) = std::fs::read_dir(&archive) else { return Vec::new() };
  let mut ids: Vec<(String, f64)> = entries
    .flatten()
    .filter(|e| e.path().is_dir())
    .map(|e| {
      let updated = e
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
      (e.file_name().to_string_lossy().into_owned(), updated)
    })
    .collect();
  ids.sort_by(|a, b| {
    b.1
      .partial_cmp(&a.1)
      .unwrap_or(std::cmp::Ordering::Equal)
      .then_with(|| a.0.cmp(&b.0))
  });
  ids.into_iter().map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn task(group: &str, done: bool) -> SpecTask {
    SpecTask {
      index: 0,
      group: group.into(),
      text: "x".into(),
      done,
      line: 1,
    }
  }

  #[test]
  fn parses_proposal_sections() {
    let md = "# Change: Do a thing\n\n## Why\n\nBecause.\n\n## What Changes\n\n- One\n- Two\n\n## Impact\n\nAffected specs: core\n";
    let p = parse_proposal(md);
    assert_eq!(p.why, "Because.");
    assert_eq!(p.what_changes, "- One\n- Two");
    assert_eq!(p.impact, "Affected specs: core");
    assert_eq!(first_title(md).unwrap(), "Do a thing");
  }

  #[test]
  fn proposal_without_headings_keeps_raw() {
    let md = "just some prose with no headings at all";
    let p = parse_proposal(md);
    assert!(p.why.is_empty());
    assert_eq!(p.raw, md);
  }

  #[test]
  fn heading_variants_still_match() {
    let p = parse_proposal("## why\ntext\n## What changes (draft)\nmore\n## Impact\nx");
    assert_eq!(p.why, "text");
    assert_eq!(p.what_changes, "more");
    assert_eq!(p.impact, "x");
  }

  #[test]
  fn parses_tasks_with_groups() {
    let md = "# Tasks\n\n## 1. Backend\n\n- [x] 1.1 Do this\n- [ ] 1.2 Do that\n\n## 2. Verify\n\n- [ ] 2.1 Check it\n";
    let tasks = parse_tasks(md);
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].group, "1. Backend");
    assert!(tasks[0].done);
    assert_eq!(tasks[0].text, "1.1 Do this");
    assert_eq!(tasks[2].group, "2. Verify");
    // Line numbers point at the real line so the writer can target it.
    assert_eq!(tasks[0].line, 5);
    assert_eq!(tasks[2].line, 10);
  }

  #[test]
  fn tolerates_odd_task_formatting() {
    // Mixed bullets, capital X, extra indentation, and a wrapped line.
    let md = "* [X] alpha\n+ [ ] beta\n   - [ ] gamma\n      continued here\n- [] not a task\n- [ ]\n";
    let tasks = parse_tasks(md);
    assert_eq!(tasks.len(), 3);
    assert!(tasks[0].done);
    assert_eq!(tasks[2].text, "gamma continued here");
  }

  #[test]
  fn ignores_checkboxes_inside_code_fences() {
    let md = "- [ ] real\n\n```md\n- [x] example in docs\n```\n\n- [ ] also real\n";
    let tasks = parse_tasks(md);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].text, "real");
    assert_eq!(tasks[1].text, "also real");
  }

  #[test]
  fn progress_counts_and_rounds() {
    let mut tasks: Vec<SpecTask> = (0..10).map(|_| task("g", false)).collect();
    for t in tasks.iter_mut().take(7) {
      t.done = true;
    }
    let p = progress_of(&tasks);
    assert_eq!((p.done, p.total, p.percent), (7, 10, 70));
    assert!(!p.is_draft);
  }

  #[test]
  fn no_tasks_is_a_draft_not_zero_percent() {
    let p = progress_of(&[]);
    assert!(p.is_draft);
    assert_eq!((p.done, p.total, p.percent), (0, 0, 0));
  }

  #[test]
  fn status_rules() {
    // No tasks at all.
    assert_eq!(status_of(&[], 1), ChangeStatus::Draft);
    // Tasks but none done.
    assert_eq!(status_of(&[task("1. Build", false)], 1), ChangeStatus::Draft);
    // Partly built.
    assert_eq!(
      status_of(&[task("1. Build", true), task("1. Build", false)], 1),
      ChangeStatus::InBuild
    );
    // Build done, verify open.
    assert_eq!(
      status_of(&[task("1. Build", true), task("3. Verify", false)], 1),
      ChangeStatus::NeedsReview
    );
    // All done with a delta.
    assert_eq!(status_of(&[task("1. Build", true)], 1), ChangeStatus::ReadyToArchive);
    // All done but nothing to merge: not archivable.
    assert_eq!(status_of(&[task("1. Build", true)], 0), ChangeStatus::InBuild);
  }

  #[test]
  fn parses_delta_requirements_and_scenarios() {
    let md = "# core Spec Delta\n\n## ADDED Requirements\n\n### Requirement: Detection\n\nGitWyrm SHALL detect it.\n\n#### Scenario: Missing folder\n\n- WHEN absent\n- THEN nothing shows\n\n#### Scenario: Appears later\n\n- WHEN created\n- THEN it shows\n\n## MODIFIED Requirements\n\n### Requirement: Rows\n\nChanged text.\n";
    let deltas = parse_delta_file("core", "specs/core/spec.md", md);
    assert_eq!(deltas.len(), 2);
    assert_eq!(deltas[0].kind, DeltaKind::Added);
    assert_eq!(deltas[0].requirements.len(), 1);
    assert_eq!(deltas[0].requirements[0].name, "Detection");
    assert_eq!(deltas[0].requirements[0].text, "GitWyrm SHALL detect it.");
    assert_eq!(deltas[0].requirements[0].scenarios.len(), 2);
    assert_eq!(deltas[0].requirements[0].scenarios[0].0, "Missing folder");
    assert_eq!(deltas[1].kind, DeltaKind::Modified);
  }

  #[test]
  fn delta_file_without_known_sections_yields_nothing() {
    let deltas = parse_delta_file("core", "specs/core/spec.md", "# Notes\n\n## Thoughts\n\ntext");
    assert!(deltas.is_empty());
  }

  #[test]
  fn review_group_detection() {
    assert!(is_review_group("3. Verify"));
    assert!(is_review_group("Testing"));
    assert!(is_review_group("QA pass"));
    assert!(!is_review_group("1. Backend"));
    assert!(!is_review_group("2. UI"));
  }

  #[test]
  fn parses_a_real_change_folder() {
    let dir = tempfile::tempdir().unwrap();
    let change = dir.path().join("changes").join("do-a-thing");
    std::fs::create_dir_all(change.join("specs").join("core")).unwrap();
    std::fs::write(
      change.join("proposal.md"),
      "# Change: Do a thing\n\n## Why\n\nBecause.\n\n## What Changes\n\n- One\n",
    )
    .unwrap();
    std::fs::write(
      change.join("tasks.md"),
      "## 1. Build\n\n- [x] 1.1 One\n- [ ] 1.2 Two\n",
    )
    .unwrap();
    std::fs::write(change.join("design.md"), "# Design\n").unwrap();
    std::fs::write(
      change.join("specs").join("core").join("spec.md"),
      "## ADDED Requirements\n\n### Requirement: Thing\n\nSHALL work.\n",
    )
    .unwrap();

    let parsed = parse_change_dir(&change).unwrap();
    assert_eq!(parsed.id, "do-a-thing");
    assert_eq!(parsed.title, "Do a thing");
    assert!(parsed.has_design);
    assert_eq!(parsed.progress.done, 1);
    assert_eq!(parsed.progress.total, 2);
    assert_eq!(parsed.status, ChangeStatus::InBuild);
    assert_eq!(parsed.deltas.len(), 1);
    assert!(parsed.notes.is_empty(), "notes: {:?}", parsed.notes);
    assert!(parsed.updated > 0.0);

    // The whole folder, skipping archive/.
    std::fs::create_dir_all(dir.path().join("changes").join("archive").join("old")).unwrap();
    let all = parse_changes_dir(dir.path());
    assert_eq!(all.len(), 1);
    assert_eq!(archived_ids(dir.path()), vec!["old".to_string()]);
  }

  #[test]
  fn missing_files_become_notes_not_failures() {
    let dir = tempfile::tempdir().unwrap();
    let change = dir.path().join("half-written");
    std::fs::create_dir_all(&change).unwrap();
    // Nothing inside at all.
    let parsed = parse_change_dir(&change).unwrap();
    assert_eq!(parsed.id, "half-written");
    // Falls back to the folder name when there is no title.
    assert_eq!(parsed.title, "half-written");
    assert!(parsed.progress.is_draft);
    assert_eq!(parsed.status, ChangeStatus::Draft);
    assert!(parsed.notes.iter().any(|n| n.contains("proposal.md is missing")));
  }
}
