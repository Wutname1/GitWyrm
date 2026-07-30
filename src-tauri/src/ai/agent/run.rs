//! The plan / act / observe loop.
//!
//! Ends when the task's checkbox is ticked, when the turn budget is spent, or
//! when the user stops it. There is no fourth way out: a loop that can run on
//! is a loop that will.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::events::{Gate, GateChoice, RunEvent, RunOutcome};
use super::guardrails::{self, Verdict};
use super::tools::{self, PathRefusal};
use super::transport::{
  AgentError, AgentTurn, Message, ProviderAgent, ToolCall, ToolResult, TurnRequest,
};

/// How the loop reaches the outside world.
///
/// A trait so the loop can be tested without a provider, a repository, or a
/// user: the interesting behaviour is budget, gating, and completion, none of
/// which need real I/O to exercise.
pub trait RunHost {
  /// Whether the targeted task's checkbox is now ticked. This is the run's
  /// definition of done.
  fn task_is_done(&self) -> bool;
  /// Runs one tool and returns what the model should be told.
  fn run_tool(&mut self, call: &ToolCall) -> Result<String, String>;
  /// Asks the user about a gate. Blocks the run until answered.
  fn ask(&mut self, gate: &Gate) -> GateChoice;
  /// Reports progress.
  fn emit(&mut self, event: RunEvent);
}

/// Everything a run needs to start.
pub struct RunConfig {
  pub repo_root: PathBuf,
  pub model: String,
  pub system: String,
  /// The task, phrased for the model.
  pub task: String,
  /// Turns before the run ends as "didn't finish".
  pub turn_budget: u32,
}

/// Set by the console's Stop button. Checked between turns and before each
/// tool, so Stop lands within a step rather than at the end of a run.
#[derive(Clone, Default)]
pub struct StopFlag(Arc<AtomicBool>);

impl StopFlag {
  pub fn new() -> Self {
    Self::default()
  }
  pub fn stop(&self) {
    self.0.store(true, Ordering::SeqCst);
  }
  pub fn is_stopped(&self) -> bool {
    self.0.load(Ordering::SeqCst)
  }
}

/// What a finished run reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
  pub outcome: RunOutcome,
  pub turns_used: u32,
  pub summary: String,
}

/// Runs the loop to completion.
pub async fn run<A: ProviderAgent, H: RunHost>(
  agent: &A,
  host: &mut H,
  config: RunConfig,
  stop: StopFlag,
) -> RunReport {
  let schemas = tools::tool_schemas();
  let mut messages: Vec<Message> = vec![Message::User { content: config.task.clone() }];
  let mut turns_used = 0;

  // Checked before the first turn as well as between them: stopping during
  // preflight should not still cost a generation.
  while turns_used < config.turn_budget {
    if stop.is_stopped() {
      return finish(host, RunOutcome::Stopped, turns_used, "You stopped this run.");
    }

    let request = TurnRequest {
      model: &config.model,
      system: &config.system,
      messages: &messages,
      tools: &schemas,
    };

    let turn: AgentTurn = match agent.turn(request).await {
      Ok(t) => t,
      Err(AgentError::Cancelled) => {
        return finish(host, RunOutcome::Stopped, turns_used, "You stopped this run.")
      }
      Err(e) => {
        let summary = super::select::plain_explanation(&e);
        return finish(host, RunOutcome::DidntFinish, turns_used, &summary);
      }
    };
    turns_used += 1;

    if !turn.text.trim().is_empty() {
      host.emit(RunEvent::Said { text: turn.text.clone() });
    }

    // No tool calls means the model has stopped working. If the task is done,
    // that is success; if not, it has given up, and saying so beats letting
    // the budget quietly run out.
    if turn.tool_calls.is_empty() {
      return if host.task_is_done() {
        finish(host, RunOutcome::Finished, turns_used, "Finished.")
      } else {
        finish(
          host,
          RunOutcome::DidntFinish,
          turns_used,
          "The AI stopped before the task was ticked off.",
        )
      };
    }

    let mut results = Vec::new();
    for call in &turn.tool_calls {
      if stop.is_stopped() {
        return finish(host, RunOutcome::Stopped, turns_used, "You stopped this run.");
      }

      match decide(&config.repo_root, call) {
        Verdict::Allow => {
          let outcome = host.run_tool(call);
          emit_for(host, call, &outcome);
          results.push(result_of(call, outcome));
        }
        Verdict::Refuse { reason } => {
          host.emit(RunEvent::Refused { summary: reason.clone() });
          results.push(ToolResult { id: call.id.clone(), content: reason, is_error: true });
        }
        Verdict::Ask(gate) => {
          host.emit(RunEvent::GateOpened { gate: gate.clone() });
          match host.ask(&gate) {
            GateChoice::AllowOnce => {
              let outcome = host.run_tool(call);
              emit_for(host, call, &outcome);
              results.push(result_of(call, outcome));
            }
            GateChoice::FindAnotherWay => {
              // Told, not silently dropped: the model can only adapt to a
              // refusal it can see.
              let msg = "The person you're helping said no to this. \
                         Do it another way, without that step.";
              host.emit(RunEvent::Refused {
                summary: "You said no. The AI will try another way.".into(),
              });
              results.push(ToolResult {
                id: call.id.clone(),
                content: msg.into(),
                is_error: true,
              });
            }
            GateChoice::StopRun => {
              return finish(host, RunOutcome::Stopped, turns_used, "You stopped this run.")
            }
          }
        }
      }
    }

    messages.push(Message::Assistant {
      content: turn.text,
      tool_calls: turn.tool_calls.clone(),
    });
    for r in results {
      messages.push(Message::Tool { result: r });
    }

    // Checked after the tools have run, since ticking the checkbox is itself
    // one of the edits a turn makes.
    if host.task_is_done() {
      return finish(host, RunOutcome::Finished, turns_used, "Finished.");
    }
  }

  finish(
    host,
    RunOutcome::OutOfSteps,
    turns_used,
    "Didn't finish: it used all its steps. You can give it more steps in settings and \
     try again.",
  )
}

/// Applies the boundary and the guardrails to one call.
fn decide(root: &std::path::Path, call: &ToolCall) -> Verdict {
  let path_arg = call
    .arguments
    .get("path")
    .and_then(|v| v.as_str())
    .unwrap_or_default()
    .to_string();

  let refusal: Option<PathRefusal> = if path_arg.is_empty() {
    None
  } else {
    tools::resolve_in_repo(root, &path_arg).err()
  };

  // The argument the guardrails inspect is the path when there is one, and the
  // whole argument blob otherwise -- a push hides in a check's command as
  // readily as in a path.
  let inspected = if path_arg.is_empty() { call.arguments.to_string() } else { path_arg };
  guardrails::judge(&call.name, &inspected, refusal)
}

fn emit_for<H: RunHost>(host: &mut H, call: &ToolCall, outcome: &Result<String, String>) {
  let path = call.arguments.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
  match (&call.name[..], outcome) {
    ("read_file", Ok(_)) => host.emit(RunEvent::ReadFile {
      summary: format!("Read {path}"),
      path,
    }),
    ("list_directory", Ok(_)) => host.emit(RunEvent::ListedDirectory {
      summary: format!("Looked in {path}"),
      path,
    }),
    ("edit_file", Ok(_)) => host.emit(RunEvent::EditedFile {
      summary: format!("Changed {path}"),
      path,
      added: 0,
      removed: 0,
    }),
    ("run_check", res) => {
      let name = call.arguments.get("name").and_then(|v| v.as_str()).unwrap_or("check");
      let passed = res.is_ok();
      host.emit(RunEvent::CheckRan {
        name: name.to_string(),
        passed,
        summary: if passed {
          format!("{name} passed")
        } else {
          format!("{name} failed")
        },
      });
    }
    (_, Err(e)) => host.emit(RunEvent::Refused { summary: e.clone() }),
    _ => {}
  }
}

fn result_of(call: &ToolCall, outcome: Result<String, String>) -> ToolResult {
  match outcome {
    Ok(content) => ToolResult { id: call.id.clone(), content, is_error: false },
    Err(content) => ToolResult { id: call.id.clone(), content, is_error: true },
  }
}

fn finish<H: RunHost>(
  host: &mut H,
  outcome: RunOutcome,
  turns_used: u32,
  summary: &str,
) -> RunReport {
  host.emit(RunEvent::Finished { outcome, summary: summary.to_string() });
  RunReport { outcome, turns_used, summary: summary.to_string() }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ai::agent::transport::Transport;
  use serde_json::json;

  /// A provider that replays a fixed script of turns.
  struct ScriptedAgent {
    turns: std::sync::Mutex<Vec<AgentTurn>>,
  }

  impl ScriptedAgent {
    fn new(turns: Vec<AgentTurn>) -> Self {
      Self { turns: std::sync::Mutex::new(turns) }
    }
  }

  impl ProviderAgent for ScriptedAgent {
    fn transport(&self) -> Transport {
      Transport::ApiKey
    }
    async fn check(&self) -> Result<(), AgentError> {
      Ok(())
    }
    async fn turn(&self, _req: TurnRequest<'_>) -> Result<AgentTurn, AgentError> {
      let mut t = self.turns.lock().unwrap();
      if t.is_empty() {
        // A script that runs dry keeps asking for a tool, so the budget is
        // what ends the run rather than the script.
        return Ok(AgentTurn {
          text: String::new(),
          tool_calls: vec![call("read_file", json!({ "path": "a.txt" }))],
        });
      }
      Ok(t.remove(0))
    }
  }

  fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall { id: "c1".into(), name: name.into(), arguments }
  }

  #[derive(Default)]
  struct TestHost {
    events: Vec<RunEvent>,
    tools_run: Vec<String>,
    done_after: Option<usize>,
    answer: Option<GateChoice>,
    asked: Vec<Gate>,
  }

  impl RunHost for TestHost {
    fn task_is_done(&self) -> bool {
      self.done_after.is_some_and(|n| self.tools_run.len() >= n)
    }
    fn run_tool(&mut self, call: &ToolCall) -> Result<String, String> {
      self.tools_run.push(call.name.clone());
      Ok("ok".into())
    }
    fn ask(&mut self, gate: &Gate) -> GateChoice {
      self.asked.push(gate.clone());
      self.answer.unwrap_or(GateChoice::FindAnotherWay)
    }
    fn emit(&mut self, event: RunEvent) {
      self.events.push(event);
    }
  }

  fn cfg(budget: u32) -> RunConfig {
    RunConfig {
      repo_root: std::env::temp_dir().join("gitwyrm-run-tests"),
      model: "m".into(),
      system: "s".into(),
      task: "do the thing".into(),
      turn_budget: budget,
    }
  }

  #[tokio::test]
  async fn a_run_ends_when_the_task_is_ticked() {
    let agent =
      ScriptedAgent::new(vec![AgentTurn {
        text: "working".into(),
        tool_calls: vec![call("edit_file", json!({ "path": "a.txt" }))],
      }]);
    let mut host = TestHost { done_after: Some(1), ..Default::default() };
    let report = run(&agent, &mut host, cfg(12), StopFlag::new()).await;
    assert_eq!(report.outcome, RunOutcome::Finished);
    assert_eq!(report.turns_used, 1);
  }

  #[tokio::test]
  async fn a_run_that_never_finishes_ends_on_the_budget() {
    let agent = ScriptedAgent::new(vec![]);
    let mut host = TestHost::default();
    let report = run(&agent, &mut host, cfg(3), StopFlag::new()).await;
    assert_eq!(report.outcome, RunOutcome::OutOfSteps);
    assert_eq!(report.turns_used, 3, "the budget is a count of turns, not tools");
    assert!(report.summary.contains("settings"), "{}", report.summary);
  }

  #[tokio::test]
  async fn stopping_ends_the_run_without_another_turn() {
    let agent = ScriptedAgent::new(vec![]);
    let mut host = TestHost::default();
    let stop = StopFlag::new();
    stop.stop();
    let report = run(&agent, &mut host, cfg(12), stop).await;
    assert_eq!(report.outcome, RunOutcome::Stopped);
    assert_eq!(report.turns_used, 0, "stopping before the first turn should cost nothing");
  }

  #[tokio::test]
  async fn a_push_is_refused_and_the_run_continues() {
    // Refused, not fatal: the AI is told and can do something else.
    let agent = ScriptedAgent::new(vec![
      AgentTurn { text: String::new(), tool_calls: vec![call("run_check", json!({ "name": "git push origin" }))] },
      AgentTurn { text: "ok, not pushing".into(), tool_calls: vec![] },
    ]);
    let mut host = TestHost { done_after: Some(0), ..Default::default() };
    let report = run(&agent, &mut host, cfg(12), StopFlag::new()).await;
    assert!(host.tools_run.is_empty(), "a push must never reach the tool");
    assert!(host
      .events
      .iter()
      .any(|e| matches!(e, RunEvent::Refused { summary } if summary.contains("never"))));
    assert_eq!(report.outcome, RunOutcome::Finished);
  }

  #[tokio::test]
  async fn an_unknown_tool_opens_a_gate_rather_than_running() {
    let agent = ScriptedAgent::new(vec![
      AgentTurn { text: String::new(), tool_calls: vec![call("bash", json!({ "cmd": "ls" }))] },
      AgentTurn { text: "done".into(), tool_calls: vec![] },
    ]);
    let mut host = TestHost {
      done_after: Some(0),
      answer: Some(GateChoice::FindAnotherWay),
      ..Default::default()
    };
    run(&agent, &mut host, cfg(12), StopFlag::new()).await;
    assert!(matches!(host.asked.first(), Some(Gate::UnknownTool { .. })));
    assert!(host.tools_run.is_empty(), "a gated tool must not run before the answer");
  }

  #[tokio::test]
  async fn allowing_a_gate_once_runs_the_tool() {
    let agent = ScriptedAgent::new(vec![
      AgentTurn { text: String::new(), tool_calls: vec![call("bash", json!({ "cmd": "ls" }))] },
      AgentTurn { text: "done".into(), tool_calls: vec![] },
    ]);
    let mut host = TestHost {
      done_after: Some(1),
      answer: Some(GateChoice::AllowOnce),
      ..Default::default()
    };
    run(&agent, &mut host, cfg(12), StopFlag::new()).await;
    assert_eq!(host.tools_run, vec!["bash"]);
  }

  #[tokio::test]
  async fn choosing_stop_at_a_gate_ends_the_run() {
    let agent = ScriptedAgent::new(vec![AgentTurn {
      text: String::new(),
      tool_calls: vec![call("bash", json!({ "cmd": "ls" }))],
    }]);
    let mut host =
      TestHost { answer: Some(GateChoice::StopRun), ..Default::default() };
    let report = run(&agent, &mut host, cfg(12), StopFlag::new()).await;
    assert_eq!(report.outcome, RunOutcome::Stopped);
    assert!(host.tools_run.is_empty());
  }

  #[tokio::test]
  async fn giving_up_without_finishing_says_so() {
    // No tool calls and the task not ticked: the model has stopped working.
    // Saying so beats letting the budget quietly run down.
    let agent = ScriptedAgent::new(vec![AgentTurn {
      text: "I can't do this".into(),
      tool_calls: vec![],
    }]);
    let mut host = TestHost::default();
    let report = run(&agent, &mut host, cfg(12), StopFlag::new()).await;
    assert_eq!(report.outcome, RunOutcome::DidntFinish);
    assert!(report.summary.contains("stopped before"), "{}", report.summary);
  }

  #[tokio::test]
  async fn a_path_outside_the_repo_is_gated() {
    let agent = ScriptedAgent::new(vec![
      AgentTurn {
        text: String::new(),
        tool_calls: vec![call("read_file", json!({ "path": "../../secrets" }))],
      },
      AgentTurn { text: "done".into(), tool_calls: vec![] },
    ]);
    let mut host = TestHost {
      done_after: Some(0),
      answer: Some(GateChoice::FindAnotherWay),
      ..Default::default()
    };
    run(&agent, &mut host, cfg(12), StopFlag::new()).await;
    assert!(matches!(host.asked.first(), Some(Gate::OutsideRepo { .. })));
    assert!(host.tools_run.is_empty());
  }

  #[tokio::test]
  async fn every_run_emits_exactly_one_finished_event() {
    let agent = ScriptedAgent::new(vec![]);
    let mut host = TestHost::default();
    run(&agent, &mut host, cfg(2), StopFlag::new()).await;
    let finished =
      host.events.iter().filter(|e| matches!(e, RunEvent::Finished { .. })).count();
    assert_eq!(finished, 1);
  }
}
