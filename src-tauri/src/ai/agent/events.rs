//! What a run tells the console, as it happens.
//!
//! Every event carries its own plain-language sentence. The console renders
//! what it is given rather than translating an enum into words itself, so the
//! same run reads identically wherever it is shown -- the run tab, the Desk, a
//! notification.
//!
//! Nothing here mentions a provider, a protocol, or a token. If a sentence
//! would only make sense to someone who knows what ACP is, it is the wrong
//! sentence.

use serde::{Deserialize, Serialize};
use specta::Type;

/// One row in the activity stream.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunEvent {
  /// The AI said what it intends to do next.
  Planned { summary: String },
  /// A file was read. Shown because "it looked at X" explains later edits.
  ReadFile { path: String, summary: String },
  /// A file was written, with counts for the row's +/- display.
  EditedFile { path: String, added: u32, removed: u32, summary: String },
  /// A directory listing.
  ListedDirectory { path: String, summary: String },
  /// One of the project's own checks ran.
  CheckRan { name: String, passed: bool, summary: String },
  /// The AI produced prose. Streamed, so this may arrive in pieces.
  Said { text: String },
  /// A step could not be taken. Not fatal on its own -- the AI is told and can
  /// try another way.
  Refused { summary: String },
  /// The run is waiting on the user. Nothing else runs until it is answered.
  GateOpened { gate: Gate },
  /// The run ended, for any reason.
  Finished { outcome: RunOutcome, summary: String },
}

/// Something the AI wants to do that needs the user to say yes.
///
/// Only side effects beyond in-repo edits and the project's own checks reach
/// here. Editing source files and running tests are the job, not a gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Gate {
  /// Add or remove a library.
  Dependency { name: String, summary: String },
  /// Run an install or any package-manager command.
  Install { command: String, summary: String },
  /// Reach the network.
  Network { target: String, summary: String },
  /// Delete files.
  Delete { paths: Vec<String>, summary: String },
  /// Anything outside the repository.
  OutsideRepo { path: String, summary: String },
  /// A tool the bounded set does not include. Raised rather than executed, so
  /// an unknown tool can never become an arbitrary command.
  UnknownTool { name: String, summary: String },
}

impl Gate {
  /// The sentence the card shows. Names the consequence, not the mechanism.
  pub fn summary(&self) -> &str {
    match self {
      Gate::Dependency { summary, .. }
      | Gate::Install { summary, .. }
      | Gate::Network { summary, .. }
      | Gate::Delete { summary, .. }
      | Gate::OutsideRepo { summary, .. }
      | Gate::UnknownTool { summary, .. } => summary,
    }
  }
}

/// What the user chose at a gate.
///
/// Exactly three, matching the card. There is deliberately no "don't ask
/// again": a remembered approval is a decision made once and then applied to
/// situations the user never saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GateChoice {
  AllowOnce,
  /// The AI is told no and visibly adapts, rather than the run ending.
  FindAnotherWay,
  StopRun,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RunOutcome {
  /// The task's checkbox was ticked.
  Finished,
  /// Ran out of turns.
  OutOfSteps,
  /// The user stopped it.
  Stopped,
  /// The AI stopped for its own reason, or something went wrong.
  DidntFinish,
}

impl RunOutcome {
  /// Whether the run did what it was asked. Everything else is a dead end the
  /// user needs a next step from.
  pub fn is_success(self) -> bool {
    matches!(self, RunOutcome::Finished)
  }
}

/// Builds the sentence for a file edit row.
pub fn edit_summary(path: &str, added: u32, removed: u32) -> String {
  match (added, removed) {
    (0, 0) => format!("Left {path} unchanged"),
    (a, 0) => format!("Added {a} {} to {path}", lines(a)),
    (0, r) => format!("Removed {r} {} from {path}", lines(r)),
    (a, r) => format!("Changed {path}: {a} {} added, {r} removed", lines(a)),
  }
}

fn lines(n: u32) -> &'static str {
  if n == 1 {
    "line"
  } else {
    "lines"
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn edit_summaries_read_as_sentences() {
    assert_eq!(edit_summary("a.rs", 3, 0), "Added 3 lines to a.rs");
    assert_eq!(edit_summary("a.rs", 1, 0), "Added 1 line to a.rs");
    assert_eq!(edit_summary("a.rs", 0, 1), "Removed 1 line from a.rs");
    assert_eq!(edit_summary("a.rs", 2, 5), "Changed a.rs: 2 lines added, 5 removed");
    assert_eq!(edit_summary("a.rs", 1, 2), "Changed a.rs: 1 line added, 2 removed");
    assert_eq!(edit_summary("a.rs", 0, 0), "Left a.rs unchanged");
  }

  #[test]
  fn gate_summaries_are_never_empty() {
    let gates = [
      Gate::Dependency { name: "left-pad".into(), summary: "adds a library".into() },
      Gate::Install { command: "npm i".into(), summary: "downloads code".into() },
      Gate::Network { target: "example.com".into(), summary: "reaches the internet".into() },
      Gate::Delete { paths: vec!["a".into()], summary: "deletes a file".into() },
      Gate::OutsideRepo { path: "/etc".into(), summary: "outside the repo".into() },
      Gate::UnknownTool { name: "bash".into(), summary: "runs a command".into() },
    ];
    for g in &gates {
      assert!(!g.summary().is_empty(), "{g:?}");
    }
  }

  #[test]
  fn only_finished_counts_as_success() {
    assert!(RunOutcome::Finished.is_success());
    for o in [RunOutcome::OutOfSteps, RunOutcome::Stopped, RunOutcome::DidntFinish] {
      assert!(!o.is_success(), "{o:?} must not read as success");
    }
  }

  #[test]
  fn gate_choices_do_not_include_remembering() {
    // Guards the house rule: no gate offers "don't ask again". If a variant is
    // ever added, this fails and the rule gets re-argued rather than eroded.
    let json = serde_json::to_string(&GateChoice::AllowOnce).unwrap();
    assert_eq!(json, "\"allowOnce\"");
    for bad in ["always", "remember", "dontAsk"] {
      assert!(!json.contains(bad));
    }
  }

  #[test]
  fn events_serialize_with_a_kind_the_console_can_switch_on() {
    let e = RunEvent::EditedFile {
      path: "a.rs".into(),
      added: 1,
      removed: 0,
      summary: edit_summary("a.rs", 1, 0),
    };
    let v: serde_json::Value = serde_json::to_value(&e).unwrap();
    assert_eq!(v["kind"], "editedFile");
    assert_eq!(v["summary"], "Added 1 line to a.rs");
  }
}
