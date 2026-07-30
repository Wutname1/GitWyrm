//! Preparing the working tree for a run, and putting it back afterwards.
//!
//! A run edits files in place, so the user's own uncommitted work has to be
//! out of the way first -- otherwise a kept run and a half-finished thought
//! end up mixed in the same diff, with no way to tell which was which.

use serde::{Deserialize, Serialize};
use specta::Type;

/// What was done to the tree before a run started.
///
/// Returned so the run can be undone precisely: restoring work that was never
/// set aside would be as wrong as leaving work stashed that was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Preparation {
  /// The tree was already clean. Nothing to restore.
  TreeWasClean,
  /// The user's uncommitted work was stashed. It is still in the stash list --
  /// visible, and recoverable by hand even if GitWyrm never gets to restore it.
  WorkSetAside { summary: String },
}

impl Preparation {
  /// Whether anything needs putting back when the run ends.
  pub fn needs_restore(&self) -> bool {
    matches!(self, Preparation::WorkSetAside { .. })
  }

  /// What the console tells the user before the run starts.
  pub fn summary(&self) -> String {
    match self {
      Preparation::TreeWasClean => "Your working folder is clean, so nothing was moved.".into(),
      Preparation::WorkSetAside { .. } => {
        "Your uncommitted changes were set aside first. They'll come back when this run \
         ends, and they're in your stash list in the meantime."
          .into()
      }
    }
  }
}

/// The message used for a run's stash, so it is recognisable in the stash list.
///
/// A user who finds it days later should be able to tell what made it without
/// remembering the run.
pub fn stash_message(task: &str) -> String {
  let trimmed = task.trim();
  let short: String = trimmed.chars().take(60).collect();
  if trimmed.chars().count() > 60 {
    format!("gitwyrm: your work, set aside before an AI task ({short}...)")
  } else {
    format!("gitwyrm: your work, set aside before an AI task ({short})")
  }
}

/// How a run ended, from the tree's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
  /// The user is keeping the run's changes.
  Kept,
  /// The run's changes are being thrown away.
  Discarded,
}

/// Whether the user's set-aside work can be restored now.
///
/// Restoring over the run's own edits would conflict, so a kept run leaves the
/// stash in place and says so. That is deliberately a smaller promise than
/// "always restored": a stash that stays visible is recoverable, where a failed
/// automatic merge is a mess the user did not ask for.
pub fn can_restore_now(prep: &Preparation, ending: Ending) -> Result<bool, String> {
  if !prep.needs_restore() {
    return Ok(false);
  }
  match ending {
    Ending::Discarded => Ok(true),
    Ending::Kept => Err(
      "Your own changes are still in the stash list. GitWyrm didn't put them back \
       automatically because the run changed some of the same files -- restore them \
       yourself when you're ready."
        .into(),
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_clean_tree_needs_no_restore() {
    let p = Preparation::TreeWasClean;
    assert!(!p.needs_restore());
    assert_eq!(can_restore_now(&p, Ending::Kept), Ok(false));
    assert_eq!(can_restore_now(&p, Ending::Discarded), Ok(false));
  }

  #[test]
  fn discarding_a_run_puts_the_users_work_back() {
    let p = Preparation::WorkSetAside { summary: "3 files".into() };
    assert_eq!(can_restore_now(&p, Ending::Discarded), Ok(true));
  }

  #[test]
  fn keeping_a_run_leaves_the_stash_and_says_so() {
    // The honest outcome: restoring over the run's edits would conflict, so we
    // do not promise it. The work is still in the stash list either way.
    let p = Preparation::WorkSetAside { summary: "3 files".into() };
    let err = can_restore_now(&p, Ending::Kept).unwrap_err();
    assert!(err.contains("stash list"), "{err}");
    assert!(err.contains("yourself"), "{err}");
  }

  #[test]
  fn the_stash_message_names_the_task() {
    let m = stash_message("Add a settings toggle");
    assert!(m.contains("Add a settings toggle"));
    assert!(m.starts_with("gitwyrm:"), "{m}");
  }

  #[test]
  fn a_long_task_is_shortened_not_dropped() {
    let long = "x".repeat(200);
    let m = stash_message(&long);
    assert!(m.contains("..."), "{m}");
    assert!(m.len() < 150, "stash messages should stay readable: {}", m.len());
  }

  #[test]
  fn preparation_summaries_avoid_git_jargon() {
    for p in [
      Preparation::TreeWasClean,
      Preparation::WorkSetAside { summary: "x".into() },
    ] {
      let s = p.summary();
      for jargon in ["HEAD", "index", "worktree", "reflog"] {
        assert!(!s.contains(jargon), "{s} contains {jargon}");
      }
      assert!(!s.is_empty());
    }
  }
}
