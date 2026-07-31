//! Finishing a run: the commit message it drafts, and the trailers that make
//! the work attributable afterwards.
//!
//! The end of a run is the trust moment. Nothing here commits anything -- this
//! composes the message and the caller hands it to the user, who clicks. That
//! split is the whole point: an agent that could commit on its own would be a
//! different and much harder thing to trust inside a git client.

use crate::git::trailers;

/// House prefixes, in the order they are tried.
///
/// The changelog generator reads these, so a drafted message has to use one or
/// the work lands in the wrong section (or none). See the commit convention in
/// the project's own docs.
const NEW_HINTS: [&str; 6] = ["add ", "create ", "introduce ", "implement ", "new ", "support "];
const FIX_HINTS: [&str; 6] = ["fix ", "fixes ", "repair ", "correct ", "resolve ", "stop "];

/// Pick `new:`, `fixes:`, or `improved:` from what the task says it does.
///
/// A guess, and deliberately a cheap one: the user edits the message before
/// committing, so being wrong costs a word. Defaulting to `improved:` is the
/// safe miss -- it is the section that reads as "something changed" rather than
/// overclaiming a new feature or implying there was a bug.
pub fn prefix_for(task_text: &str) -> &'static str {
  let t = task_text.trim().to_ascii_lowercase();
  if FIX_HINTS.iter().any(|h| t.starts_with(h)) {
    "fixes"
  } else if NEW_HINTS.iter().any(|h| t.starts_with(h)) {
    "new"
  } else {
    "improved"
  }
}

/// Strip a leading task number like "1.2 " so it does not end up in the subject.
fn without_task_number(task_text: &str) -> &str {
  let trimmed = task_text.trim();
  let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
  // Only treat it as a number when something was actually removed and what
  // follows is a separator -- "3D rendering" must keep its 3.
  if rest.len() < trimmed.len() && rest.starts_with(' ') {
    rest.trim_start()
  } else {
    trimmed
  }
}

/// Lowercase the first letter of a sentence that starts with an ordinary
/// capitalised word, so "Add the thing" reads as "new: add the thing".
///
/// Left alone when the first *word* carries another capital inside it, which is
/// what distinguishes a name or an acronym from a normal sentence opener:
/// "GitHub" and "API" keep their shape, "Add" does not. Checking only the second
/// letter is not enough -- it lowercases "GitHub" into "gitHub", since its
/// second letter is lowercase.
fn decapitalize(text: &str) -> String {
  let mut chars = text.chars();
  let Some(first) = chars.next() else {
    return String::new();
  };
  if !first.is_uppercase() {
    return text.to_string();
  }
  let first_word = text.split_whitespace().next().unwrap_or("");
  let has_inner_capital = first_word.chars().skip(1).any(char::is_uppercase);
  if has_inner_capital {
    text.to_string()
  } else {
    first.to_lowercase().collect::<String>() + chars.as_str()
  }
}

/// Subject line for a finished task: `<prefix>: <task text>`.
///
/// Kept under a sensible width by truncating on a word boundary rather than
/// mid-word, since this is the line every log listing shows.
pub fn subject_for(task_text: &str) -> String {
  const MAX: usize = 72;
  let body = decapitalize(without_task_number(task_text));
  let prefix = prefix_for(&body);
  let head = format!("{prefix}: ");
  let room = MAX.saturating_sub(head.len());

  if body.chars().count() <= room {
    return format!("{head}{body}");
  }
  // Cut at the last space that fits, so the subject ends on a whole word.
  let mut cut = String::new();
  for word in body.split_whitespace() {
    let candidate = if cut.is_empty() {
      word.to_string()
    } else {
      format!("{cut} {word}")
    };
    if candidate.chars().count() > room.saturating_sub(1) {
      break;
    }
    cut = candidate;
  }
  if cut.is_empty() {
    cut = body.chars().take(room.saturating_sub(1)).collect();
  }
  format!("{head}{cut}…")
}

/// The full message a finished run offers for approval.
///
/// Both trailers are always present on a run's commit: `Spec:` links it to the
/// change so the graph can draw its chip, and `Assisted-by:` is what makes the
/// ✦ marker honest. Neither is optional -- a run's commit that did not say it
/// came from a run would be the one piece of history nobody could audit.
pub fn commit_message(task_text: &str, change_id: &str, provider: &str) -> String {
  let mut message = subject_for(task_text);
  if !change_id.trim().is_empty() {
    message = trailers::upsert(&message, trailers::SPEC_KEY, change_id.trim());
  }
  if !provider.trim().is_empty() {
    message = trailers::upsert(&message, trailers::ASSISTED_KEY, provider.trim());
  }
  message
}

/// The History line a committed run leaves behind.
///
/// Names the provider and how many steps the user approved, because "the AI did
/// it" is not an audit trail -- how much a human agreed to is the part worth
/// recording.
///
/// Not yet called: History is derived from commits (`openspec::history`), and
/// surfacing this line needs the approval count carried through the session,
/// which is task 2.2 of `add-ai-run-completion`. Kept and tested here so the
/// wording is settled before the plumbing arrives.
#[allow(dead_code)]
pub fn history_line(task_number: u32, provider: &str, short_sha: &str, approvals: u32) -> String {
  let approved = match approvals {
    0 => "you approved nothing".to_string(),
    1 => "you approved 1 step".to_string(),
    n => format!("you approved {n} steps"),
  };
  format!("Task {task_number} finished by AI ({provider}) - commit {short_sha} · {approved}")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_task_that_adds_something_reads_as_new() {
    assert_eq!(prefix_for("Add a settings toggle"), "new");
    assert_eq!(prefix_for("Create the room view"), "new");
    assert_eq!(prefix_for("Implement the queue"), "new");
  }

  #[test]
  fn a_task_that_fixes_something_reads_as_a_fix() {
    assert_eq!(prefix_for("Fix the crash on open"), "fixes");
    assert_eq!(prefix_for("Stop the double toast"), "fixes");
  }

  /// The safe miss. Anything unrecognised is a change, not a new feature and
  /// not a bug that implies there was one.
  #[test]
  fn anything_else_is_an_improvement() {
    assert_eq!(prefix_for("Rename the panel"), "improved");
    assert_eq!(prefix_for("Wire the tab up"), "improved");
    assert_eq!(prefix_for(""), "improved");
  }

  #[test]
  fn the_task_number_does_not_reach_the_subject() {
    assert_eq!(subject_for("1.2 Add a toggle"), "new: add a toggle");
    assert_eq!(subject_for("3. Fix the crash"), "fixes: fix the crash");
  }

  /// "3D" is not a task number.
  #[test]
  fn a_leading_digit_that_is_part_of_a_word_survives() {
    assert!(subject_for("3D preview support").contains("3D preview"));
  }

  #[test]
  fn a_capitalised_first_word_is_lowercased() {
    assert_eq!(subject_for("Add a toggle"), "new: add a toggle");
  }

  /// Names and acronyms keep their shape.
  #[test]
  fn an_acronym_or_name_is_left_alone() {
    assert!(subject_for("GitHub sync for issues").contains("GitHub"));
    assert!(subject_for("API keys in settings").contains("API"));
  }

  #[test]
  fn a_long_subject_is_cut_on_a_word_boundary() {
    let long = "Add a really quite extraordinarily long description of the thing that was done here today";
    let subject = subject_for(long);
    assert!(subject.chars().count() <= 72, "got {} chars", subject.chars().count());
    assert!(subject.ends_with('…'));
    // Cut between words, never mid-word.
    let body = subject.trim_end_matches('…');
    assert!(!body.ends_with(' '));
    assert!(long.to_lowercase().contains(body.split(": ").nth(1).unwrap()));
  }

  #[test]
  fn a_run_commit_carries_both_trailers() {
    let message = commit_message("Add a toggle", "add-ai-run-completion", "GitHub Copilot");
    assert_eq!(
      trailers::spec_id(&message).as_deref(),
      Some("add-ai-run-completion")
    );
    assert_eq!(trailers::assisted_by(&message).as_deref(), Some("GitHub Copilot"));
    assert!(message.starts_with("new: add a toggle"));
  }

  /// One trailer block, one of each key, however many times this is called.
  #[test]
  fn composing_twice_does_not_duplicate_trailers() {
    let once = commit_message("Add a toggle", "add-thing", "Copilot");
    assert_eq!(once.matches("Spec:").count(), 1);
    assert_eq!(once.matches("Assisted-by:").count(), 1);
  }

  #[test]
  fn a_missing_provider_or_change_omits_that_trailer_rather_than_writing_an_empty_one() {
    let no_provider = commit_message("Add a toggle", "add-thing", "");
    assert!(trailers::assisted_by(&no_provider).is_none());
    assert_eq!(trailers::spec_id(&no_provider).as_deref(), Some("add-thing"));

    let neither = commit_message("Add a toggle", "  ", "");
    assert_eq!(neither, "new: add a toggle");
  }

  #[test]
  fn the_history_line_counts_approvals_in_plain_language() {
    assert!(history_line(3, "Copilot", "a1b2c3d", 2).contains("you approved 2 steps"));
    assert!(history_line(3, "Copilot", "a1b2c3d", 1).contains("you approved 1 step"));
    assert!(history_line(3, "Copilot", "a1b2c3d", 0).contains("you approved nothing"));
  }

  #[test]
  fn the_history_line_names_the_task_provider_and_commit() {
    let line = history_line(7, "GitHub Copilot", "deadbee", 4);
    assert!(line.contains("Task 7"));
    assert!(line.contains("GitHub Copilot"));
    assert!(line.contains("deadbee"));
  }
}
