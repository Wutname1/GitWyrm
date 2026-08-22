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
use std::sync::Arc;
use std::time::Duration;

use crate::ai::agent::acp::{Incoming, PermissionDecision, StopReason};
use crate::ai::agent::cli_agent::CliAgent;
use crate::ai::agent::transport::AgentError;
use crate::agentdesk::policy::ExecutionPolicy;

use super::driver::{GateAnswer, GateRequest, RunState, RunStep};
use super::engine::Sink;

/// How long a cooperative `session/cancel` gets to produce the CLI's own
/// `StopReason::Cancelled` before this run gives up waiting and ends the
/// turn itself. Task 2.5: "Do not persist Stopped until cancellation is
/// acknowledged, or a typed timeout is shown" -- this is that timeout.
/// [`AcpConnection::shutdown`] already uses a 3s grace period for the
/// process-level kill; this is longer because a cooperative cancel is worth
/// waiting a little more for than a hard kill is.
const CANCEL_ACK_TIMEOUT: Duration = Duration::from_secs(8);

/// Signals a running [`run_task`] to stop, and reports whether the CLI
/// acknowledged it.
///
/// Exists so the caller that owns *starting* an execution
/// (`commands::agent_desk::start_execution_at`) also owns a way to reach it
/// while it runs, without threading a raw channel through
/// `agentdesk::execution_registry` -- one `Notify` per execution, stored in
/// the registry, is simpler than a channel this consumer-side code would
/// otherwise have to manage the lifetime of.
#[derive(Clone)]
pub struct CancelHandle {
    notify: Arc<tokio::sync::Notify>,
}

impl CancelHandle {
    pub fn new() -> Self {
        Self {
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Asks the running turn to stop. Idempotent: calling this more than
    /// once (e.g. a duplicate Stop click) is harmless -- `Notify::notify_one`
    /// just wakes the waiter again, and `run_task`'s loop only reads it once
    /// per cancel cycle.
    pub fn cancel(&self) {
        self.notify.notify_one();
    }
}

/// Runs one task to completion, reporting as it goes.
///
/// Blocking: the caller owns the thread. Gate answers arrive on `answers`, and
/// a closed channel is treated as a stop rather than an approval.
///
/// `policy` is THE engine-boundary enforcement for task 1.2/1.3: every
/// `Incoming::PermissionRequest` the CLI raises is classified and checked
/// against it in [`handle`] *before* a decision is ever sent back to the
/// CLI -- a malicious or confused provider asking to write under a read-only
/// intent is refused here, never at the prompt layer. `started` mirrors the
/// session's own "has Start been pressed" flag (see
/// `policy::WorktreePolicy::NotUntilStart`); it is fixed for the lifetime of
/// one `run_task` call because starting IS what causes a fresh `run_task`
/// invocation to begin (Plan -> Start re-derives `for_intent(Fix)`-shaped
/// behavior in a brand new execution, it does not flip this flag on an
/// existing one).
///
/// `cancel` is task 2.2/2.3's actual stop mechanism: unlike the pre-existing
/// `answers` channel (which only unblocks a run that is *currently paused at
/// a permission gate* -- see `handle`'s `Ok(GateAnswer::StopRun) | Err(_) =>
/// PermissionDecision::Cancelled` branch, which does nothing for a run that
/// is mid-tool-call or streaming text), `cancel` is watched by the SAME
/// `select!` that drives the whole turn, so a stop reaches the CLI
/// regardless of what the run is doing when it arrives. `session/cancel` is
/// a notification (`AcpConnection::cancel`), not a request -- the protocol
/// requires the agent to answer the in-flight `session/prompt` with
/// `StopReason::Cancelled` afterward, so the same `prompt` future the select
/// loop is already waiting on is what reports the acknowledgement; nothing
/// here waits on a second, separate reply.
pub async fn run_task(
    agent: &CliAgent,
    task: &str,
    sink: Sink,
    answers: mpsc::Receiver<GateAnswer>,
    policy: ExecutionPolicy,
    started: bool,
    cancel: CancelHandle,
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

    // Set once a cancel has been requested, so a second `Notify::notified()`
    // wait is not armed again after the CLI has already been asked to stop
    // (which would otherwise race a second `session/cancel` send against the
    // timeout branch below on every subsequent loop iteration).
    let mut cancel_requested = false;
    // Set once the timeout branch fires, so `outcome`'s own `Err` path below
    // reports a typed "did not acknowledge" failure instead of the generic
    // transport-closed message `prompt`'s own `Err` would otherwise produce
    // once `conn` is dropped out from under it.
    let mut cancel_timed_out = false;

    let outcome: Result<StopReason, AgentError> = {
        let prompt = conn.prompt(task);
        tokio::pin!(prompt);
        // `tokio::time::sleep` needs a fixed deadline to `select!` against
        // repeatedly without being re-created on every loop iteration --
        // `Instant::now() + CANCEL_ACK_TIMEOUT` computed once cancellation
        // is actually requested (never armed at all otherwise, since a
        // never-cancelled run must be able to run indefinitely).
        let mut deadline: Option<tokio::time::Instant> = None;
        loop {
            let timeout = async {
                match deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    // No deadline armed: never resolves, so this branch is
                    // inert until a cancel actually starts the clock.
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! {
              result = &mut prompt => break result,
              Some(item) = incoming.recv() => handle(item, &sink, &answers, &policy, started),
              _ = cancel.notify.notified(), if !cancel_requested => {
                  cancel_requested = true;
                  deadline = Some(tokio::time::Instant::now() + CANCEL_ACK_TIMEOUT);
                  // Best-effort: a transport failure here does not change
                  // what happens next -- either the CLI still answers
                  // `prompt` (client crashed mid-send but the process is
                  // fine), or the timeout branch below ends the turn
                  // anyway. Silently dropping the error is deliberate, not
                  // an oversight: there is no separate error path a
                  // send failure here could usefully feed into that the
                  // timeout does not already cover.
                  let _ = conn.cancel().await;
              }
              _ = timeout, if deadline.is_some() => {
                  cancel_timed_out = true;
                  break Err(AgentError::Failed {
                      detail: "the AI did not confirm it stopped in time".into(),
                  });
              }
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
        Err(e) if cancel_timed_out => (
            // A typed timeout (task 2.5), distinct from an ordinary
            // transport failure: the run is stopped from the app's
            // perspective (the process is about to be killed by
            // `conn.shutdown()` below, same as any other stop), but the CLI
            // itself never confirmed it heard the request. Any edits already
            // on disk or in the worktree are untouched by this -- stopping
            // the CLI process does not revert filesystem changes it already
            // made (task 2.6).
            RunState::Stopped,
            format!(
                "This didn't stop right away, so it was shut down directly. {} Any changes already made are still there.",
                crate::ai::agent::select::plain_explanation(&e)
            ),
        ),
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
///
/// `policy`/`started` are consulted for EVERY `PermissionRequest` before a
/// decision is ever sent back to the CLI -- this is task 1.2/1.3/1.7's actual
/// enforcement point. A read-only intent (Ask/Explain/Summarize/Review), or
/// Plan before Start, gets `PermissionDecision::RejectOnce` (or `Cancelled`
/// if no reject option was offered) without ever consulting `answers` -- the
/// user is never asked to approve something the intent cannot do at all, and
/// the CLI cannot get a write past this by asking nicely.
fn handle(
    item: Incoming,
    sink: &Sink,
    answers: &mpsc::Receiver<GateAnswer>,
    policy: &ExecutionPolicy,
    started: bool,
) {
    match item {
        Incoming::TextChunk(text) => {
            if !text.trim().is_empty() {
                sink(RunState::Working, RunStep::Note { text });
            }
        }
        Incoming::ToolCall { title, .. } => {
            // `Activity`, never `Note`: tool calls are what the agent is
            // *doing*, not what it is *saying*. Keeping them a distinct step
            // kind is what lets `agentdesk::bridge` route them to
            // `MessageKind::Tool` (the compact activity feed) instead of the
            // prose transcript, and stops them from coalescing with -- and
            // getting glued onto -- the agent's own streamed text chunks.
            if !title.trim().is_empty() {
                sink(RunState::Working, RunStep::Activity { text: title });
            }
        }
        Incoming::PermissionRequest {
            tool_call,
            options,
            respond,
        } => {
            let request = describe(&tool_call);

            // Classify from the tool call's own declared kind (ACP's
            // `toolCall.kind`), failing closed to a write for anything
            // unrecognised or missing -- see
            // `policy::ToolCapability::from_acp_kind`'s doc comment.
            let acp_kind = tool_call.get("kind").and_then(|v| v.as_str());
            let capability = crate::agentdesk::policy::ToolCapability::from_acp_kind(acp_kind);

            if let Err(refusal) = policy.check_tool_capability(started, capability) {
                // Refused BEFORE disk is ever touched, and before the run
                // even shows a gate to the user: this is not something the
                // user needs to approve or deny, it is something this
                // intent is structurally unable to do. Still surfaced on the
                // sink (as a Note, not a Gate) so the transcript honestly
                // says the request was declined rather than silently eating
                // it.
                sink(
                    RunState::Working,
                    RunStep::Note {
                        text: refusal_note(&refusal),
                    },
                );
                let decision = pick(&options, false);
                let _ = respond.send(decision);
                return;
            }

            // R6.4: a HELPER execution (`policy.allowed_paths.is_some()`)
            // that passed the ordinary capability gate above must still stay
            // inside its own path allowance -- enforced here, not merely
            // recorded on the `ExecutionRecord`, so a helper cannot edit a
            // sibling's files just because its role permits writing at all.
            let edit_path = extract_edit_path(&tool_call);
            if let Err(refusal) = policy.check_path_allowance(capability, edit_path.as_deref()) {
                sink(
                    RunState::Working,
                    RunStep::Note {
                        text: refusal_note(&refusal),
                    },
                );
                let decision = pick(&options, false);
                let _ = respond.send(decision);
                return;
            }

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

/// Plain-language sentence for a policy refusal, shown in the transcript so
/// a read-only run that tried to write is visibly explained rather than
/// silently declined with nothing to see.
fn refusal_note(refusal: &crate::agentdesk::policy::ToolRefusal) -> String {
    use crate::agentdesk::policy::ToolRefusal;
    match refusal {
        ToolRefusal::ReadOnlyIntent { .. } => {
            "This chat can only read and explain -- it can't make changes, so that request was turned down.".to_string()
        }
        ToolRefusal::NotStartedYet { .. } => {
            "This plan hasn't been started yet, so that change was turned down. Press Start to let it make changes.".to_string()
        }
        ToolRefusal::PathNotAllowed { path } => {
            format!("This helper can only change its own files, and \"{path}\" isn't one of them, so that change was turned down.")
        }
    }
}

/// Best-effort extraction of the file path a write-shaped tool call names,
/// from ACP's `toolCall.locations` array (`[{ "path": "...": ... }, ...]`).
/// Only the first location is used -- a tool call touching several files at
/// once is not a shape any provider this build talks to produces today, and
/// `check_path_allowance` fails closed (refuses) when this returns `None`
/// rather than guessing a single path covers a multi-file edit.
fn extract_edit_path(tool_call: &serde_json::Value) -> Option<String> {
    tool_call
        .get("locations")
        .and_then(|v| v.as_array())
        .and_then(|locations| locations.first())
        .and_then(|loc| loc.get("path"))
        .and_then(|p| p.as_str())
        .map(|s| s.to_string())
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

    // -- task 1.7: adversarial engine-boundary refusal tests --
    //
    // Every test here calls the real `handle()` the way `run_task`'s select
    // loop does, with a `PermissionRequest` shaped exactly like the live ACP
    // client would raise. `answers` is a channel `handle` must NEVER read
    // from in a refusal case -- proven by closing it before calling `handle`,
    // so a fallback to `answers.recv()` would panic (or hang, in a real
    // run), not merely disagree with the assertion.

    use crate::agentdesk::model::SessionIntent;
    use crate::agentdesk::policy::ExecutionPolicy;
    use std::sync::{mpsc, Arc, Mutex};

    fn policy_for(intent: SessionIntent) -> ExecutionPolicy {
        use crate::agentdesk::policy::{ExecutionMode, ExecutionTeam};
        ExecutionPolicy::resolve(intent, ExecutionMode::Auto, ExecutionTeam::Lead, None)
            .expect("no override always resolves")
    }

    /// A channel whose receiving half is dropped immediately -- any call
    /// that reaches `answers.recv()` observes a closed channel, so a refusal
    /// path that (incorrectly) fell through to reading it fails loudly
    /// rather than silently reading `Err` and calling that "refused."  To
    /// tell the two apart, refusal tests assert the decision came back
    /// WITHOUT ever calling `respond` through the `Cancelled`-on-closed-
    /// channel path (that path never emits a `Note`, only a `Gate`) -- see
    /// each test's own assertions on the sink's recorded steps.
    fn closed_answers() -> mpsc::Receiver<GateAnswer> {
        let (_tx, rx) = mpsc::channel::<GateAnswer>();
        rx
    }

    fn recording_sink() -> (Sink, Arc<Mutex<Vec<(RunState, RunStep)>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let log_for_sink = log.clone();
        let sink: Sink = Arc::new(move |state, step| {
            log_for_sink.lock().unwrap().push((state, step));
        });
        (sink, log)
    }

    fn permission_request(
        kind: &str,
    ) -> (Incoming, tokio::sync::oneshot::Receiver<PermissionDecision>) {
        let (respond, rx) = tokio::sync::oneshot::channel();
        let item = Incoming::PermissionRequest {
            tool_call: serde_json::json!({ "title": "Edit a file", "kind": kind }),
            options: vec![opt("allow_once", "a"), opt("reject_once", "r")],
            respond,
        };
        (item, rx)
    }

    /// The literal scenario task 1.7 asks for: the model asks to write
    /// (`kind: "edit"`) under a read-only intent (Ask). The engine must
    /// refuse before ever touching `answers` -- proven by the channel being
    /// closed and `handle` still returning normally with a `RejectOnce`
    /// decision, plus a `Note` (not a `Gate`) in the transcript.
    #[test]
    fn ask_intent_refuses_a_write_request_before_touching_disk_or_the_answer_channel() {
        let policy = policy_for(SessionIntent::Ask);
        let (sink, log) = recording_sink();
        let answers = closed_answers();
        let (item, mut decision_rx) = permission_request("edit");

        handle(item, &sink, &answers, &policy, false);

        let decision = decision_rx.try_recv().expect("a decision was sent");
        assert!(
            matches!(decision, PermissionDecision::RejectOnce { .. }),
            "expected a reject decision, got {decision:?}"
        );
        let recorded = log.lock().unwrap();
        assert!(
            recorded.iter().any(|(_, step)| matches!(step, RunStep::Note { .. })),
            "a refusal must be visible as a Note in the transcript, got {recorded:?}"
        );
        assert!(
            !recorded.iter().any(|(_, step)| matches!(step, RunStep::Gate { .. })),
            "a refused write must never surface as something the user is asked to approve"
        );
    }

    #[test]
    fn explain_review_and_summarize_also_refuse_writes_before_asking() {
        for intent in [SessionIntent::Explain, SessionIntent::Review, SessionIntent::Summarize] {
            let policy = policy_for(intent);
            let (sink, _log) = recording_sink();
            let answers = closed_answers();
            let (item, mut decision_rx) = permission_request("edit");

            handle(item, &sink, &answers, &policy, false);

            let decision = decision_rx.try_recv().expect("a decision was sent");
            assert!(
                matches!(decision, PermissionDecision::RejectOnce { .. }),
                "{intent:?}: expected a reject decision, got {decision:?}"
            );
        }
    }

    /// Plan before Start: the exact same refusal path, but the reason is
    /// `NotStartedYet` rather than `ReadOnlyIntent`.
    #[test]
    fn plan_before_start_refuses_a_write_request() {
        let policy = policy_for(SessionIntent::Plan);
        let (sink, _log) = recording_sink();
        let answers = closed_answers();
        let (item, mut decision_rx) = permission_request("edit");

        handle(item, &sink, &answers, &policy, false);

        let decision = decision_rx.try_recv().expect("a decision was sent");
        assert!(matches!(decision, PermissionDecision::RejectOnce { .. }));
    }

    /// Plan AFTER Start behaves like Fix: a write request is a real gate the
    /// user sees and answers, not an automatic refusal.
    #[test]
    fn plan_after_start_allows_a_write_request_to_reach_the_user() {
        let policy = policy_for(SessionIntent::Plan);
        let (sink, log) = recording_sink();
        let (answer_tx, answers) = mpsc::channel::<GateAnswer>();
        let (item, mut decision_rx) = permission_request("edit");

        answer_tx.send(GateAnswer::AllowOnce).unwrap();
        handle(item, &sink, &answers, &policy, true);

        let decision = decision_rx.try_recv().expect("a decision was sent");
        assert!(matches!(decision, PermissionDecision::AllowOnce { .. }));
        let recorded = log.lock().unwrap();
        assert!(recorded.iter().any(|(_, step)| matches!(step, RunStep::Gate { .. })));
    }

    /// Every ACP `kind` this build recognises as read-only (`read`, `search`,
    /// `think`, `fetch`) must reach the user's own gate/answer channel even
    /// under a read-only intent -- refusing those would make Review/Summarize
    /// unable to do their actual job (they still call tools, just never
    /// write).
    #[test]
    fn read_only_acp_kinds_are_never_refused_even_under_a_read_only_intent() {
        for kind in ["read", "search", "think", "fetch"] {
            let policy = policy_for(SessionIntent::Review);
            let (sink, log) = recording_sink();
            let (answer_tx, answers) = mpsc::channel::<GateAnswer>();
            let (item, mut decision_rx) = permission_request(kind);

            answer_tx.send(GateAnswer::AllowOnce).unwrap();
            handle(item, &sink, &answers, &policy, false);

            let decision = decision_rx.try_recv().expect("a decision was sent");
            assert!(
                matches!(decision, PermissionDecision::AllowOnce { .. }),
                "{kind}: expected the request to reach the user, got {decision:?}"
            );
            let recorded = log.lock().unwrap();
            assert!(recorded.iter().any(|(_, step)| matches!(step, RunStep::Gate { .. })));
        }
    }

    /// The fail-closed classifier applies here too: a tool call with no
    /// `kind` at all must be refused under a read-only intent, not waved
    /// through because it could not be classified.
    #[test]
    fn a_tool_call_with_no_kind_at_all_is_refused_under_a_read_only_intent() {
        let policy = policy_for(SessionIntent::Ask);
        let (sink, _log) = recording_sink();
        let answers = closed_answers();
        let (respond, mut decision_rx) = tokio::sync::oneshot::channel();
        let item = Incoming::PermissionRequest {
            tool_call: serde_json::json!({ "title": "Do something" }),
            options: vec![opt("allow_once", "a"), opt("reject_once", "r")],
            respond,
        };

        handle(item, &sink, &answers, &policy, false);

        let decision = decision_rx.try_recv().expect("a decision was sent");
        assert!(matches!(decision, PermissionDecision::RejectOnce { .. }));
    }

    /// Fix (an ordinary write-capable intent) is unaffected: a write request
    /// still reaches the user's own approval gate exactly as before this
    /// change.
    #[test]
    fn fix_intent_still_gates_writes_through_the_normal_approval_flow() {
        let policy = policy_for(SessionIntent::Fix);
        let (sink, log) = recording_sink();
        let (answer_tx, answers) = mpsc::channel::<GateAnswer>();
        let (item, mut decision_rx) = permission_request("edit");

        answer_tx.send(GateAnswer::AllowOnce).unwrap();
        handle(item, &sink, &answers, &policy, true);

        let decision = decision_rx.try_recv().expect("a decision was sent");
        assert!(matches!(decision, PermissionDecision::AllowOnce { .. }));
        let recorded = log.lock().unwrap();
        assert!(recorded.iter().any(|(_, step)| matches!(step, RunStep::Gate { .. })));
    }

    // -- R6.4: helper path-allowance enforcement inside handle() --

    fn permission_request_with_path(
        kind: &str,
        path: &str,
    ) -> (Incoming, tokio::sync::oneshot::Receiver<PermissionDecision>) {
        let (respond, rx) = tokio::sync::oneshot::channel();
        let item = Incoming::PermissionRequest {
            tool_call: serde_json::json!({
                "title": "Edit a file",
                "kind": kind,
                "locations": [{ "path": path }],
            }),
            options: vec![opt("allow_once", "a"), opt("reject_once", "r")],
            respond,
        };
        (item, rx)
    }

    #[test]
    fn extract_edit_path_reads_the_first_location() {
        let call = serde_json::json!({ "locations": [{ "path": "src/lib.rs" }, { "path": "src/other.rs" }] });
        assert_eq!(extract_edit_path(&call).as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn extract_edit_path_is_none_when_no_locations_are_present() {
        assert_eq!(extract_edit_path(&serde_json::json!({})), None);
    }

    /// The literal R6.4 scenario: a helper whose `allowed_paths` is
    /// `src/**` tries to edit a file outside that allowance. Refused before
    /// the user ever sees a gate, exactly like the read-only-intent case.
    #[test]
    fn a_helper_edit_outside_its_allowed_paths_is_refused_before_a_gate_is_shown() {
        let policy = ExecutionPolicy::resolve_for_helper(true, vec!["src/**".into()]);
        let (sink, log) = recording_sink();
        let answers = closed_answers();
        let (item, mut decision_rx) = permission_request_with_path("edit", "Cargo.toml");

        handle(item, &sink, &answers, &policy, true);

        let decision = decision_rx.try_recv().expect("a decision was sent");
        assert!(matches!(decision, PermissionDecision::RejectOnce { .. }));
        let recorded = log.lock().unwrap();
        assert!(recorded.iter().any(|(_, step)| matches!(step, RunStep::Note { .. })));
        assert!(!recorded.iter().any(|(_, step)| matches!(step, RunStep::Gate { .. })));
    }

    /// The same helper editing a file that IS inside its allowance reaches
    /// the ordinary approval gate exactly like a lead/solo execution would.
    #[test]
    fn a_helper_edit_inside_its_allowed_paths_reaches_the_approval_gate() {
        let policy = ExecutionPolicy::resolve_for_helper(true, vec!["src/**".into()]);
        let (sink, log) = recording_sink();
        let (answer_tx, answers) = mpsc::channel::<GateAnswer>();
        let (item, mut decision_rx) = permission_request_with_path("edit", "src/lib.rs");

        answer_tx.send(GateAnswer::AllowOnce).unwrap();
        handle(item, &sink, &answers, &policy, true);

        let decision = decision_rx.try_recv().expect("a decision was sent");
        assert!(matches!(decision, PermissionDecision::AllowOnce { .. }));
        let recorded = log.lock().unwrap();
        assert!(recorded.iter().any(|(_, step)| matches!(step, RunStep::Gate { .. })));
    }

    // -- CancelHandle (task 2.2/2.3) --

    #[tokio::test]
    async fn cancel_wakes_a_waiter() {
        let handle = CancelHandle::new();
        let waiter = handle.notify.notified();
        tokio::pin!(waiter);

        // Not yet signaled: the waiter must not resolve on its own within a
        // short window (proven by racing it against a timeout, not by
        // asserting a `Poll::Pending` directly -- this crate has no manual
        // waker plumbing to do that without pulling in a futures test
        // helper).
        let raced = tokio::time::timeout(std::time::Duration::from_millis(20), &mut waiter).await;
        assert!(raced.is_err(), "must not resolve before cancel() is called");

        handle.cancel();
        let raced = tokio::time::timeout(std::time::Duration::from_secs(1), &mut waiter).await;
        assert!(raced.is_ok(), "must resolve once cancel() is called");
    }

    #[test]
    fn cancel_is_idempotent_across_clones() {
        // Calling cancel() more than once (a duplicate Stop click racing
        // the runtime registry, see `execution_registry`'s own
        // `stopping_the_same_execution_twice_is_harmless` test) must never
        // panic, and a clone must signal the same underlying waiter as the
        // original -- `CancelHandle` is handed to the registry as one clone
        // and consumed by `run_task` as another.
        let handle = CancelHandle::new();
        let clone = handle.clone();
        handle.cancel();
        clone.cancel();
        // No assertion beyond "did not panic" -- Notify::notify_one has no
        // observable state to inspect without a waiter attached, and the
        // actual wake behavior is covered by `cancel_wakes_a_waiter` above.
    }
}
