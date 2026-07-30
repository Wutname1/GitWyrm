//! One run per repository, and the rule that keeps them apart.
//!
//! Sessions are identified rather than merely replaced. A driver that is still
//! finishing when its session is superseded must not write into the newer
//! one's console -- so every event carries the session it belongs to, and the
//! registry drops anything from a session that is no longer current.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::driver::{RunEventKind, RunState, RunStep};

/// A run the console is showing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunSession {
  pub session_id: String,
  pub repo_id: String,
  /// The change this run belongs to, so the header always names its own.
  pub change_id: String,
  /// Task number within that change, as the header shows it.
  pub task_number: u32,
  pub task_text: String,
  /// Branch the run is pinned to. A run must not continue on another.
  pub branch: String,
  pub state: RunState,
  pub steps: Vec<RunStep>,
}

impl RunSession {
  /// "Task 3 · Add a settings toggle"
  pub fn header(&self) -> String {
    format!("Task {} · {}", self.task_number, self.task_text)
  }
}

/// Why a run could not be started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartRefusal {
  /// One is already going in this repository.
  AlreadyRunning { session_id: String, summary: String },
}

/// Holds the current session per repository.
#[derive(Default)]
pub struct SessionRegistry {
  inner: Mutex<HashMap<String, RunSession>>,
  counter: AtomicU64,
}

impl SessionRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  /// Session ids are sequential rather than random so an out-of-order event is
  /// obvious in a log, and so tests are deterministic.
  fn next_id(&self) -> String {
    format!("run-{}", self.counter.fetch_add(1, Ordering::SeqCst) + 1)
  }

  /// Starts a session, or refuses because one is already active.
  ///
  /// Refusing rather than replacing is deliberate: a second start is usually a
  /// double-click or a forgotten run in another window, and silently discarding
  /// the first would throw away work in progress.
  pub fn start(
    &self,
    repo_id: &str,
    change_id: &str,
    task_number: u32,
    task_text: &str,
    branch: &str,
  ) -> Result<RunSession, StartRefusal> {
    let mut map = self.inner.lock().unwrap();
    if let Some(existing) = map.get(repo_id) {
      if existing.state.is_active() {
        return Err(StartRefusal::AlreadyRunning {
          session_id: existing.session_id.clone(),
          summary: format!(
            "A task is already running in this repository: {}. Open the AI tab to see it.",
            existing.header()
          ),
        });
      }
    }
    let session = RunSession {
      session_id: self.next_id(),
      repo_id: repo_id.to_string(),
      change_id: change_id.to_string(),
      task_number,
      task_text: task_text.to_string(),
      branch: branch.to_string(),
      state: RunState::Preparing,
      steps: Vec::new(),
    };
    map.insert(repo_id.to_string(), session.clone());
    Ok(session)
  }

  pub fn get(&self, repo_id: &str) -> Option<RunSession> {
    self.inner.lock().unwrap().get(repo_id).cloned()
  }

  /// Records an event, unless it belongs to a session that is no longer
  /// current. Returns false when it was dropped.
  pub fn record(&self, event: &RunEventKind) -> bool {
    let mut map = self.inner.lock().unwrap();
    let Some(session) = map.get_mut(&event.repo_id) else {
      return false;
    };
    if session.session_id != event.session_id {
      log::info!(
        "dropping an event from session {} -- {} is current",
        event.session_id,
        session.session_id
      );
      return false;
    }
    session.state = event.state;
    session.steps.push(event.step.clone());
    true
  }

  /// Clears a finished session so the repository can start another.
  pub fn clear(&self, repo_id: &str) {
    self.inner.lock().unwrap().remove(repo_id);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn reg() -> SessionRegistry {
    SessionRegistry::new()
  }

  fn event(repo: &str, session: &str, state: RunState) -> RunEventKind {
    RunEventKind {
      repo_id: repo.into(),
      session_id: session.into(),
      state,
      summary: "x".into(),
      step: RunStep::Note { text: "x".into() },
    }
  }

  #[test]
  fn a_second_run_is_refused_with_a_route_to_the_first() {
    let r = reg();
    r.start("repo", "chg", 1, "do a thing", "main").unwrap();
    let err = r.start("repo", "chg", 2, "another", "main").unwrap_err();
    let StartRefusal::AlreadyRunning { summary, .. } = err;
    assert!(summary.contains("already running"), "{summary}");
    assert!(summary.contains("AI tab"), "should say where to look: {summary}");
  }

  #[test]
  fn a_finished_run_does_not_block_the_next() {
    let r = reg();
    let s = r.start("repo", "chg", 1, "first", "main").unwrap();
    r.record(&event("repo", &s.session_id, RunState::Finished));
    assert!(r.start("repo", "chg", 2, "second", "main").is_ok());
  }

  #[test]
  fn two_repositories_run_independently() {
    let r = reg();
    r.start("a", "chg", 1, "t", "main").unwrap();
    assert!(r.start("b", "chg", 1, "t", "main").is_ok());
  }

  #[test]
  fn an_old_session_cannot_write_into_a_newer_console() {
    // The integrity rule: a driver still finishing when its session was
    // replaced must not appear in the new run's stream.
    let r = reg();
    let first = r.start("repo", "chg", 1, "first", "main").unwrap();
    r.record(&event("repo", &first.session_id, RunState::Finished));
    let second = r.start("repo", "chg", 2, "second", "main").unwrap();

    assert!(!r.record(&event("repo", &first.session_id, RunState::Working)));
    let now = r.get("repo").unwrap();
    assert_eq!(now.session_id, second.session_id);
    assert!(now.steps.is_empty(), "the stale event must not have been recorded");

    assert!(r.record(&event("repo", &second.session_id, RunState::Working)));
    assert_eq!(r.get("repo").unwrap().steps.len(), 1);
  }

  #[test]
  fn an_event_for_an_unknown_repository_is_dropped() {
    let r = reg();
    assert!(!r.record(&event("nobody", "run-1", RunState::Working)));
  }

  #[test]
  fn the_header_names_the_runs_own_task() {
    let r = reg();
    let s = r.start("repo", "chg", 3, "Add a settings toggle", "main").unwrap();
    assert_eq!(s.header(), "Task 3 · Add a settings toggle");
    assert_eq!(s.change_id, "chg", "the header must show the run's own change");
  }

  #[test]
  fn recording_moves_the_session_state() {
    let r = reg();
    let s = r.start("repo", "chg", 1, "t", "main").unwrap();
    assert_eq!(s.state, RunState::Preparing);
    r.record(&event("repo", &s.session_id, RunState::NeedsYou));
    assert_eq!(r.get("repo").unwrap().state, RunState::NeedsYou);
  }
}
