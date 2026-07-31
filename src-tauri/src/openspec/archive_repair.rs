//! Working out why an archive failed, and offering to put it right.
//!
//! Archiving shells out to the OpenSpec CLI, which reports problems as a wall
//! of transcript ending in one line that matters. Two of those problems are
//! ordinary, recoverable, and completely opaque to someone who did not write
//! the tool. This module names them, says what happened in plain language, and
//! describes a fix the user can accept.
//!
//! The rule this module exists to honour: **the app repairs, the user decides.**
//! Nothing here runs on its own. `diagnose` only looks; `repair` only runs when
//! the user has read the explanation and asked for it.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;

/// A reason not to archive a change yet, found before running anything.
///
/// These exist because GitWyrm passes `--yes` to the CLI, which is what stops
/// it prompting in a GUI -- and `--yes` also overrides the CLI's own
/// incomplete-task check. Without a check here, a change with half its tasks
/// open merges into the specs library and gets committed, with only a warning
/// nobody sees. Archiving is the one spec action that rewrites the library and
/// makes a commit, so it is the one that has to be sure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ArchiveBlock {
  /// Tasks remain unticked.
  TasksOpen { open: u32, total: u32 },
  /// No tasks were ever written, so nothing says the work happened.
  NoTasks,
  /// Nothing to merge: archiving would file the change away and add no
  /// requirements to the library at all.
  NoDeltas,
}

impl ArchiveBlock {
  /// One sentence naming what is not ready.
  pub fn summary(&self) -> String {
    match self {
      Self::TasksOpen { open, total } => format!(
        "{open} of {total} {} still open.",
        if *open == 1 { "task is" } else { "tasks are" }
      ),
      Self::NoTasks => "This change has no tasks.".into(),
      Self::NoDeltas => "This change has no requirements to file.".into(),
    }
  }

  /// What archiving anyway would actually do. The user is allowed to proceed --
  /// they know things the app does not -- but not without being told.
  pub fn detail(&self) -> String {
    match self {
      Self::TasksOpen { .. } => "Archiving files this change's requirements into your specs \
         and moves it out of the active list. The open tasks stay unticked, so the record \
         will say the work was finished when part of it was not."
        .into(),
      Self::NoTasks => "Nothing here records what was done or whether it is finished. \
         Archiving still files its requirements into your specs."
        .into(),
      Self::NoDeltas => "Archiving would move it out of the active list and add nothing to \
         your specs. That is right for a pure refactor or a docs change, and wrong if the \
         requirements were simply never written."
        .into(),
    }
  }
}

/// Check a change before archiving it.
///
/// `done`/`total` and `deltas` come from GitWyrm's own parse of the change, not
/// from the CLI -- the point is to decide *before* anything is merged.
pub fn check_ready(done: u32, total: u32, deltas: usize) -> Option<ArchiveBlock> {
  if total == 0 {
    return Some(ArchiveBlock::NoTasks);
  }
  if done < total {
    return Some(ArchiveBlock::TasksOpen { open: total - done, total });
  }
  if deltas == 0 {
    return Some(ArchiveBlock::NoDeltas);
  }
  None
}

/// A recognised reason an archive did not go through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ArchiveProblem {
  /// A previous archive merged the specs, created its folder, then stopped
  /// before moving anything into it. Every later attempt refuses because the
  /// destination exists.
  LeftoverArchive {
    /// The empty folder in the way, repo-relative.
    path: String,
  },
  /// A delta creates a capability but also carries MODIFIED or RENAMED
  /// requirements, which only make sense against a spec that already exists.
  ModifiedInNewSpec {
    /// The capability the delta would create.
    capability: String,
  },
  /// Recognised as a failure, but not as one of the above. The user gets the
  /// tool's own words and no false promise of a fix.
  Unrecognised,
}

/// What the app can tell the user, and what it offers to do about it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ArchiveDiagnosis {
  pub problem: ArchiveProblem,
  /// One sentence naming what went wrong, in the user's terms.
  pub summary: String,
  /// What happened and why, still in plain language. Two or three sentences.
  pub detail: String,
  /// Label for the button that fixes it, when a fix is possible.
  pub fix_label: Option<String>,
  /// Exactly what the fix would do, so accepting it is informed.
  pub fix_detail: Option<String>,
}

/// Read a failed archive's output and work out what went wrong.
///
/// `output` is the CLI's combined stdout and stderr. Matching is on the phrases
/// the tool actually prints; anything unfamiliar comes back `Unrecognised`
/// rather than being forced into a category, because a confidently wrong
/// diagnosis is worse than none.
pub fn diagnose(change_id: &str, output: &str) -> ArchiveDiagnosis {
  let text = output.to_ascii_lowercase();

  if text.contains("already exists") && text.contains("archive") {
    let path = archive_path_from(output).unwrap_or_else(|| {
      format!("openspec/changes/archive/<dated>-{change_id}")
    });
    return ArchiveDiagnosis {
      problem: ArchiveProblem::LeftoverArchive { path: path.clone() },
      summary: "An earlier archive of this change stopped half way.".into(),
      detail: format!(
        "The last attempt filed this change's requirements into your specs, then stopped \
         before moving the change itself. It left an empty folder at {path}, and every \
         attempt since has stopped because that folder is in the way."
      ),
      fix_label: Some("Remove the empty folder and archive".into()),
      fix_detail: Some(format!(
        "Deletes the empty folder at {path} and archives again. The folder holds no \
         files, so nothing of yours is lost."
      )),
    };
  }

  if text.contains("target spec does not exist") || text.contains("only added requirements") {
    let capability = capability_from(output).unwrap_or_else(|| "that capability".into());
    return ArchiveDiagnosis {
      problem: ArchiveProblem::ModifiedInNewSpec { capability: capability.clone() },
      summary: format!("This change edits a requirement that {capability} does not have yet."),
      detail: format!(
        "The change would create {capability} from scratch, but it also edits a \
         requirement inside it. A brand new capability can only add requirements, since \
         there is nothing there to edit. Usually this means the edited requirement \
         belongs to a capability that already exists."
      ),
      // No safe automatic fix: which capability the requirement belongs to is a
      // judgement about intent, and guessing wrong would move someone's
      // requirement into the wrong spec.
      fix_label: None,
      fix_detail: None,
    };
  }

  ArchiveDiagnosis {
    problem: ArchiveProblem::Unrecognised,
    summary: format!("{change_id} could not be archived."),
    detail: "The OpenSpec tool stopped without a reason GitWyrm recognises. Its own \
             message is below."
      .into(),
    fix_label: None,
    fix_detail: None,
  }
}

/// Pull the archive folder name out of "Archive 'x' already exists."
fn archive_path_from(output: &str) -> Option<String> {
  let start = output.find('\'')?;
  let rest = &output[start + 1..];
  let end = rest.find('\'')?;
  let name = &rest[..end];
  (!name.is_empty()).then(|| format!("openspec/changes/archive/{name}"))
}

/// Pull the capability name out of "<name>: target spec does not exist".
fn capability_from(output: &str) -> Option<String> {
  output.lines().find_map(|line| {
    let line = line.trim();
    let at = line.find(": target spec does not exist")?;
    let name = line[..at].trim();
    (!name.is_empty() && !name.contains(' ')).then(|| name.to_string())
  })
}

/// Whether `dir` holds no files at any depth. Empty directories do not count:
/// the leftover the CLI leaves behind is a skeleton of them.
fn holds_no_files(dir: &Path) -> bool {
  let Ok(entries) = std::fs::read_dir(dir) else {
    return false;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      if !holds_no_files(&path) {
        return false;
      }
    } else {
      return false;
    }
  }
  true
}

/// Carry out the fix the user accepted.
///
/// Deliberately conservative: it re-checks the condition it was told about
/// rather than trusting the diagnosis it was handed. A diagnosis can be minutes
/// old, and this deletes something.
pub fn repair(repo_root: &Path, problem: &ArchiveProblem) -> Result<(), AppError> {
  match problem {
    ArchiveProblem::LeftoverArchive { path } => {
      let target = repo_root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
      if !target.exists() {
        // Already gone -- treat as done rather than erroring on the good case.
        return Ok(());
      }
      if !target.is_dir() {
        return Err(AppError::Other(format!(
          "{path} is a file, not the empty folder GitWyrm expected. Nothing was removed."
        )));
      }
      // The one thing that makes this safe to do for the user.
      if !holds_no_files(&target) {
        return Err(AppError::Other(format!(
          "{path} has files in it, so it is not the leftover GitWyrm expected. \
           Nothing was removed."
        )));
      }
      std::fs::remove_dir_all(&target)?;
      Ok(())
    }
    ArchiveProblem::ModifiedInNewSpec { .. } | ArchiveProblem::Unrecognised => Err(
      AppError::Other("There is nothing GitWyrm can safely fix for this one.".into()),
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const LEFTOVER: &str = "Task status: Complete\nSpecs to update:\n  ai-provider: update\n\
Totals: + 0, ~ 0, - 0\nSpecs already in sync; no files changed.\n\
Error: Archive '2026-07-30-add-ai-provider-surface' already exists.";

  const MODIFIED_NEW: &str = "Task status: Complete\nSpecs to update:\n  app-windows: create\n\
app-windows: target spec does not exist; only ADDED requirements are allowed for new \
specs. MODIFIED and RENAMED operations require an existing spec.\n\
Aborted. No files were changed.";

  /// The check that matters most. `--yes` silences the CLI's own guard, so a
  /// half-finished change archives with exit 0 and a warning nobody reads --
  /// verified against the real CLI. This is the only thing standing between
  /// that and a merged, committed spec library.
  #[test]
  fn a_half_finished_change_is_blocked() {
    assert_eq!(
      check_ready(1, 2, 1),
      Some(ArchiveBlock::TasksOpen { open: 1, total: 2 })
    );
    assert_eq!(
      check_ready(9, 12, 3),
      Some(ArchiveBlock::TasksOpen { open: 3, total: 12 })
    );
  }

  #[test]
  fn a_change_with_no_tasks_is_blocked() {
    assert_eq!(check_ready(0, 0, 1), Some(ArchiveBlock::NoTasks));
  }

  /// Archiving with nothing to merge is legal but almost never intended, so it
  /// asks rather than refusing outright.
  #[test]
  fn a_change_with_no_requirements_is_blocked() {
    assert_eq!(check_ready(3, 3, 0), Some(ArchiveBlock::NoDeltas));
  }

  #[test]
  fn a_finished_change_with_requirements_passes() {
    assert_eq!(check_ready(3, 3, 1), None);
    assert_eq!(check_ready(12, 12, 6), None);
  }

  #[test]
  fn every_block_says_what_is_wrong_and_what_would_happen() {
    for block in [
      ArchiveBlock::TasksOpen { open: 3, total: 12 },
      ArchiveBlock::NoTasks,
      ArchiveBlock::NoDeltas,
    ] {
      let summary = block.summary();
      let detail = block.detail();
      assert!(!summary.is_empty() && !detail.is_empty());
      // Plain language: no CLI flags or tool tokens leaking into the copy.
      for text in [&summary, &detail] {
        assert!(!text.contains("--yes"), "leaked a flag: {text}");
        assert!(!text.contains("delta header"), "leaked a tool term: {text}");
      }
    }
  }

  #[test]
  fn the_open_task_count_reads_naturally_for_one() {
    let one = ArchiveBlock::TasksOpen { open: 1, total: 2 }.summary();
    assert!(one.contains("1 of 2 task is still open"), "{one}");
    let many = ArchiveBlock::TasksOpen { open: 3, total: 12 }.summary();
    assert!(many.contains("3 of 12 tasks are still open"), "{many}");
  }

  #[test]
  fn recognises_a_leftover_archive_folder_and_names_it() {
    let d = diagnose("add-ai-provider-surface", LEFTOVER);
    assert_eq!(
      d.problem,
      ArchiveProblem::LeftoverArchive {
        path: "openspec/changes/archive/2026-07-30-add-ai-provider-surface".into()
      }
    );
    assert!(d.fix_label.is_some(), "this one is safely fixable");
    // The explanation has to say what is lost, which is nothing.
    assert!(d.fix_detail.unwrap().contains("nothing of yours is lost"));
  }

  #[test]
  fn recognises_a_modified_requirement_in_a_new_spec() {
    let d = diagnose("add-window-state-memory", MODIFIED_NEW);
    assert_eq!(
      d.problem,
      ArchiveProblem::ModifiedInNewSpec { capability: "app-windows".into() }
    );
    assert!(d.summary.contains("app-windows"));
    // No fix offered: which capability the requirement belongs to is a
    // judgement about intent, and guessing would move it somewhere wrong.
    assert!(d.fix_label.is_none());
  }

  #[test]
  fn an_unfamiliar_failure_is_not_forced_into_a_category() {
    let d = diagnose("add-thing", "Something else went wrong entirely.");
    assert_eq!(d.problem, ArchiveProblem::Unrecognised);
    assert!(d.fix_label.is_none());
  }

  #[test]
  fn every_diagnosis_speaks_plainly() {
    for output in [LEFTOVER, MODIFIED_NEW, "unknown"] {
      let d = diagnose("add-thing", output);
      for text in [&d.summary, &d.detail] {
        assert!(!text.contains("MODIFIED"), "leaked a tool token: {text}");
        assert!(!text.contains("ENOENT"), "leaked an error code: {text}");
        assert!(!text.is_empty());
      }
    }
  }

  #[test]
  fn repair_removes_an_empty_leftover() {
    let dir = tempfile::tempdir().unwrap();
    let leftover = dir.path().join("openspec/changes/archive/2026-01-01-add-thing/specs/core");
    std::fs::create_dir_all(&leftover).unwrap();

    let problem = ArchiveProblem::LeftoverArchive {
      path: "openspec/changes/archive/2026-01-01-add-thing".into(),
    };
    repair(dir.path(), &problem).unwrap();
    assert!(!dir
      .path()
      .join("openspec/changes/archive/2026-01-01-add-thing")
      .exists());
  }

  /// The guard that matters. A folder with real files in it is somebody's
  /// archived change, not a leftover skeleton.
  #[test]
  fn repair_refuses_to_delete_a_folder_holding_files() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("openspec/changes/archive/2026-01-01-add-thing");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("proposal.md"), "# real work\n").unwrap();

    let problem = ArchiveProblem::LeftoverArchive {
      path: "openspec/changes/archive/2026-01-01-add-thing".into(),
    };
    assert!(repair(dir.path(), &problem).is_err());
    assert!(real.join("proposal.md").exists(), "the user's file must survive");
  }

  #[test]
  fn repairing_an_already_clean_state_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let problem = ArchiveProblem::LeftoverArchive {
      path: "openspec/changes/archive/gone".into(),
    };
    assert!(repair(dir.path(), &problem).is_ok());
  }

  #[test]
  fn nothing_is_offered_for_problems_with_no_safe_fix() {
    let dir = tempfile::tempdir().unwrap();
    assert!(repair(dir.path(), &ArchiveProblem::Unrecognised).is_err());
    assert!(repair(
      dir.path(),
      &ArchiveProblem::ModifiedInNewSpec { capability: "x".into() }
    )
    .is_err());
  }

  #[test]
  fn a_nested_empty_skeleton_counts_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("a/b/c");
    std::fs::create_dir_all(&deep).unwrap();
    assert!(holds_no_files(&dir.path().join("a")));
    std::fs::write(deep.join("f.md"), "x").unwrap();
    assert!(!holds_no_files(&dir.path().join("a")));
  }
}
