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

use crate::ai::agent::acp::{Incoming, StopReason};
use crate::ai::agent::wire::PermissionDecision;
use crate::ai::agent::cli_agent::CliAgent;
use crate::ai::agent::transport::AgentError;
use crate::agentdesk::graph::JobBudget;
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

/// R6.4: which of a helper's two budget limits actually caused `run_task` to
/// self-cancel, so the reported detail names the real reason rather than
/// reading like an ordinary user-initiated stop or an unexplained hang. Kept
/// as a small internal (non-`Type`) enum -- it never crosses the IPC
/// boundary; the plain sentence it produces is what reaches the transcript
/// via the ordinary `RunStep::Ended { detail, .. }` path every other outcome
/// already uses.
#[derive(Debug, Clone, Copy)]
enum BudgetExceeded {
    Turns { limit: u32 },
    Seconds { limit: u32 },
}

impl BudgetExceeded {
    fn plain_reason(self) -> String {
        match self {
            BudgetExceeded::Turns { limit } => {
                format!("This agent reached its limit of {limit} turns, so it stopped here. Any changes it already made are still there.")
            }
            BudgetExceeded::Seconds { limit } => {
                format!(
                    "This agent reached its time limit of {} minutes, so it stopped here. Any changes it already made are still there.",
                    limit.div_ceil(60)
                )
            }
        }
    }
}

/// The turn-count half of R6.4's enforcement, pulled out of `run_task`'s
/// `select!` body as a pure function so it is directly unit-testable without
/// standing up a real `CliAgent` connection (which `run_task` itself
/// requires and this crate has no mock transport for). `run_task`'s own loop
/// calls this with exactly the values it already tracks (`turns` after
/// incrementing, `budget`), so a change to the cutoff rule only has to be
/// correct here to be correct there.
fn turn_budget_exceeded(turns: u32, budget: Option<JobBudget>) -> Option<BudgetExceeded> {
    let b = budget?;
    if turns >= b.max_turns {
        Some(BudgetExceeded::Turns { limit: b.max_turns })
    } else {
        None
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
///
/// `budget` is R6.4's actual enforcement, not just the `ExecutionRecord`
/// storing a number nobody reads: `None` (the lead, and any solo/Fix/Ask/
/// Plan run -- none of those are helpers with a proposed budget) means no
/// limit, exactly today's behavior. `Some(budget)` is checked on every
/// `ToolCall` (the turn proxy at this boundary -- see this function's
/// `handle` calls below) and via a wall-clock deadline armed once at the
/// start of the loop; either one exceeded self-triggers the SAME `cancel`
/// path a user's own Stop click uses, so a budget-exceeded helper stops
/// exactly as cleanly (worktree preserved, process killed, `Ended` reported)
/// as any other stop -- see `BudgetExceeded`'s doc comment for how the
/// reported reason stays distinct from an ordinary user-initiated stop.
/// Collects what the auditor needs to judge, or `None` when there is nothing
/// worth judging.
///
/// `None` for a run that changed no files: there is no work to have cut
/// corners on, and asking a model to review an empty diff wastes a turn to be
/// told nothing happened.
fn gather_evidence(
    worktree: &std::path::Path,
    spec: &str,
    agent_summary: &str,
) -> Option<crate::agentdesk::auditor::AuditEvidence> {
    use crate::agentdesk::auditor::{clamp_diff, AuditEvidence};

    let repo = git2::Repository::open(worktree).ok()?;

    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    // Against the working tree, not a commit: the changes sit uncommitted
    // until someone presses Keep, so a commit-to-commit diff would be empty
    // for exactly the runs this needs to check.
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts)).ok()?;

    let mut changed_paths = Vec::new();
    let mut text = String::new();
    diff.print(git2::DiffFormat::Patch, |delta, _, line| {
        if let Some(path) = delta.new_file().path().and_then(|p| p.to_str()) {
            let entry = format!("{path}");
            if !changed_paths.contains(&entry) {
                changed_paths.push(entry);
            }
        }
        match line.origin() {
            '+' | '-' | ' ' => text.push(line.origin()),
            _ => {}
        }
        text.push_str(&String::from_utf8_lossy(line.content()));
        true
    })
    .ok()?;

    if changed_paths.is_empty() {
        return None;
    }

    let (diff, diff_truncated) = clamp_diff(&text);
    Some(AuditEvidence {
        spec: spec.to_string(),
        agent_summary: agent_summary.to_string(),
        diff,
        diff_truncated,
        changed_paths,
        // Checks live in the transcript rather than here; the auditor is told
        // plainly that passing checks are weak evidence anyway.
        checks: Vec::new(),
    })
}

/// Checks a finished run's work, and says whether it should keep going.
///
/// Runs on a FRESH session of the same tool. Fresh matters: an agent asked to
/// review its own turn has the whole conversation in front of it, including
/// its own reasoning for why what it did was fine, and agrees with itself. A
/// new session sees only the change and the request.
///
/// Everything about this is best effort. A tool that will not start, a reply
/// that cannot be read, a worktree that cannot be diffed -- each ends the
/// audit as [`Verdict::Unavailable`] and lets the work through, because an
/// audit that did not happen knows nothing about the work and refusing to
/// hand over on that basis would strand people behind a broken check.
async fn audit_finished_work(
    agent: &CliAgent,
    policy: &ExecutionPolicy,
    evidence: crate::agentdesk::auditor::AuditEvidence,
) -> crate::agentdesk::auditor::Verdict {
    use crate::agentdesk::auditor::{audit_prompt, parse_verdict, Verdict};

    // The auditor only reads. It is given the same read-only policy a Review
    // chat gets, so an auditor that decided to "just fix it" cannot -- the
    // whole value of a second opinion is that it did not touch the code.
    let review_policy = ExecutionPolicy::resolve_for_audit(policy);

    let mut conn = match agent.connect(&review_policy, false).await {
        Ok(c) => c,
        Err(e) => {
            return Verdict::Unavailable {
                detail: crate::ai::agent::select::plain_explanation(&e),
            }
        }
    };

    let said = conn.ask(&audit_prompt(&evidence)).await;
    conn.shutdown().await;

    match said {
        Ok(text) => parse_verdict(&text),
        Err(e) => Verdict::Unavailable { detail: e.to_string() },
    }
}

pub async fn run_task(
    agent: &CliAgent,
    task: &str,
    sink: Sink,
    answers: mpsc::Receiver<GateAnswer>,
    policy: ExecutionPolicy,
    started: bool,
    cancel: CancelHandle,
    budget: Option<JobBudget>,
    // `audit_target`: the worktree to read a diff out of when the work is
    // checked over, or `None` to skip the check -- a read-only run has
    // nothing to audit. `spec_text`: what was asked for, as the working agent
    // saw it; empty when the run had no spec behind it.
    audit_target: Option<std::path::PathBuf>,
    spec_text: String,
) {
    let mut conn = match agent.connect(&policy, started).await {
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
    // R6.4: which limit (if any) actually triggered the self-cancel below,
    // so the final report says "reached its limit of N turns" / "ran out of
    // time" rather than reading like an ordinary user Stop. `None` for every
    // path that is not a budget cutoff (including an unbounded `budget:
    // None` run, which never sets this at all).
    let mut budget_exceeded: Option<BudgetExceeded> = None;
    // R6.4: incremented on every `Incoming::ToolCall` -- the discrete unit of
    // agent action visible at this boundary, since the CLI's own internal
    // tool-call loop runs inside one continuous `session/prompt` and this
    // client never sees turn boundaries any finer than that.
    let mut turns: u32 = 0;
    let run_started = tokio::time::Instant::now();

    let outcome: Result<crate::ai::agent::acp::TurnOutcome, AgentError> = {
        let prompt = conn.prompt(task);
        tokio::pin!(prompt);
        // `tokio::time::sleep` needs a fixed deadline to `select!` against
        // repeatedly without being re-created on every loop iteration --
        // `Instant::now() + CANCEL_ACK_TIMEOUT` computed once cancellation
        // is actually requested (never armed at all otherwise, since a
        // never-cancelled run must be able to run indefinitely).
        let mut deadline: Option<tokio::time::Instant> = None;
        // The budget's own wall-clock cutoff, armed once up front (not
        // re-armed per iteration like `deadline` above, which times a
        // cancel ACK rather than the run itself) -- `None` for an unbounded
        // run, matching `deadline`'s "never resolves" inert branch.
        let budget_deadline = budget.map(|b| run_started + Duration::from_secs(b.max_seconds as u64));
        loop {
            let timeout = async {
                match deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    // No deadline armed: never resolves, so this branch is
                    // inert until a cancel actually starts the clock.
                    None => std::future::pending::<()>().await,
                }
            };
            let budget_timeout = async {
                match budget_deadline {
                    Some(d) if !cancel_requested => tokio::time::sleep_until(d).await,
                    _ => std::future::pending::<()>().await,
                }
            };
            tokio::select! {
              result = &mut prompt => break result,
              Some(item) = incoming.recv() => {
                  if matches!(item, Incoming::ToolCall { .. }) {
                      turns += 1;
                  }
                  handle(item, &sink, &answers, &policy, started);
                  // R6.4: a turn-count cutoff is checked right after the turn
                  // that crossed it, not on a timer -- the run must stop
                  // BEFORE its next tool call is answered, not merely at some
                  // point after the Nth one.
                  if !cancel_requested {
                      if let Some(reason) = turn_budget_exceeded(turns, budget) {
                          budget_exceeded = Some(reason);
                          cancel_requested = true;
                          deadline = Some(tokio::time::Instant::now() + CANCEL_ACK_TIMEOUT);
                          let _ = conn.cancel().await;
                      }
                  }
              }
              _ = budget_timeout, if budget_deadline.is_some() && !cancel_requested => {
                  budget_exceeded = Some(BudgetExceeded::Seconds {
                      limit: budget.expect("budget_deadline is only Some when budget is Some").max_seconds,
                  });
                  cancel_requested = true;
                  deadline = Some(tokio::time::Instant::now() + CANCEL_ACK_TIMEOUT);
                  let _ = conn.cancel().await;
              }
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

    // Emitted before the run's terminal step so the durable record carries
    // the cost even for a run that ends as Stopped rather than Finished -- a
    // cancelled turn still spent tokens, and dropping its usage would quietly
    // under-report what the session cost. A clean cancel still answers
    // `session/prompt` (the protocol requires it), so its usage arrives here
    // like any other turn's.
    //
    // What CANNOT be recovered is the turn that never answered at all: the
    // cancel-ACK timeout below breaks with an `Err`, and the usage for that
    // turn was in the reply that never came. That is a real under-report on
    // exactly the most expensive runs (a budget-blown agent that then ignored
    // its cancel), and it is a limit of the transport rather than something
    // this function can paper over -- there is no number here to record.
    // Guessing one would be worse than the gap.
    match &outcome {
        Ok(o) => {
            if let Some(usage) = &o.usage {
                sink(
                    RunState::Working,
                    RunStep::Usage {
                        usage: usage.clone(),
                    },
                );
            }
        }
        Err(e) => {
            log::info!("this turn ended without reporting what it cost: {e}");
        }
    }

    // The work is checked over before it is called finished, and sent back
    // if it is not really done. Only for a run that (a) believed it finished,
    // (b) was allowed to change files, and (c) has a worktree to read a diff
    // out of -- there is nothing to audit about an answer to a question.
    //
    // Every failure here lets the work through. An audit that could not run
    // knows nothing about the work, and stranding people behind a broken
    // check would be worse than the corner-cutting it exists to catch.
    let mut audit_verdict: Option<crate::agentdesk::auditor::Verdict> = None;
    if matches!(outcome.as_ref().map(|o| o.stop_reason), Ok(StopReason::EndTurn)) {
        if let Some(worktree) = audit_target.as_deref() {
            let mut pass = 0u32;
            while pass < crate::agentdesk::auditor::MAX_CORRECTION_PASSES {
                let evidence = match gather_evidence(worktree, &spec_text, "") {
                    Some(e) => e,
                    None => break,
                };
                let verdict = audit_finished_work(agent, &policy, evidence).await;
                sink(RunState::Working, RunStep::Note { text: verdict.summary() });

                let crate::agentdesk::auditor::Verdict::Hollow { reasons } = &verdict else {
                    audit_verdict = Some(verdict);
                    break;
                };

                // Send it back. The correction runs on the SAME connection as
                // the original work, so the agent still has the context it
                // built up -- it is being asked to finish, not to start over.
                let correction = crate::agentdesk::auditor::correction_prompt(reasons);
                audit_verdict = Some(verdict);
                if conn.prompt(&correction).await.is_err() {
                    break;
                }
                pass += 1;
            }
        }
    }

    let (state, detail) = match outcome.map(|o| o.stop_reason) {
        Ok(stop) => match stop {
            StopReason::EndTurn => (
                RunState::Finished,
                "Finished. Your changes are ready to look over.".to_string(),
            ),
            // NOTE: the audit pass runs before this match, in `run_task`'s
            // caller loop -- see `audit_and_maybe_correct`. Reaching here
            // means either the work passed, the audit could not run, or the
            // corrections were used up.
            StopReason::Cancelled => match budget_exceeded {
                // R6.4: the CLI acknowledged the cancel cleanly (the common
                // case -- `CANCEL_ACK_TIMEOUT` is the same 8s either way),
                // but the REASON was this agent's own budget, not a user
                // Stop click. Reported plainly, per the reset doc's "must
                // report plainly why it stopped ... rather than looking like
                // it failed or hung."
                Some(reason) => (RunState::Stopped, reason.plain_reason()),
                None => (
                    RunState::Stopped,
                    "You stopped this run. Nothing was committed.".to_string(),
                ),
            },
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
            //
            // R6.4: when the timeout was itself preceded by a budget cutoff
            // (`budget_exceeded.is_some()`), the message still leads with
            // WHY the stop was requested in the first place, not just that
            // it was slow to happen -- a budget-exceeded helper that also
            // failed to ACK in time must not read as an unexplained hang.
            RunState::Stopped,
            match budget_exceeded {
                Some(reason) => format!(
                    "{} This didn't stop right away, so it was shut down directly. Any changes already made are still there.",
                    reason.plain_reason()
                ),
                None => format!(
                    "This didn't stop right away, so it was shut down directly. {} Any changes already made are still there.",
                    crate::ai::agent::select::plain_explanation(&e)
                ),
            },
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
        // Context occupancy, not spend. Emitted as its own step so the
        // execution record can carry "this session is holding 31k of a 200k
        // window" without it being mistaken for a running total -- the figure
        // REPLACES the previous one and falls when the agent compacts.
        Incoming::ContextUsage {
            used,
            size,
            cost_micro_usd,
        } => {
            sink(
                RunState::Working,
                RunStep::ContextUsage {
                    used,
                    size,
                    cost_micro_usd,
                },
            );
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
        Incoming::PermissionRequest(request) => {
            let crate::ai::agent::wire::PermissionRequest {
                capability,
                path,
                summary,
                respond,
            } = request;
            let gate = GateRequest::Unclassified {
                summary: summary.clone(),
            };

            if let Err(refusal) = policy.check_tool_capability(started, capability) {
                // Refused BEFORE disk is ever touched, and before the run even
                // shows a gate: this is not something the user needs to
                // approve or deny, it is something this run may never do.
                sink(
                    RunState::Working,
                    RunStep::Note {
                        text: refusal_note(&refusal),
                    },
                );
                let _ = respond.send(PermissionDecision::RejectOnce);
                return;
            }

            // R6.4: a HELPER execution (`policy.allowed_paths.is_some()`)
            // that passed the ordinary capability gate above must still stay
            // inside its own path allowance -- enforced here, not merely
            // recorded on the `ExecutionRecord`, so a helper cannot edit a
            // sibling's files just because its role permits writing at all.
            if let Err(refusal) = policy.check_path_allowance(capability, path.as_deref()) {
                sink(
                    RunState::Working,
                    RunStep::Note {
                        text: refusal_note(&refusal),
                    },
                );
                let _ = respond.send(PermissionDecision::RejectOnce);
                return;
            }

            sink(RunState::NeedsYou, RunStep::Gate { request: gate });

            // Blocking here is what "the run fully pauses" means: the agent's
            // turn does not continue until this is answered.
            let decision = match answers.recv() {
                Ok(GateAnswer::AllowOnce) => PermissionDecision::AllowOnce,
                Ok(GateAnswer::FindAnotherWay) => PermissionDecision::RejectOnce,
                // A closed channel means the console went away. Cancelling is
                // the safe reading; treating it as approval would let a run
                // continue with nobody watching.
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




#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::agent::acp::PermissionOption;






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

    /// A request as an ADAPTER would hand it over: already classified.
    ///
    /// `kind` is still an ACP kind string so these tests keep exercising the
    /// same classification rule they always did -- it is just applied here,
    /// where an adapter applies it, rather than inside the gate.
    fn permission_request(
        kind: &str,
    ) -> (Incoming, tokio::sync::oneshot::Receiver<PermissionDecision>) {
        permission_request_with_path(kind, None)
    }

    fn permission_request_with_path(
        kind: &str,
        path: Option<&str>,
    ) -> (Incoming, tokio::sync::oneshot::Receiver<PermissionDecision>) {
        let (respond, rx) = tokio::sync::oneshot::channel();
        let item = Incoming::PermissionRequest(crate::ai::agent::wire::PermissionRequest {
            capability: crate::agentdesk::policy::ToolCapability::from_acp_kind(Some(kind)),
            path: path.map(str::to_string),
            summary: "Edit a file".to_string(),
            respond,
        });
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
        // No kind at all: the adapter's classifier must call this a write, so
        // a tool GitWyrm does not understand cannot slip past a read-only run.
        let (item, mut decision_rx) = permission_request_with_path("", None);

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




    /// The literal R6.4 scenario: a helper whose `allowed_paths` is
    /// `src/**` tries to edit a file outside that allowance. Refused before
    /// the user ever sees a gate, exactly like the read-only-intent case.
    #[test]
    fn a_helper_edit_outside_its_allowed_paths_is_refused_before_a_gate_is_shown() {
        let policy = ExecutionPolicy::resolve_for_helper(true, vec!["src/**".into()]);
        let (sink, log) = recording_sink();
        let answers = closed_answers();
        let (item, mut decision_rx) = permission_request_with_path("edit", Some("Cargo.toml"));

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
        let (item, mut decision_rx) = permission_request_with_path("edit", Some("src/lib.rs"));

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

    // -- R6.4: turn/time budget enforcement --
    //
    // `run_task` itself needs a real `CliAgent` connection to exercise end to
    // end (no mock transport exists in this crate), so these tests cover the
    // two pieces that are pure: the turn-cutoff decision (`turn_budget_exceeded`,
    // which `run_task`'s own `select!` arm calls -- see that call site's own
    // comment) and the plain-language report each cutoff produces
    // (`BudgetExceeded::plain_reason`, which is exactly the string that ends
    // up in the transcript's `RunStep::Ended { detail, .. }`).

    fn budget(max_turns: u32, max_seconds: u32) -> JobBudget {
        JobBudget { max_turns, max_seconds }
    }

    #[test]
    fn no_budget_never_triggers_a_turn_cutoff() {
        assert!(turn_budget_exceeded(0, None).is_none());
        assert!(turn_budget_exceeded(1_000_000, None).is_none());
    }

    #[test]
    fn a_turn_count_below_the_limit_does_not_trigger() {
        assert!(turn_budget_exceeded(4, Some(budget(5, 900))).is_none());
    }

    #[test]
    fn a_turn_count_at_the_limit_triggers() {
        let reason = turn_budget_exceeded(5, Some(budget(5, 900)));
        assert!(matches!(reason, Some(BudgetExceeded::Turns { limit: 5 })));
    }

    #[test]
    fn a_turn_count_past_the_limit_still_triggers() {
        // Defensive: the real call site only ever calls this right after
        // incrementing by exactly one, so `turns` should never overshoot the
        // limit by more than one in practice, but the check itself must not
        // depend on that -- `>=`, not `==`.
        let reason = turn_budget_exceeded(9, Some(budget(5, 900)));
        assert!(matches!(reason, Some(BudgetExceeded::Turns { limit: 5 })));
    }

    #[test]
    fn the_turns_exceeded_report_names_the_actual_limit_in_plain_language() {
        let reason = BudgetExceeded::Turns { limit: 20 };
        let text = reason.plain_reason();
        assert!(text.contains("20 turns"), "expected the limit in the message, got: {text}");
        assert!(
            text.to_lowercase().contains("still there"),
            "must reassure that existing changes were not reverted, got: {text}"
        );
    }

    #[test]
    fn the_seconds_exceeded_report_converts_to_minutes_and_rounds_up() {
        // 901 seconds is just past 15 minutes -- rounding up (not down) means
        // the report never claims a shorter wait than the run actually got.
        let reason = BudgetExceeded::Seconds { limit: 901 };
        let text = reason.plain_reason();
        assert!(text.contains("16 minutes"), "expected a round-up to 16 minutes, got: {text}");
    }

    #[test]
    fn a_budget_of_exactly_one_minute_reports_one_minute_not_zero() {
        let reason = BudgetExceeded::Seconds { limit: 60 };
        assert!(reason.plain_reason().contains("1 minutes"));
    }

    #[test]
    fn the_two_budget_reasons_are_distinguishable_in_their_own_text() {
        // Turns vs. seconds must read as different situations, not the same
        // generic "you were stopped" sentence -- a helper that ran out of
        // turns and one that ran out of time are different things for the
        // user to react to (write a smaller job vs. give it more time).
        let turns_text = BudgetExceeded::Turns { limit: 5 }.plain_reason();
        let seconds_text = BudgetExceeded::Seconds { limit: 300 }.plain_reason();
        assert_ne!(turns_text, seconds_text);
        assert!(turns_text.contains("turns"));
        assert!(seconds_text.contains("minutes"));
    }
}
