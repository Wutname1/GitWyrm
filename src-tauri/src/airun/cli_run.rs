//! Driving a whole task through the Copilot CLI's own agent loop.
//!
//! Separate from [`crate::ai::agent::run`], which is our loop for transports
//! that answer one turn at a time. The CLI is not one of those: it plans, calls
//! its own tools, and reports as it goes. Pretending otherwise -- feeding it
//! single turns -- would silently do half a run, which is why `CliAgent::turn`
//! refuses rather than faking it.
//!
//! What we keep is the part that matters: the tool set is fixed when the server
//! starts, permission requests come back to us to answer, and the run is
//! cancellable.

use std::sync::mpsc;

use crate::ai::agent::acp::{Incoming, PermissionDecision, StopReason};
use crate::ai::agent::cli_agent::CliAgent;
use crate::ai::agent::transport::AgentError;

use super::driver::{GateAnswer, GateRequest, RunState, RunStep};
use super::engine::Sink;

/// Runs one task to completion, reporting as it goes.
///
/// Blocking: the caller owns the thread. Gate answers arrive on `answers`, and
/// a closed channel is treated as a stop rather than an approval.
pub async fn run_task(
    agent: &CliAgent,
    task: &str,
    sink: Sink,
    answers: mpsc::Receiver<GateAnswer>,
) {
    let mut conn = match agent.connect().await {
        Ok(c) => c,
        Err(e) => {
            let detail = crate::ai::agent::select::plain_explanation(&e);
            sink(
                RunState::Failed,
                RunStep::Ended {
                    state: RunState::Failed,
                    detail,
                },
            );
            return;
        }
    };

    // Draining the stream and waiting for the turn have to happen together: a
    // permission request arrives *during* the prompt, and the prompt does not
    // return until it is answered. Waiting for one before reading the other
    // would deadlock, so the receiver is taken out rather than borrowed.
    let mut incoming = match conn.take_incoming() {
        Some(rx) => rx,
        None => {
            sink(
                RunState::Failed,
                RunStep::Ended {
                    state: RunState::Failed,
                    detail: "This run could not start listening to the AI.".into(),
                },
            );
            return;
        }
    };

    let outcome: Result<StopReason, AgentError> = {
        let prompt = conn.prompt(task);
        tokio::pin!(prompt);
        loop {
            tokio::select! {
              result = &mut prompt => break result,
              Some(item) = incoming.recv() => handle(item, &sink, &answers),
            }
        }
    };

    let (state, detail) = match outcome {
        Ok(stop) => match stop {
            StopReason::EndTurn => (
                RunState::Finished,
                "Finished. Your changes are ready to look over.".to_string(),
            ),
            StopReason::Cancelled => (
                RunState::Stopped,
                "You stopped this run. Nothing was committed.".to_string(),
            ),
            other => (
                RunState::Failed,
                format!(
                    "Didn't finish: {}. Nothing was committed and your own work is untouched.",
                    other.plain_reason()
                ),
            ),
        },
        Err(e) => (
            RunState::Failed,
            format!(
                "{} Nothing was committed and your own work is untouched.",
                crate::ai::agent::select::plain_explanation(&e)
            ),
        ),
    };

    sink(state, RunStep::Ended { state, detail });
    conn.shutdown().await;
}

/// Turns one message from the agent into a console row, answering permission
/// requests along the way.
fn handle(item: Incoming, sink: &Sink, answers: &mpsc::Receiver<GateAnswer>) {
    match item {
        Incoming::TextChunk(text) => {
            if !text.trim().is_empty() {
                sink(RunState::Working, RunStep::Note { text });
            }
        }
        Incoming::ToolCall { title, .. } => {
            sink(RunState::Working, RunStep::Note { text: title });
        }
        Incoming::PermissionRequest {
            tool_call,
            options,
            respond,
        } => {
            let request = describe(&tool_call);
            sink(RunState::NeedsYou, RunStep::Gate { request });

            // Blocking here is what "the run fully pauses" means: the agent's turn
            // does not continue until this is answered.
            let decision = match answers.recv() {
                Ok(GateAnswer::AllowOnce) => pick(&options, true),
                Ok(GateAnswer::FindAnotherWay) => pick(&options, false),
                // A closed channel means the console went away. Cancelling is the safe
                // reading; treating it as approval would let a run continue with
                // nobody watching.
                Ok(GateAnswer::StopRun) | Err(_) => PermissionDecision::Cancelled,
            };
            let _ = respond.send(decision);
        }
    }
}

/// Chooses an allow-once or reject-once option from what the agent offered.
///
/// Never picks an `allow_always` variant even when one is offered: a remembered
/// approval is a decision made once and then applied to situations the user
/// never saw.
fn pick(options: &[crate::ai::agent::acp::PermissionOption], allow: bool) -> PermissionDecision {
    let wanted = if allow { "allow_once" } else { "reject_once" };
    let chosen = options
        .iter()
        .find(|o| o.kind == wanted)
        // Fall back to any option of the right sense, but never a remembered one.
        .or_else(|| {
            options.iter().find(|o| {
                if allow {
                    o.kind == "allow_once"
                } else {
                    o.kind.starts_with("reject")
                }
            })
        });

    match chosen {
        Some(o) if allow => PermissionDecision::AllowOnce {
            option_id: o.option_id.clone(),
        },
        Some(o) => PermissionDecision::RejectOnce {
            option_id: o.option_id.clone(),
        },
        // Nothing usable on offer: cancelling is safer than guessing.
        None => PermissionDecision::Cancelled,
    }
}

/// Best-effort reading of what the agent is asking permission for.
///
/// The tool call's shape is the agent's to define, so this falls back to a
/// generic ask rather than guessing wrongly and mislabelling the consequence.
fn describe(tool_call: &serde_json::Value) -> GateRequest {
    let title = tool_call
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("something outside its normal tools");
    GateRequest::Unclassified {
        summary: title.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::agent::acp::PermissionOption;

    fn opt(kind: &str, id: &str) -> PermissionOption {
        PermissionOption {
            option_id: id.into(),
            name: kind.into(),
            kind: kind.into(),
        }
    }

    #[test]
    fn allowing_picks_the_once_option_not_the_remembered_one() {
        let options = vec![opt("allow_always", "a"), opt("allow_once", "b")];
        match pick(&options, true) {
            PermissionDecision::AllowOnce { option_id } => assert_eq!(option_id, "b"),
            other => panic!("expected allow-once, got {other:?}"),
        }
    }

    #[test]
    fn denying_picks_a_reject_option() {
        let options = vec![opt("allow_once", "a"), opt("reject_once", "b")];
        match pick(&options, false) {
            PermissionDecision::RejectOnce { option_id } => assert_eq!(option_id, "b"),
            other => panic!("expected reject-once, got {other:?}"),
        }
    }

    #[test]
    fn nothing_usable_cancels_rather_than_guessing() {
        let options = vec![opt("allow_always", "a")];
        assert!(matches!(
            pick(&options, true),
            PermissionDecision::Cancelled
        ));
    }

    #[test]
    fn a_gate_always_has_something_to_show() {
        let empty = serde_json::json!({});
        assert!(!describe(&empty).title().is_empty());
        let named = serde_json::json!({ "title": "Run npm install" });
        assert!(describe(&named).title().contains("npm install"));
    }
}
