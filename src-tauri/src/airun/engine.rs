//! Connects the agent engine to the run console.
//!
//! The engine speaks its own vocabulary -- tool calls, gates, turn budgets --
//! and the console speaks the user's. This is the one place the two are mapped
//! onto each other, so neither has to know the other's shape.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::ai::agent::events::{Gate as EngineGate, GateChoice, RunEvent, RunOutcome};
use crate::ai::agent::exec;
use crate::ai::agent::run::RunHost;
use crate::ai::agent::transport::ToolCall;

use super::driver::{GateAnswer, GateRequest, RunState, RunStep};

/// What the host does with each event it produces.
///
/// A callback rather than a channel so the caller decides whether to emit,
/// record, or both -- the Tauri layer needs all three and the tests need none.
pub type Sink = Arc<dyn Fn(RunState, RunStep) + Send + Sync>;

/// Bridges one run.
pub struct EngineHost {
  root: std::path::PathBuf,
  sink: Sink,
  /// Set when the targeted task's checkbox is ticked. Checked by the loop as
  /// its definition of done.
  done: Arc<Mutex<bool>>,
  /// Answers arriving from the console. The loop blocks on this while a gate
  /// is open, which is what "the run fully pauses" means in practice.
  answers: mpsc::Receiver<GateAnswer>,
  /// Path of the tasks file, so a tick can be noticed without re-parsing the
  /// whole change on every tool call.
  tasks_file: std::path::PathBuf,
  /// Zero-based position of the target task in that file -- `SpecTask::index`,
  /// not the author's own `1.2` numbering. Those disagree whenever a plan skips
  /// or repeats a number, which real plans do.
  task_index: u32,
}

impl EngineHost {
  pub fn new(
    root: std::path::PathBuf,
    tasks_file: std::path::PathBuf,
    task_index: u32,
    sink: Sink,
    answers: mpsc::Receiver<GateAnswer>,
  ) -> Self {
    Self {
      root,
      sink,
      done: Arc::new(Mutex::new(false)),
      answers,
      tasks_file,
      task_index,
    }
  }

  /// Named `send` rather than `emit` so it does not collide with the
  /// `RunHost::emit` this type also implements.
  fn send(&self, state: RunState, step: RunStep) {
    (self.sink)(state, step);
  }
}

impl RunHost for EngineHost {
  fn task_is_done(&self) -> bool {
    if *self.done.lock().unwrap() {
      return true;
    }
    // Read the file rather than trust the model's word. The checkbox is the
    // agreed signal precisely because the user can see it too, and a model
    // claiming completion is not the same as the file saying so.
    let Ok(text) = std::fs::read_to_string(&self.tasks_file) else {
      return false;
    };
    let ticked = crate::openspec::parse::task_is_ticked(&text, self.task_index);
    if ticked {
      *self.done.lock().unwrap() = true;
    }
    ticked
  }

  fn run_tool(&mut self, call: &ToolCall) -> Result<String, String> {
    exec::run_tool(&self.root, &call.name, &call.arguments)
  }

  fn ask(&mut self, gate: &EngineGate) -> GateChoice {
    self.send(RunState::NeedsYou, RunStep::Gate { request: to_request(gate) });
    // Blocking is the point: nothing else runs until this returns. A dropped
    // channel means the console went away, which is treated as a stop rather
    // than an approval.
    match self.answers.recv() {
      Ok(GateAnswer::AllowOnce) => GateChoice::AllowOnce,
      Ok(GateAnswer::FindAnotherWay) => GateChoice::FindAnotherWay,
      Ok(GateAnswer::StopRun) | Err(_) => GateChoice::StopRun,
    }
  }

  fn emit(&mut self, event: RunEvent) {
    let (state, step) = to_step(event);
    self.send(state, step);
  }
}

/// Engine gate -> console gate.
fn to_request(gate: &EngineGate) -> GateRequest {
  match gate {
    EngineGate::Dependency { name, .. } => GateRequest::AddDependency { name: name.clone() },
    EngineGate::Install { command, .. } => GateRequest::RunInstall { command: command.clone() },
    EngineGate::Network { target, .. } => GateRequest::NetworkAccess { target: target.clone() },
    EngineGate::Delete { paths, .. } => GateRequest::DeleteFiles { paths: paths.clone() },
    EngineGate::OutsideRepo { path, .. } => GateRequest::OutsideRepo { path: path.clone() },
    // An unknown tool has no console gate of its own; it is the same question
    // as running something arbitrary, so it is shown as an install-style ask.
    EngineGate::UnknownTool { name, .. } => GateRequest::RunInstall { command: name.clone() },
  }
}

/// Engine event -> console step.
fn to_step(event: RunEvent) -> (RunState, RunStep) {
  match event {
    RunEvent::Planned { summary } => (RunState::Working, RunStep::Plan { text: summary }),
    RunEvent::ReadFile { summary, .. } => (RunState::Working, RunStep::Note { text: summary }),
    RunEvent::EditedFile { path, added, removed, .. } => {
      (RunState::Working, RunStep::Edit { path, added, removed })
    }
    RunEvent::ListedDirectory { summary, .. } => {
      (RunState::Working, RunStep::Note { text: summary })
    }
    RunEvent::CheckRan { name, passed, summary } => {
      (RunState::Working, RunStep::Check { name, passed, detail: summary })
    }
    RunEvent::Said { text } => (RunState::Working, RunStep::Note { text }),
    RunEvent::Refused { summary } => (RunState::Working, RunStep::Adapted { text: summary }),
    RunEvent::GateOpened { gate } => {
      (RunState::NeedsYou, RunStep::Gate { request: to_request(&gate) })
    }
    RunEvent::Finished { outcome, summary } => {
      let state = match outcome {
        RunOutcome::Finished => RunState::Finished,
        RunOutcome::Stopped => RunState::Stopped,
        RunOutcome::OutOfSteps | RunOutcome::DidntFinish => RunState::Failed,
      };
      (state, RunStep::Ended { state, detail: summary })
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_finished_run_is_not_reported_as_a_failure() {
    let (state, _) = to_step(RunEvent::Finished {
      outcome: RunOutcome::Finished,
      summary: "Finished.".into(),
    });
    assert_eq!(state, RunState::Finished);
  }

  #[test]
  fn running_out_of_steps_reads_as_didnt_finish_not_stopped() {
    // "Stopped" means the user did it. Conflating the two would blame them for
    // something the budget did.
    let (state, _) = to_step(RunEvent::Finished {
      outcome: RunOutcome::OutOfSteps,
      summary: "used all its steps".into(),
    });
    assert_eq!(state, RunState::Failed);
    let (stopped, _) = to_step(RunEvent::Finished {
      outcome: RunOutcome::Stopped,
      summary: "you stopped it".into(),
    });
    assert_eq!(stopped, RunState::Stopped);
  }

  #[test]
  fn a_refusal_shows_as_the_run_adapting() {
    let (_, step) = to_step(RunEvent::Refused { summary: "no push".into() });
    assert!(matches!(step, RunStep::Adapted { .. }));
  }

  #[test]
  fn every_engine_gate_maps_to_a_console_gate() {
    let gates = [
      EngineGate::Dependency { name: "zod".into(), summary: String::new() },
      EngineGate::Install { command: "npm i".into(), summary: String::new() },
      EngineGate::Network { target: "x.com".into(), summary: String::new() },
      EngineGate::Delete { paths: vec!["a".into()], summary: String::new() },
      EngineGate::OutsideRepo { path: "/etc".into(), summary: String::new() },
      EngineGate::UnknownTool { name: "bash".into(), summary: String::new() },
    ];
    for g in &gates {
      // Every one produces a card with a non-empty title, so no gate can reach
      // the user as a blank ask.
      assert!(!to_request(g).title().is_empty(), "{g:?}");
    }
  }

  #[test]
  fn an_edit_keeps_its_line_counts_for_the_row() {
    let (_, step) = to_step(RunEvent::EditedFile {
      path: "a.rs".into(),
      added: 3,
      removed: 1,
      summary: String::new(),
    });
    match step {
      RunStep::Edit { path, added, removed } => {
        assert_eq!((path.as_str(), added, removed), ("a.rs", 3, 1));
      }
      other => panic!("expected an edit row, got {other:?}"),
    }
  }
}
