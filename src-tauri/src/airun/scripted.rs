//! A driver that replays a fixed sequence, for building and checking the
//! console before a provider is wired.
//!
//! **This is never presented to the user as a real run.** Every session it
//! produces is flagged, and the command that starts one is only reachable in a
//! debug build -- a scripted run that looked real would be a lie told by the
//! product, not a test fixture.

use std::collections::VecDeque;

use super::driver::{
  GateAnswer, GateRequest, PreflightItem, RunDriver, RunState, RunStep,
};

/// One scripted beat.
#[derive(Debug, Clone)]
pub struct Beat {
  pub step: RunStep,
  pub state: RunState,
  /// Milliseconds to wait before emitting, so timing looks like real work
  /// rather than everything arriving at once.
  pub delay_ms: u64,
}

impl Beat {
  pub fn new(step: RunStep, state: RunState) -> Self {
    Self { step, state, delay_ms: 400 }
  }
  pub fn after(mut self, ms: u64) -> Self {
    self.delay_ms = ms;
    self
  }
}

/// Which scenario to replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
  /// Plans, edits, checks, finishes.
  Happy,
  /// Pauses at a dependency gate.
  Gate,
  /// Fails partway with a provider problem.
  ProviderExpired,
  /// Fails partway with something else.
  Failure,
}

/// Replays a script, pausing at gates until answered.
pub struct ScriptedDriver {
  beats: VecDeque<Beat>,
  emitted: Vec<(RunStep, RunState)>,
  awaiting_gate: bool,
  stopped: bool,
  notes: Vec<String>,
}

impl ScriptedDriver {
  pub fn new(scenario: Scenario) -> Self {
    Self {
      beats: script_for(scenario).into(),
      emitted: Vec::new(),
      awaiting_gate: false,
      stopped: false,
      notes: Vec::new(),
    }
  }

  /// Pulls the next beat, or nothing when paused, stopped, or finished.
  ///
  /// The caller drives this so timing lives with whoever owns the clock,
  /// keeping the driver itself synchronous and testable.
  pub fn next_beat(&mut self) -> Option<Beat> {
    if self.awaiting_gate || self.stopped {
      return None;
    }
    let beat = self.beats.pop_front()?;
    if matches!(beat.step, RunStep::Gate { .. }) {
      self.awaiting_gate = true;
    }
    self.emitted.push((beat.step.clone(), beat.state));
    Some(beat)
  }

  /// Whether the run is paused waiting on the user.
  pub fn is_paused(&self) -> bool {
    self.awaiting_gate
  }

  /// Notes the user queued. Assertions only: the console reads them back off
  /// the emitted stream, not from here.
  #[cfg(test)]
  pub fn notes(&self) -> &[String] {
    &self.notes
  }

  /// Steps emitted so far, for assertions.
  pub fn emitted(&self) -> &[(RunStep, RunState)] {
    &self.emitted
  }
}

impl RunDriver for ScriptedDriver {
  fn start(&mut self) {}

  fn answer_gate(&mut self, answer: GateAnswer) {
    if !self.awaiting_gate {
      return;
    }
    self.awaiting_gate = false;
    match answer {
      GateAnswer::AllowOnce => {}
      GateAnswer::FindAnotherWay => {
        // A denial has to be visible in the stream, or the user cannot tell
        // their "no" did anything.
        self.beats.push_front(Beat::new(
          RunStep::Adapted {
            text: "Skipped that and wrote it by hand instead, with no new library.".into(),
          },
          RunState::Working,
        ));
      }
      GateAnswer::StopRun => {
        self.beats.clear();
        self.beats.push_back(Beat::new(
          RunStep::Ended {
            state: RunState::Stopped,
            detail: "You stopped this run. Nothing was committed.".into(),
          },
          RunState::Stopped,
        ));
      }
    }
  }

  fn note(&mut self, text: String) {
    // Queued, not interrupting: the current step finishes first. Echoed so the
    // user can see it was received.
    self.notes.push(text.clone());
    self.beats.push_front(Beat::new(RunStep::YouSaid { text }, RunState::Working).after(0));
  }

  fn stop(&mut self) {
    self.stopped = true;
    self.beats.clear();
    self.emitted.push((
      RunStep::Ended {
        state: RunState::Stopped,
        detail: "You stopped this run. Nothing was committed.".into(),
      },
      RunState::Stopped,
    ));
  }
}

fn preflight() -> RunStep {
  RunStep::Preflight {
    items: vec![
      PreflightItem {
        label: "Read the plan".into(),
        done: true,
        detail: "proposal.md".into(),
      },
      PreflightItem {
        label: "Read the spec changes".into(),
        done: false,
        detail: "There are none for this task".into(),
      },
      PreflightItem {
        label: "Read the task and what counts as done".into(),
        done: true,
        detail: "Task 3".into(),
      },
      PreflightItem {
        label: "Set your own edits aside".into(),
        done: true,
        detail: "2 files, in your stash list".into(),
      },
    ],
  }
}

fn script_for(scenario: Scenario) -> Vec<Beat> {
  let mut beats = vec![
    Beat::new(preflight(), RunState::Preparing).after(300),
    Beat::new(
      RunStep::Plan { text: "Add the toggle to the settings screen and wire it up.".into() },
      RunState::Working,
    )
    .after(900),
    Beat::new(
      RunStep::Edit { path: "src/settings.tsx".into(), added: 14, removed: 2 },
      RunState::Working,
    )
    .after(1200),
  ];

  match scenario {
    Scenario::Happy => {
      beats.push(
        Beat::new(
          RunStep::Check {
            name: "typecheck".into(),
            passed: true,
            detail: "no problems".into(),
          },
          RunState::Working,
        )
        .after(1500),
      );
      beats.push(Beat::new(
        RunStep::Ended {
          state: RunState::Finished,
          detail: "Finished. Your changes are ready to look over.".into(),
        },
        RunState::Finished,
      ));
    }
    Scenario::Gate => {
      beats.push(
        Beat::new(
          RunStep::Gate { request: GateRequest::AddDependency { name: "zod".into() } },
          RunState::NeedsYou,
        )
        .after(800),
      );
      beats.push(
        Beat::new(
          RunStep::Check { name: "typecheck".into(), passed: true, detail: String::new() },
          RunState::Working,
        )
        .after(1200),
      );
      beats.push(Beat::new(
        RunStep::Ended {
          state: RunState::Finished,
          detail: "Finished. Your changes are ready to look over.".into(),
        },
        RunState::Finished,
      ));
    }
    Scenario::ProviderExpired => {
      beats.push(Beat::new(
        RunStep::Ended {
          state: RunState::Failed,
          detail: "Your AI sign-in has expired. Nothing was committed and your own work \
                   is untouched."
            .into(),
        },
        RunState::Failed,
      ));
    }
    Scenario::Failure => {
      beats.push(Beat::new(
        RunStep::Ended {
          state: RunState::Failed,
          detail: "The AI stopped before the task was done. Nothing was committed and \
                   your own work is untouched."
            .into(),
        },
        RunState::Failed,
      ));
    }
  }
  beats
}

#[cfg(test)]
mod tests {
  use super::*;

  fn drain(d: &mut ScriptedDriver) -> Vec<RunStep> {
    let mut out = Vec::new();
    while let Some(b) = d.next_beat() {
      out.push(b.step);
    }
    out
  }

  #[test]
  fn a_happy_run_reaches_finished() {
    let mut d = ScriptedDriver::new(Scenario::Happy);
    let steps = drain(&mut d);
    assert!(matches!(steps.first(), Some(RunStep::Preflight { .. })));
    assert!(matches!(
      steps.last(),
      Some(RunStep::Ended { state: RunState::Finished, .. })
    ));
  }

  #[test]
  fn a_gate_pauses_everything_until_answered() {
    // The requirement: while a gate is open, no further steps execute.
    let mut d = ScriptedDriver::new(Scenario::Gate);
    let before = drain(&mut d);
    assert!(d.is_paused());
    assert!(matches!(before.last(), Some(RunStep::Gate { .. })));

    let after_none = drain(&mut d);
    assert!(after_none.is_empty(), "a paused run must emit nothing");

    d.answer_gate(GateAnswer::AllowOnce);
    assert!(!d.is_paused());
    assert!(!drain(&mut d).is_empty(), "answering resumes the run");
  }

  #[test]
  fn denying_a_gate_visibly_adapts() {
    let mut d = ScriptedDriver::new(Scenario::Gate);
    drain(&mut d);
    d.answer_gate(GateAnswer::FindAnotherWay);
    let after = drain(&mut d);
    assert!(
      matches!(after.first(), Some(RunStep::Adapted { .. })),
      "the next row must state the alternative taken"
    );
  }

  #[test]
  fn stopping_at_a_gate_ends_the_run() {
    let mut d = ScriptedDriver::new(Scenario::Gate);
    drain(&mut d);
    d.answer_gate(GateAnswer::StopRun);
    let after = drain(&mut d);
    assert!(matches!(
      after.last(),
      Some(RunStep::Ended { state: RunState::Stopped, .. })
    ));
  }

  #[test]
  fn stopping_says_nothing_was_committed() {
    let mut d = ScriptedDriver::new(Scenario::Happy);
    d.next_beat();
    d.stop();
    let (step, state) = d.emitted().last().unwrap();
    assert_eq!(*state, RunState::Stopped);
    match step {
      RunStep::Ended { detail, .. } => assert!(detail.contains("Nothing was committed")),
      other => panic!("expected an ending, got {other:?}"),
    }
  }

  #[test]
  fn a_note_is_queued_and_echoed_without_interrupting() {
    let mut d = ScriptedDriver::new(Scenario::Happy);
    d.next_beat();
    d.note("use tabs".into());
    assert_eq!(d.notes(), ["use tabs"]);
    let next = d.next_beat().unwrap();
    assert!(matches!(next.step, RunStep::YouSaid { .. }), "the note is echoed back");
  }

  #[test]
  fn failure_scenarios_promise_the_users_work_is_untouched() {
    for s in [Scenario::Failure, Scenario::ProviderExpired] {
      let mut d = ScriptedDriver::new(s);
      let steps = drain(&mut d);
      match steps.last() {
        Some(RunStep::Ended { state, detail }) => {
          assert_eq!(*state, RunState::Failed);
          assert!(detail.contains("untouched"), "{s:?}: {detail}");
          assert!(detail.contains("Nothing was committed"), "{s:?}: {detail}");
        }
        other => panic!("{s:?} did not end: {other:?}"),
      }
    }
  }

  #[test]
  fn preflight_says_when_there_is_nothing_to_read() {
    let RunStep::Preflight { items } = preflight() else { panic!() };
    let none = items.iter().find(|i| !i.done).expect("one item honestly has nothing to do");
    assert!(!none.detail.is_empty(), "an unticked item must say why");
  }

  #[test]
  fn a_stopped_driver_emits_nothing_further() {
    let mut d = ScriptedDriver::new(Scenario::Happy);
    d.stop();
    assert!(drain(&mut d).is_empty());
  }
}
