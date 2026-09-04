//! The run bridge: turns `airun::RunEventKind` into durable session events.
//!
//! `src-tauri/src/commands/airun.rs`'s `emit()` is the single choke point
//! every run event already flows through on its way to the `ai-run-event`
//! Tauri event (architecture.md section 4). This module adds a *parallel*
//! path from that same point into a durable [`super::model::AgentSession`]:
//! it does not touch `RUN_EVENT`, does not change `emit()`'s signature, and
//! a caller that never wires this module in gets the exact behavior that
//! existed before it (task 4.5).
//!
//! Persist-before-emit (design.md, "Backend commands own canonical content" /
//! "Persist an event before emitting it to the UI") is enforced by
//! [`route_run_event`] returning [`RunEventRouted::Persisted`] only after a
//! successful write -- there is no code path that hands the caller an event
//! to emit without a successful write behind it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::airun::driver::{summarize, RunEventKind, RunState as AirunRunState, RunStep};

use super::events::{AgentSessionEvent, AgentSessionEventKind};
use super::locks::SessionLocks;
use super::model::{
    AgentSession, ExecutionId, ExecutionRecord, MessageId, MessageKind, MessageRole,
    MessageTarget, SegmentId, SessionId, SessionMessage, SessionState,
};
use super::store::{self, SessionStoreRoot, WriteError};

/// The event name `agent-session-event` payloads travel on, mirroring
/// `commands::airun::RUN_EVENT` for the durable path. Kept here (not in
/// `commands/agent_desk.rs`) so the constant lives next to the type it names,
/// the same relationship `RUN_EVENT` has to `RunEventKind`.
pub const AGENT_SESSION_EVENT: &str = "agent-session-event";

/// Which durable [`SessionId`] one EXECUTION's live `airun` run feeds into,
/// when it has been linked.
///
/// Keyed by execution ID, not repository ID (the P0 fix for "an event can
/// reach the wrong chat": the reset audit's own words are "map execution ID
/// to session ID, unlink only that exact mapping"). A run event's own
/// `RunEventKind::session_id` field IS the durable `execution_id` in every
/// real (non-legacy-demo) caller -- see [`execution_id_for_run_session`] --
/// so this map answers "which durable session owns this exact execution's
/// events," never "which session currently owns this repository." Linking a
/// second, third, ... concurrent execution for the SAME repository (two
/// chats open on one repo, or a lead plus helpers) each gets its own entry
/// and none of them overwrite each other, which is the actual bug this
/// replaces: the previous `HashMap<repo_id, SessionId>` shape meant linking
/// session B's execution silently stole routing for every future event that
/// happened to name the same `repo_id`, even one still destined for session
/// A's still-running execution.
///
/// Cheaply `Clone`: every field is behind its own `Arc`, so a clone shares
/// the same underlying maps rather than snapshotting them. This is what lets
/// a command hand an owned copy into a `spawn_blocking` closure (`'static`,
/// crosses threads) without changing how `RunSessionLinks` is registered as
/// Tauri state (`app.manage(RunSessionLinks::new())`, not
/// `Arc<RunSessionLinks>`) -- every existing `app.state::<RunSessionLinks>()`
/// / `tauri::State<'_, RunSessionLinks>` call site keeps working unchanged.
#[derive(Default, Clone)]
pub struct RunSessionLinks {
    /// `execution_id -> session_id`. THE routing table `route_run_event`
    /// consults. Never keyed by repository.
    inner: Arc<Mutex<HashMap<String, SessionId>>>,
    /// Per-execution sequence counters, keyed by the `airun` session ID
    /// (which doubles as the durable `execution_id`, see
    /// [`execution_id_for_run_session`]). A fresh counter per execution is
    /// what makes sequence numbers monotonic *per execution* rather than per
    /// repository -- two executions in the same repository (a retry after a
    /// stop) each start their own transcript at sequence 1.
    sequences: Arc<Mutex<HashMap<String, u32>>>,
}

impl RunSessionLinks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Links `execution_id`'s future run events to durable session
    /// `session_id`. `execution_id` is the `airun` run's own session ID
    /// (`RunEventKind::session_id`), which is the same string as the durable
    /// `ExecutionId` for every real caller (see
    /// [`execution_id_for_run_session`]) -- callers pass whichever they
    /// already have in hand under either name.
    pub fn link(&self, execution_id: &str, session_id: &SessionId) {
        self.inner
            .lock()
            .unwrap()
            .insert(execution_id.to_string(), session_id.clone());
    }

    /// Which durable session `execution_id`'s events belong to, if any.
    pub fn get(&self, execution_id: &str) -> Option<SessionId> {
        self.inner.lock().unwrap().get(execution_id).cloned()
    }

    /// Removes exactly `execution_id`'s mapping. A sibling execution (a
    /// helper, or a second session's own execution that happens to share a
    /// repository) is untouched -- this is the "unlink only that exact
    /// mapping" half of the P0 fix.
    pub fn unlink(&self, execution_id: &str) {
        self.inner.lock().unwrap().remove(execution_id);
    }

    /// The next sequence number for `run_session_id`, starting at 1. Callers
    /// call this exactly once per event, in emission order -- the counter has
    /// no way to notice a call was skipped, which is precisely what lets a
    /// dropped call (rather than a dropped event) manifest as the gap
    /// [`apply_run_event`] is built to tolerate.
    pub fn next_sequence(&self, run_session_id: &str) -> u32 {
        let mut map = self.sequences.lock().unwrap();
        let counter = map.entry(run_session_id.to_string()).or_insert(0);
        *counter += 1;
        *counter
    }

    /// Drops the sequence counter for a finished `airun` session, so a future
    /// reuse of the same session ID (should the counter type ever wrap, or in
    /// a test) starts clean rather than inheriting a stale count.
    ///
    /// NOT CURRENTLY REACHED, and neither is [`Self::next_sequence`]: the
    /// `sequences` map is touched only by those two, and only from tests.
    /// Production sequence numbers arrive on the event itself. Recorded here
    /// rather than deleted because the pair is coherent and the counter is
    /// the obvious home if a transport ever needs GitWyrm to number its own
    /// events -- but a reader should not assume this map is live, and should
    /// not "fix" its growth: nothing fills it.
    ///
    /// If a caller is ever added for `next_sequence`, this must be called
    /// wherever `unlink` is, or the map really will grow one entry per run.
    pub fn forget_sequence(&self, run_session_id: &str) {
        self.sequences.lock().unwrap().remove(run_session_id);
    }
}

/// Maps an `airun` run's own session ID (`RunEventKind::session_id`) to the
/// durable [`ExecutionId`] it corresponds to. `airun` session IDs (`run-1`,
/// `run-2`, ...) already satisfy everything an `ExecutionId` needs -- both
/// are opaque, comparable, `String`-backed identifiers -- so no separate ID
/// is minted; this exists as a named seam so a future divergence between the
/// two ID spaces has exactly one place to change.
pub fn execution_id_for_run_session(run_session_id: &str) -> ExecutionId {
    run_session_id.to_string()
}

/// What happened when a `RunEventKind` was folded into a session.
///
/// Every branch is a state the caller is expected to see in normal use, not
/// a fault: a stale/duplicate event is dropped by design (design.md's
/// "Failure behavior"), not an error condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BridgeOutcome {
    /// The event was folded into `session` and is ready to emit as `event`.
    Applied {
        session: AgentSession,
        event: AgentSessionEvent,
    },
    /// This exact sequence number was already recorded for this execution.
    /// Ignored per design.md ("Duplicate event sequence: ignore it").
    DuplicateSequence { execution_id: ExecutionId, sequence: u32 },
    /// The event's execution is no longer the session's active one -- the
    /// session moved on to a replacement execution. Ignored in the backend
    /// per design.md ("Event for replaced execution: ignore it in both
    /// backend and frontend") and spec.md's "Late output" scenario.
    ExecutionSuperseded { execution_id: ExecutionId },
}

/// Folds one `RunEventKind` into `session`, in place, and returns the durable
/// event ready to persist-then-emit -- or the reason it was not applied.
///
/// Callers own persistence: this function only mutates the in-memory
/// `session`; nothing here touches disk. That keeps the ordering rule
/// (persist before emit) enforced by the caller sequencing `store::write_session`
/// before `app.emit`, not by this function reaching for a store itself, which
/// is what keeps it a plain, synchronous, exhaustively-testable function.
pub fn apply_run_event(
    session: &mut AgentSession,
    execution_id: &ExecutionId,
    sequence: u32,
    occurred_at: &str,
    event: &RunEventKind,
) -> BridgeOutcome {
    // Whether `execution_id` already has a record naming it a HELPER
    // (`parent_execution_id.is_some()`) -- R6.3/R6.7: a helper's events must
    // keep landing on its own `ExecutionRecord` and must never be treated as
    // "superseded" just because the session's single `active_execution_id`
    // slot names the lead (or a different helper). `active_execution_id` is
    // a lead-track concept only -- see `find_or_start_execution`'s own doc
    // comment on why a brand-new id is never rejected here, and this is the
    // same reasoning extended to an id that already exists but is a helper's.
    let is_known_helper = session
        .executions
        .iter()
        .any(|e| &e.execution_id == execution_id && e.parent_execution_id.is_some());

    // "Event for a replaced execution": this execution has already appeared
    // in the session (it has a record) but is no longer the active LEAD
    // track. A never-before-seen execution ID is always allowed to start and
    // become active -- that is how a session's second, third, ... execution
    // (a retry, a follow-up) is meant to begin. It is only an event arriving
    // *after* its lead execution was superseded by another that is stale.
    // Helper executions are exempt: `active_execution_id` never names a
    // helper (see `commit_started_graph_if_still_proposed`, which sets it to
    // the lead), so a helper record is never "the active one" by that
    // check's original meaning, and applying it to helpers would silently
    // drop every helper event forever the moment the graph starts.
    let already_known = session
        .executions
        .iter()
        .any(|e| &e.execution_id == execution_id);
    if already_known && !is_known_helper {
        if let Some(active) = &session.header.active_execution_id {
            if active != execution_id {
                return BridgeOutcome::ExecutionSuperseded {
                    execution_id: execution_id.clone(),
                };
            }
        }
    }

    let record = find_or_start_execution(session, execution_id, occurred_at);

    // "Duplicate event sequence": this execution has already recorded this
    // sequence number or a later one. `last_sequence` starts at 0 (no events
    // yet), so sequence 0 would collide with "nothing recorded yet" -- the
    // sequencer is expected to start at 1, and `record.last_sequence == 0`
    // means "never applied," never "applied sequence 0."
    if record.last_sequence != 0 && sequence <= record.last_sequence {
        return BridgeOutcome::DuplicateSequence {
            execution_id: execution_id.clone(),
            sequence,
        };
    }

    // A gap (sequence skips ahead of last_sequence + 1) is still persisted,
    // per design.md: "Sequence gap: persist and expose a diagnostic flag."
    // The flag is the returned event carrying the sequence as-is (not
    // renumbered), which is what lets a frontend listener notice the jump;
    // there is no separate stored flag because the gap is fully recoverable
    // from `sequence` vs. the previous message's `sequence` for the same
    // execution.
    let record_is_helper = record.parent_execution_id.is_some();

    record.last_sequence = sequence;
    record.state = map_run_state(event.state);
    if !matches!(event.state, AirunRunState::Finished | AirunRunState::Stopped | AirunRunState::Failed) {
        record.ended_at = None;
    } else {
        record.ended_at = Some(occurred_at.to_string());
    }

    // Usage folds into the execution's running total and produces no
    // transcript row -- see `RunStep::Usage`. Done here, while the record is
    // still borrowed, so the accumulated figure is written by the same
    // persist-then-emit path as everything else: a usage number that reached
    // the UI but not the disk would reappear as a smaller total after a
    // restart.
    if let RunStep::Usage { usage } = &event.step {
        record.usage.get_or_insert_with(Default::default).accumulate(usage);
    }

    // Occupancy replaces rather than accumulates -- see `set_context`. A cost
    // reported alongside it is cumulative for the session, so it is recorded
    // the same way rather than being added to the per-turn total.
    if let RunStep::ContextUsage { used, size, cost_micro_usd } = &event.step {
        let slot = record.usage.get_or_insert_with(Default::default);
        slot.set_context(*used, *size);
        if let Some(c) = cost_micro_usd {
            slot.cost_micro_usd = Some(*c);
        }
    }

    let kind = map_run_step(session, execution_id, sequence, occurred_at, event);

    match &kind {
        AgentSessionEventKind::MessageAppended { message } => {
            session.messages.push(message.clone());
        }
        // Coalescing (see `map_run_step`'s doc comment): the message already
        // exists at this id, so it is replaced in place rather than pushed
        // again -- this is what keeps one streaming reply as one transcript
        // row. `map_run_step` only ever returns `MessageUpdated` for a
        // `message_id` it already found at `session.messages.last()`, so
        // this always finds a match; falling back to a push if it somehow
        // did not would silently resurrect the exact per-chunk-message bug
        // this feature exists to fix, so a missing match is a bug to see
        // (debug_assert) rather than paper over.
        AgentSessionEventKind::MessageUpdated { message } => {
            match session.messages.last_mut() {
                Some(last) if last.message_id == message.message_id => {
                    *last = message.clone();
                }
                _ => {
                    debug_assert!(
                        false,
                        "MessageUpdated for a message_id not at the end of session.messages"
                    );
                    session.messages.push(message.clone());
                }
            }
        }
        AgentSessionEventKind::StateChanged { .. } | AgentSessionEventKind::ExecutionSuperseded { .. } => {}
    }
    // The session's own header state always tracks its LEAD execution's
    // state, not only on the `Ended` step that produces an explicit
    // `StateChanged` event -- a run entering `Preparing`/`Working`/`NeedsYou`
    // moves the session there too, it just does so alongside a transcript
    // message instead of as the event's sole content.
    //
    // A helper's own event must NEVER overwrite this (R6.3/R6.7): before this
    // guard, a helper finishing after the lead had already moved on to
    // reviewing results would flip `session.header.state` back to
    // `Finished`/`Working` and, via `find_or_start_execution`'s brand-new-id
    // branch, `active_execution_id` itself, hijacking the whole session's
    // displayed status away from the lead the user is actually watching. The
    // Graph panel still sees every helper's own state through
    // `session.executions` (`agentGraphProjection.ts`, `graph::project_graph`)
    // regardless of this guard -- only the single session-wide header summary
    // is scoped to the lead.
    if !record_is_helper {
        session.header.state = map_run_state(event.state);
    }
    session.header.updated_at = occurred_at.to_string();

    let durable = AgentSessionEvent {
        session_id: session.header.session_id.clone(),
        execution_id: Some(execution_id.clone()),
        sequence,
        occurred_at: occurred_at.to_string(),
        kind,
    };

    BridgeOutcome::Applied {
        session: session.clone(),
        event: durable,
    }
}

/// What happened when a live `airun::RunEventKind` was routed toward the
/// durable store. Every branch is something the caller can act on without
/// needing to distinguish "no error" from "no session" -- both leave
/// `ai-run-event` as the only thing that fired, which is correct behavior,
/// not a degraded one, for a repository that was never linked to a durable
/// session.
#[derive(Debug)]
pub enum RunEventRouted {
    /// No durable session is linked to this repository. Nothing was read,
    /// nothing was written -- the caller emits `ai-run-event` only, exactly
    /// as it always has (task 4.5).
    NoLinkedSession,
    /// A session was linked but its file could not be read at all (deleted,
    /// or damaged in a way that is not this bridge's job to repair).
    SessionUnavailable,
    /// The event was folded into the session per [`apply_run_event`] but was
    /// not new content -- a duplicate sequence or a superseded execution.
    /// Nothing was written to disk because nothing changed.
    Ignored(BridgeOutcome),
    /// The event was written to disk. `event` is what the caller should emit
    /// as `agent-session-event`, now that the write behind it has succeeded.
    Persisted { event: AgentSessionEvent },
    /// The session was updated in memory but the write to disk failed. Per
    /// design.md ("persist an event before emitting it to the UI") and task
    /// 4.3, this must not be emitted -- a write failure here is the one case
    /// where the durable path produces neither a persisted record nor a UI
    /// event, matching architecture.md's "a window crash cannot show content
    /// that was never saved."
    WriteFailed { detail: String },
}

/// The full persist-then-emit step for one `airun` event, given a store root
/// and the registry of which repository is linked to which durable session.
///
/// This is the function `commands/airun.rs::emit()` calls in addition to
/// (never instead of) its existing `app.emit(RUN_EVENT, event)` -- see that
/// function's own doc comment for why the two paths are independent.
///
/// The read-modify-write runs under `locks`' mutex for `session_id`, exactly
/// like every mutating command in `commands/agent_desk.rs` -- otherwise a
/// user action (rename, archive, mark-read, append) racing this call could
/// read the same stale state and clobber whichever of the two wrote last.
/// `links.get(&event.session_id)` is called *before* the session lock is
/// taken and `RunSessionLinks` is never touched again inside this function,
/// so there is exactly one lock-acquisition order in play -- see
/// `agentdesk::locks`'s module doc for the full argument.
///
/// Looked up by `event.session_id` (the `airun` run's own ID, which IS the
/// durable `execution_id` -- see [`execution_id_for_run_session`]), never by
/// `event.repo_id`. This is the P0 routing fix: two sessions with live
/// executions against the same repository each keep their own mapping, so an
/// event for one never lands in the other just because both name the same
/// repository.
pub fn route_run_event(
    root: &SessionStoreRoot,
    links: &RunSessionLinks,
    locks: &SessionLocks,
    sequence: u32,
    occurred_at: &str,
    event: &RunEventKind,
) -> RunEventRouted {
    let Some(session_id) = links.get(&event.session_id) else {
        return RunEventRouted::NoLinkedSession;
    };

    locks.with_session_lock(&session_id, || {
        let mut session = match store::read_session(root, &session_id) {
            Ok(s) => s,
            Err(_) => return RunEventRouted::SessionUnavailable,
        };

        let execution_id = execution_id_for_run_session(&event.session_id);
        let outcome = apply_run_event(&mut session, &execution_id, sequence, occurred_at, event);

        let BridgeOutcome::Applied { session: updated, event: durable } = outcome else {
            return RunEventRouted::Ignored(outcome);
        };

        match store::write_session(root, &updated) {
            Ok(()) => RunEventRouted::Persisted { event: durable },
            Err(WriteError::Serialize { detail })
            | Err(WriteError::WriteTemp { detail })
            | Err(WriteError::Flush { detail }) => RunEventRouted::WriteFailed { detail },
            Err(e) => RunEventRouted::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

/// Finds `session`'s execution record for `execution_id`, creating (and
/// activating) one if this is that execution's first event.
fn find_or_start_execution<'a>(
    session: &'a mut AgentSession,
    execution_id: &ExecutionId,
    occurred_at: &str,
) -> &'a mut ExecutionRecord {
    let already_present = session
        .executions
        .iter()
        .any(|e| &e.execution_id == execution_id);

    if !already_present {
        session.executions.push(ExecutionRecord::minimal(
            execution_id.clone(),
            session.header.session_id.clone(),
            None,
            SessionState::Working,
            occurred_at.to_string(),
            None,
            0,
        ));
        session.header.active_execution_id = Some(execution_id.clone());
    }

    session
        .executions
        .iter_mut()
        .find(|e| &e.execution_id == execution_id)
        .expect("just inserted or already present")
}

/// `airun::RunState` -> the durable `SessionState` an execution/session
/// carries. Session states beyond an execution's own lifecycle (`Draft`,
/// `MissingSource`) never come from a run event, so they have no case here.
fn map_run_state(state: AirunRunState) -> SessionState {
    match state {
        AirunRunState::Preparing => SessionState::Preparing,
        AirunRunState::Working => SessionState::Working,
        AirunRunState::NeedsYou => SessionState::NeedsInput,
        AirunRunState::Finished => SessionState::Finished,
        AirunRunState::Stopped => SessionState::Stopped,
        AirunRunState::Failed => SessionState::Failed,
    }
}

/// Maps one `RunStep` into the durable event kind for it, without losing any
/// of the variant's own typed fields.
///
/// Every branch that produces a message carries the *entire* `RunStep` as
/// `rendered_content` (serialized JSON, via `RunStep`'s own `Serialize`) in
/// addition to the plain-language `summarize()` sentence in `plain_content` --
/// that is what makes this a lossless mapping rather than a flattening into
/// display text: a reader that wants `added`/`removed` counts, a `GateRequest`,
/// or a `PreflightItem` list back gets it by deserializing `rendered_content`,
/// not by re-parsing a sentence.
///
/// Coalescing consecutive `Note` steps: `cli_run.rs::handle` turns every
/// streamed text chunk from the provider CLI into its own `RunStep::Note`
/// (`Incoming::TextChunk`), which used to mean one durable session message
/// per chunk -- a chat reply of any length rendered as a wall of one-line
/// messages, each its own JSON envelope. A `Note` step whose immediately
/// preceding message (`session.messages.last()`) is itself an uncoalesced
/// `Note` from the *same execution* is folded into that message instead of
/// appended as a new one: `coalescing_note` decides eligibility, and this
/// function returns `MessageUpdated` (not `MessageAppended`) in that case, so
/// one streaming reply grows as one transcript row. Any other step kind --
/// `Edit`, `Check`, `Gate`, `YouSaid`, `Activity`, `Adapted`, `Preflight`,
/// `Ended` -- is never coalesced (`coalescing_note` only matches `Note`) and,
/// once appended, ends the run: the next `Note` after it finds a non-`Note`
/// last message and starts a fresh row. `Activity` (tool calls) is the
/// concrete case this matters for: a tool call between two text chunks must
/// not glue them into one message across it, and must not itself be treated
/// as prose to fold into.
///
/// Deliberately decided here (not in `cli_run.rs`): `cli_run.rs`'s `Sink`
/// also feeds the existing append-only AI-run console (`ai-run-event`),
/// whose one-row-per-chunk behavior is unrelated to this bug and must not
/// change. Folding happens only on the durable session path, which is what
/// `apply_run_event`/`map_run_step` already exclusively own.
fn map_run_step(
    session: &AgentSession,
    execution_id: &ExecutionId,
    sequence: u32,
    occurred_at: &str,
    event: &RunEventKind,
) -> AgentSessionEventKind {
    // `Ended` is a state transition first and a transcript entry second: the
    // session/execution state itself is what listeners actually branch on,
    // so it is mapped to `StateChanged` rather than `MessageAppended` -- a
    // `RunStep::Ended` still fully determines `event.state` above.
    //
    // Its `detail` IS dropped here, though -- this comment used to claim no
    // field was lost, which was wrong and hid the fact that seven distinct
    // ending sentences collapsed into three states. `cli_run` now emits that
    // sentence as a `Note` just before the `Ended` step, so it reaches the
    // transcript as a message and this mapping stays a pure state change.
    if let RunStep::Ended { state, .. } = &event.step {
        return AgentSessionEventKind::StateChanged {
            state: map_run_state(*state),
        };
    }

    // Usage is bookkeeping, not conversation: it has already been folded into
    // the execution record by `apply_run_event`, and appending a transcript
    // row for it would put "Recorded what the turn cost" in the user's chat.
    // Reported as a state change so a listener still refreshes the usage
    // card without a message appearing.
    if let RunStep::Usage { .. } | RunStep::ContextUsage { .. } = &event.step {
        return AgentSessionEventKind::StateChanged {
            state: map_run_state(event.state),
        };
    }

    if let RunStep::Note { text } = &event.step {
        if let Some(previous) = coalescing_note(session, execution_id) {
            let message = update_message_for_coalesced_note(previous, text, sequence, occurred_at);
            return AgentSessionEventKind::MessageUpdated { message };
        }
    }

    let message = build_message(session, execution_id, sequence, occurred_at, event);
    AgentSessionEventKind::MessageAppended { message }
}

/// The session's last message, if it is eligible to have a new `Note` step
/// folded into it: same execution, and itself an uncoalesced `Note` (never
/// anything else -- an `Edit`/`Check`/`Gate`/etc. message is never a valid
/// coalescing target even though some of those also render as
/// `MessageKind::Assistant`).
///
/// Reads `rendered_content` back into a `RunStep` rather than adding a
/// separate "this message is a coalescible Note" flag to `SessionMessage` or
/// `ExecutionRecord`: `rendered_content` already carries the full step
/// losslessly (see `map_run_step`'s doc comment), so this is the one place
/// that needs to know "was the last step a Note" and it can answer that
/// without a schema change or an extra piece of state to keep in sync.
fn coalescing_note<'a>(session: &'a AgentSession, execution_id: &ExecutionId) -> Option<&'a SessionMessage> {
    let last = session.messages.last()?;
    if last.execution_id.as_ref() != Some(execution_id) {
        return None;
    }
    let rendered = last.rendered_content.as_deref()?;
    let step: RunStep = serde_json::from_str(rendered).ok()?;
    if matches!(step, RunStep::Note { .. }) {
        Some(last)
    } else {
        None
    }
}

/// Builds the replacement for `previous` once a new `Note` chunk is folded
/// into it: text is concatenated directly (the provider CLI's chunks already
/// carry their own inter-word whitespace, so inserting a separator here would
/// double it up), and `message_id`/`segment_id`/`role`/`kind`/`targets` are
/// carried over unchanged -- only content, timestamp, and sequence advance,
/// matching `AgentSessionEventKind::MessageUpdated`'s doc comment.
fn update_message_for_coalesced_note(
    previous: &SessionMessage,
    new_text: &str,
    sequence: u32,
    occurred_at: &str,
) -> SessionMessage {
    let combined_text = format!("{}{}", previous.plain_content, new_text);
    let combined_step = RunStep::Note {
        text: combined_text.clone(),
    };
    SessionMessage {
        message_id: previous.message_id.clone(),
        segment_id: previous.segment_id.clone(),
        role: previous.role,
        timestamp: occurred_at.to_string(),
        plain_content: combined_text,
        rendered_content: serde_json::to_string(&combined_step).ok(),
        provider: previous.provider.clone(),
        model: previous.model.clone(),
        kind: previous.kind,
        execution_id: previous.execution_id.clone(),
        sequence: Some(sequence),
        import: previous.import.clone(),
        targets: previous.targets.clone(),
    }
}

fn build_message(
    session: &AgentSession,
    execution_id: &ExecutionId,
    sequence: u32,
    occurred_at: &str,
    event: &RunEventKind,
) -> SessionMessage {
    let segment_id: SegmentId = session
        .segments
        .last()
        .map(|s| s.segment_id.clone())
        .unwrap_or_default();

    let kind = message_kind_for_step(&event.step);
    let role = match kind {
        MessageKind::User => MessageRole::User,
        MessageKind::System => MessageRole::System,
        _ => MessageRole::Assistant,
    };

    let targets = match &event.step {
        RunStep::Edit { path, .. } => vec![MessageTarget::File { path: path.clone() }],
        _ => Vec::new(),
    };

    let rendered = serde_json::to_string(&event.step).ok();

    SessionMessage {
        message_id: new_message_id(execution_id, sequence),
        segment_id,
        role,
        timestamp: occurred_at.to_string(),
        plain_content: summarize(&event.step),
        rendered_content: rendered,
        provider: None,
        model: None,
        kind,
        execution_id: Some(execution_id.clone()),
        sequence: Some(sequence),
        import: None,
        targets,
    }
}

/// Deterministic message IDs (rather than a random UUID) keep this function
/// pure and its tests reproducible; `(execution, sequence)` is already unique
/// per message because sequence is monotonic per execution.
fn new_message_id(execution_id: &ExecutionId, sequence: u32) -> MessageId {
    format!("{execution_id}-{sequence}")
}

fn message_kind_for_step(step: &RunStep) -> MessageKind {
    match step {
        RunStep::Preflight { .. } => MessageKind::System,
        RunStep::Plan { .. } => MessageKind::Assistant,
        RunStep::Edit { .. } => MessageKind::Tool,
        RunStep::Check { .. } => MessageKind::Tool,
        RunStep::Gate { .. } => MessageKind::Approval,
        RunStep::YouSaid { .. } => MessageKind::User,
        RunStep::Note { .. } => MessageKind::Assistant,
        // Tool activity, never assistant prose -- this is what keeps "Finding
        // files matching **/tasks.md" out of the chat transcript and routes
        // it to the compact activity feed instead (`EventStack`, grouped by
        // `agentDeskEvents.ts::groupEventStacks`, which already groups every
        // `MessageKind::Tool` message the same way `Edit`/`Check` are).
        RunStep::Activity { .. } => MessageKind::Tool,
        RunStep::Adapted { .. } => MessageKind::Assistant,
        // Handled before this is reached (see `map_run_step`); kept here so
        // the match stays exhaustive against every `RunStep` variant rather
        // than relying on the caller's short-circuit, which would silently
        // stop being exhaustive if `map_run_step`'s early return were ever
        // removed.
        RunStep::Ended { .. } => MessageKind::System,
        // Also handled before this is reached (see `map_run_step`), for the
        // same reason `Ended` is: kept here only to keep the match
        // exhaustive.
        RunStep::Usage { .. } => MessageKind::System,
        // Also handled before this is reached, for the same reason as `Usage`.
        RunStep::ContextUsage { .. } => MessageKind::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::{
        AgentSessionHeader, SessionIntent, SessionSource, CURRENT_SCHEMA_VERSION,
    };
    use crate::airun::driver::{GateRequest, PreflightItem};

    fn header() -> AgentSessionHeader {
        AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: "sess-1".into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/proj".into(),
            repo_name: "proj".into(),
            title: "A session".into(),
            source: SessionSource::Manual {
                repo_id: "repo-1".into(),
            },
            intent: SessionIntent::Fix,
            state: SessionState::Ready,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            unread: false,
            changed_file_count: 0,
            active_execution_id: None,
            archived: false,
            graph_started_at: None,
            preferred_provider: None,
            preferred_mode: None,
            preferred_team: None,
        }
    }

    fn session() -> AgentSession {
        AgentSession::new(header())
    }

    fn run_event(state: AirunRunState, step: RunStep) -> RunEventKind {
        RunEventKind {
            repo_id: "repo-1".into(),
            session_id: "run-1".into(),
            state,
            summary: summarize(&step),
            step,
        }
    }

    #[test]
    fn the_first_event_starts_and_activates_an_execution() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");
        let event = run_event(AirunRunState::Working, RunStep::Note { text: "go".into() });

        let outcome = apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &event);
        let BridgeOutcome::Applied { session: applied, .. } = outcome else {
            panic!("expected Applied");
        };
        assert_eq!(applied.executions.len(), 1);
        assert_eq!(applied.executions[0].execution_id, exec);
        assert_eq!(applied.executions[0].last_sequence, 1);
        assert_eq!(applied.header.active_execution_id, Some(exec));
    }

    #[test]
    fn every_run_step_variant_survives_the_mapping_losslessly() {
        let steps = vec![
            RunStep::Preflight {
                items: vec![PreflightItem {
                    label: "Read the spec".into(),
                    done: true,
                    detail: "ok".into(),
                }],
            },
            RunStep::Plan {
                text: "Read the file".into(),
            },
            RunStep::Edit {
                path: "src/a.rs".into(),
                added: 3,
                removed: 1,
            },
            RunStep::Check {
                name: "typecheck".into(),
                passed: true,
                detail: "no errors".into(),
            },
            RunStep::Gate {
                request: GateRequest::RunInstall {
                    command: "npm i".into(),
                },
            },
            RunStep::YouSaid {
                text: "use tabs".into(),
            },
            RunStep::Note {
                text: "thinking".into(),
            },
            RunStep::Activity {
                text: "Finding files matching **/tasks.md".into(),
            },
            RunStep::Adapted {
                text: "did it another way".into(),
            },
        ];

        for (i, step) in steps.into_iter().enumerate() {
            let mut session = session();
            let exec = execution_id_for_run_session("run-1");
            let event = run_event(AirunRunState::Working, step.clone());
            let seq = (i as u32) + 1;
            let outcome = apply_run_event(&mut session, &exec, seq, "2026-01-01T00:00:01Z", &event);
            let BridgeOutcome::Applied { event: durable, .. } = outcome else {
                panic!("expected Applied for {step:?}");
            };
            let AgentSessionEventKind::MessageAppended { message } = durable.kind else {
                panic!("expected MessageAppended for {step:?}");
            };
            let rendered = message
                .rendered_content
                .expect("every mapped step carries its full data as rendered_content");
            let round_tripped: RunStep =
                serde_json::from_str(&rendered).expect("rendered_content must be valid RunStep json");
            assert_eq!(round_tripped, step, "lossy mapping for {step:?}");
        }
    }

    #[test]
    fn an_edit_step_carries_a_file_target() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");
        let event = run_event(
            AirunRunState::Working,
            RunStep::Edit {
                path: "src/a.rs".into(),
                added: 1,
                removed: 0,
            },
        );
        let outcome = apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &event);
        let BridgeOutcome::Applied { event: durable, .. } = outcome else {
            panic!("expected Applied");
        };
        let AgentSessionEventKind::MessageAppended { message } = durable.kind else {
            panic!("expected MessageAppended");
        };
        assert_eq!(
            message.targets,
            vec![MessageTarget::File { path: "src/a.rs".into() }]
        );
    }

    #[test]
    fn ended_is_a_state_change_not_a_transcript_message() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");
        let event = run_event(
            AirunRunState::Finished,
            RunStep::Ended {
                state: AirunRunState::Finished,
                detail: "Finished.".into(),
            },
        );
        let outcome = apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &event);
        let BridgeOutcome::Applied { session: applied, event: durable } = outcome else {
            panic!("expected Applied");
        };
        assert!(matches!(
            durable.kind,
            AgentSessionEventKind::StateChanged { state: SessionState::Finished }
        ));
        assert_eq!(applied.header.state, SessionState::Finished);
        assert!(applied.messages.is_empty());
        assert!(applied.executions[0].ended_at.is_some());
    }

    #[test]
    fn a_duplicate_sequence_is_ignored() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");
        let event = run_event(AirunRunState::Working, RunStep::Note { text: "a".into() });
        apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &event);

        let before = session.clone();
        let outcome = apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:02Z", &event);
        assert!(matches!(
            outcome,
            BridgeOutcome::DuplicateSequence { sequence: 1, .. }
        ));
        assert_eq!(session, before, "a duplicate must not mutate the session");
    }

    #[test]
    fn an_earlier_sequence_arriving_late_is_also_a_duplicate() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");
        let e1 = run_event(AirunRunState::Working, RunStep::Note { text: "a".into() });
        let e2 = run_event(AirunRunState::Working, RunStep::Note { text: "b".into() });
        apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &e1);
        apply_run_event(&mut session, &exec, 2, "2026-01-01T00:00:02Z", &e2);

        let before = session.clone();
        let outcome = apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:03Z", &e1);
        assert!(matches!(
            outcome,
            BridgeOutcome::DuplicateSequence { sequence: 1, .. }
        ));
        assert_eq!(session, before);
    }

    #[test]
    fn a_sequence_gap_is_persisted_and_visible_on_the_event() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");
        let event = run_event(AirunRunState::Working, RunStep::Note { text: "a".into() });
        apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &event);

        // Sequence jumps from 1 straight to 5.
        let outcome = apply_run_event(&mut session, &exec, 5, "2026-01-01T00:00:02Z", &event);
        let BridgeOutcome::Applied { session: applied, event: durable } = outcome else {
            panic!("expected Applied -- a gap is persisted, not dropped");
        };
        assert_eq!(durable.sequence, 5);
        assert_eq!(applied.executions[0].last_sequence, 5);
        assert_eq!(
            applied.messages.last().unwrap().sequence,
            Some(5),
            "the gap must be visible: sequence is not renumbered to 2"
        );
    }

    #[test]
    fn an_event_for_a_replaced_execution_is_superseded_and_not_applied() {
        let mut session = session();
        let exec1 = execution_id_for_run_session("run-1");
        let exec2 = execution_id_for_run_session("run-2");
        let event = run_event(AirunRunState::Working, RunStep::Note { text: "a".into() });

        apply_run_event(&mut session, &exec1, 1, "2026-01-01T00:00:01Z", &event);
        // A second execution takes over (e.g. a retry).
        apply_run_event(&mut session, &exec2, 1, "2026-01-01T00:00:02Z", &event);

        let before = session.clone();
        let late_event = run_event(AirunRunState::Working, RunStep::Note { text: "late".into() });
        let outcome = apply_run_event(&mut session, &exec1, 2, "2026-01-01T00:00:03Z", &late_event);
        assert!(matches!(
            outcome,
            BridgeOutcome::ExecutionSuperseded { execution_id } if execution_id == exec1
        ));
        assert_eq!(session, before, "a superseded event must not mutate the session");
    }

    #[test]
    fn the_session_id_on_the_durable_event_matches_the_session_not_the_run() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");
        let event = run_event(AirunRunState::Working, RunStep::Note { text: "a".into() });
        let outcome = apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &event);
        let BridgeOutcome::Applied { event: durable, .. } = outcome else {
            panic!("expected Applied");
        };
        assert_eq!(durable.session_id, "sess-1");
        assert_eq!(durable.execution_id, Some(exec));
    }

    #[test]
    fn state_advances_through_a_full_run_and_a_gate_is_preserved_as_approval() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");

        let preflight = run_event(
            AirunRunState::Preparing,
            RunStep::Preflight { items: vec![] },
        );
        apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &preflight);
        assert_eq!(session.header.state, SessionState::Preparing);

        let gate = run_event(
            AirunRunState::NeedsYou,
            RunStep::Gate {
                request: GateRequest::RunInstall {
                    command: "npm i".into(),
                },
            },
        );
        apply_run_event(&mut session, &exec, 2, "2026-01-01T00:00:02Z", &gate);
        assert_eq!(session.header.state, SessionState::NeedsInput);
        assert_eq!(session.messages.last().unwrap().kind, MessageKind::Approval);

        let ended = run_event(
            AirunRunState::Finished,
            RunStep::Ended {
                state: AirunRunState::Finished,
                detail: "Finished.".into(),
            },
        );
        apply_run_event(&mut session, &exec, 3, "2026-01-01T00:00:03Z", &ended);
        assert_eq!(session.header.state, SessionState::Finished);
    }

    // -- RunSessionLinks --

    #[test]
    fn sequence_numbers_start_at_one_and_increment_per_execution() {
        let links = RunSessionLinks::new();
        assert_eq!(links.next_sequence("run-1"), 1);
        assert_eq!(links.next_sequence("run-1"), 2);
        assert_eq!(links.next_sequence("run-1"), 3);
    }

    #[test]
    fn two_executions_have_independent_sequence_counters() {
        let links = RunSessionLinks::new();
        assert_eq!(links.next_sequence("run-1"), 1);
        assert_eq!(links.next_sequence("run-2"), 1, "a different execution starts its own count");
        assert_eq!(links.next_sequence("run-1"), 2);
    }

    #[test]
    fn a_link_can_be_set_read_and_removed() {
        let links = RunSessionLinks::new();
        assert_eq!(links.get("exec-1"), None);
        links.link("exec-1", &"sess-1".to_string());
        assert_eq!(links.get("exec-1"), Some("sess-1".to_string()));
        links.unlink("exec-1");
        assert_eq!(links.get("exec-1"), None);
    }

    /// The P0 fix's actual proof at this layer: linking a second execution
    /// must never disturb a first execution's own mapping, even when both
    /// belong to the same repository (not modeled here at all -- this type
    /// no longer has any notion of repository) or the same session.
    #[test]
    fn linking_a_second_execution_does_not_overwrite_the_first() {
        let links = RunSessionLinks::new();
        links.link("exec-1", &"sess-a".to_string());
        links.link("exec-2", &"sess-b".to_string());
        assert_eq!(links.get("exec-1"), Some("sess-a".to_string()));
        assert_eq!(links.get("exec-2"), Some("sess-b".to_string()));

        links.unlink("exec-1");
        assert_eq!(links.get("exec-1"), None);
        assert_eq!(
            links.get("exec-2"),
            Some("sess-b".to_string()),
            "unlinking one execution must not touch a sibling's mapping"
        );
    }

    // -- route_run_event --

    fn temp_store() -> (tempfile::TempDir, SessionStoreRoot) {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let root = SessionStoreRoot::at(dir.path().join("agent-desk").join("v1"))
            .expect("init store root");
        (dir, root)
    }

    #[test]
    fn an_unlinked_repository_routes_to_nothing() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();
        let event = run_event(AirunRunState::Working, RunStep::Note { text: "a".into() });

        let routed = route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:01Z", &event);
        assert!(matches!(routed, RunEventRouted::NoLinkedSession));
    }

    #[test]
    fn a_linked_repository_persists_before_it_can_be_emitted() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();
        let session = session();
        store::write_session(&root, &session).unwrap();
        links.link("run-1", &session.header.session_id);

        let event = run_event(AirunRunState::Working, RunStep::Note { text: "hi".into() });
        let seq = links.next_sequence(&event.session_id);
        let routed = route_run_event(&root, &links, &locks, seq, "2026-01-01T00:00:01Z", &event);

        let RunEventRouted::Persisted { event: durable } = routed else {
            panic!("expected Persisted, got {routed:?}");
        };
        assert_eq!(durable.sequence, 1);

        // The write really happened -- re-reading from disk shows the message,
        // not just the in-memory outcome.
        let reread = store::read_session(&root, &session.header.session_id).unwrap();
        assert_eq!(reread.messages.len(), 1);
        assert_eq!(reread.messages[0].plain_content, "hi");
    }

    #[test]
    fn a_session_that_cannot_be_read_reports_unavailable() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();
        links.link("run-1", &"does-not-exist".to_string());

        let event = run_event(AirunRunState::Working, RunStep::Note { text: "a".into() });
        let routed = route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:01Z", &event);
        assert!(matches!(routed, RunEventRouted::SessionUnavailable));
    }

    #[test]
    fn a_duplicate_sequence_is_ignored_not_persisted() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();
        let session = session();
        store::write_session(&root, &session).unwrap();
        links.link("run-1", &session.header.session_id);

        let event = run_event(AirunRunState::Working, RunStep::Note { text: "a".into() });
        route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:01Z", &event);
        let before = store::read_session(&root, &session.header.session_id).unwrap();

        let routed = route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:02Z", &event);
        assert!(matches!(
            routed,
            RunEventRouted::Ignored(BridgeOutcome::DuplicateSequence { .. })
        ));

        let after = store::read_session(&root, &session.header.session_id).unwrap();
        assert_eq!(before, after, "an ignored event must not touch the file on disk");
    }

    #[test]
    fn an_event_for_a_superseded_execution_is_ignored_not_persisted() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();
        let session = session();
        store::write_session(&root, &session).unwrap();
        // Both executions are linked to the same session -- a retry/second
        // execution against the same repository still routes to the session
        // that owns it, it just becomes the new active lead once it arrives.
        links.link("run-1", &session.header.session_id);
        links.link("run-2", &session.header.session_id);

        let first = RunEventKind {
            repo_id: "repo-1".into(),
            session_id: "run-1".into(),
            state: AirunRunState::Working,
            summary: "a".into(),
            step: RunStep::Note { text: "a".into() },
        };
        route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:01Z", &first);

        let second = RunEventKind {
            repo_id: "repo-1".into(),
            session_id: "run-2".into(),
            state: AirunRunState::Working,
            summary: "b".into(),
            step: RunStep::Note { text: "b".into() },
        };
        route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:02Z", &second);

        let before = store::read_session(&root, &session.header.session_id).unwrap();
        let late = RunEventKind {
            repo_id: "repo-1".into(),
            session_id: "run-1".into(),
            state: AirunRunState::Working,
            summary: "late".into(),
            step: RunStep::Note { text: "late".into() },
        };
        let routed = route_run_event(&root, &links, &locks, 2, "2026-01-01T00:00:03Z", &late);
        assert!(matches!(
            routed,
            RunEventRouted::Ignored(BridgeOutcome::ExecutionSuperseded { .. })
        ));
        let after = store::read_session(&root, &session.header.session_id).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn sequential_events_on_the_same_execution_all_persist_in_order() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();
        let session = session();
        store::write_session(&root, &session).unwrap();
        links.link("run-1", &session.header.session_id);

        // Distinct step kinds (not three `Note`s -- see the coalescing tests
        // below for that case): each is its own row, so this still checks
        // that *non*-coalescing sequential events persist in order and keep
        // advancing `last_sequence`.
        let steps = vec![
            RunStep::Plan {
                text: "step 1".into(),
            },
            RunStep::Note {
                text: "step 2".into(),
            },
            RunStep::YouSaid {
                text: "step 3".into(),
            },
        ];
        for step in steps {
            let event = run_event(AirunRunState::Working, step);
            let seq = links.next_sequence(&event.session_id);
            let routed = route_run_event(&root, &links, &locks, seq, "2026-01-01T00:00:01Z", &event);
            assert!(matches!(routed, RunEventRouted::Persisted { .. }));
        }

        let reread = store::read_session(&root, &session.header.session_id).unwrap();
        assert_eq!(reread.messages.len(), 3);
        assert_eq!(reread.executions[0].last_sequence, 3);
    }

    // -- P0: execution-addressed routing (two sessions, one repository) --

    fn header_for(session_id: &str, repo_id: &str) -> AgentSessionHeader {
        AgentSessionHeader {
            session_id: session_id.into(),
            repo_id: repo_id.into(),
            source: SessionSource::Manual {
                repo_id: repo_id.into(),
            },
            ..header()
        }
    }

    /// THE regression test for "an event can reach the wrong chat": two
    /// sessions, each with its own live execution, share ONE repository --
    /// exactly the scenario the reset audit names (a helper's own session,
    /// concurrent with a lead's, or simply two chats open on the same repo).
    /// An event for execution A must land ONLY in session A, never in B, even
    /// though both `RunEventKind`s carry the same `repo_id`.
    #[test]
    fn an_event_for_execution_a_lands_in_session_a_only_when_two_sessions_share_a_repository() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();

        let session_a = AgentSession::new(header_for("sess-a", "repo-shared"));
        let session_b = AgentSession::new(header_for("sess-b", "repo-shared"));
        store::write_session(&root, &session_a).unwrap();
        store::write_session(&root, &session_b).unwrap();

        // Session A's lead execution and session B's own (different)
        // execution, both against the same repository.
        links.link("exec-a", &"sess-a".to_string());
        links.link("exec-b", &"sess-b".to_string());

        let event_for_a = RunEventKind {
            repo_id: "repo-shared".into(),
            session_id: "exec-a".into(),
            state: AirunRunState::Working,
            summary: "a".into(),
            step: RunStep::Note { text: "hello from A's execution".into() },
        };
        let routed = route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:01Z", &event_for_a);
        assert!(matches!(routed, RunEventRouted::Persisted { .. }), "got {routed:?}");

        let a_after = store::read_session(&root, "sess-a").unwrap();
        let b_after = store::read_session(&root, "sess-b").unwrap();
        assert_eq!(a_after.messages.len(), 1, "the event must land in session A");
        assert_eq!(a_after.messages[0].plain_content, "hello from A's execution");
        assert!(b_after.messages.is_empty(), "the event must NEVER land in session B, which shares the repository but not the execution");

        // Unlinking A's execution must leave B's own mapping intact -- the
        // other half of the audit's required outcome ("unlink only that
        // exact mapping").
        links.unlink("exec-a");
        assert_eq!(links.get("exec-a"), None);
        assert_eq!(
            links.get("exec-b"),
            Some("sess-b".to_string()),
            "unlinking A's execution must not disturb B's own mapping"
        );

        // And B's execution still routes correctly afterward.
        let event_for_b = RunEventKind {
            repo_id: "repo-shared".into(),
            session_id: "exec-b".into(),
            state: AirunRunState::Working,
            summary: "b".into(),
            step: RunStep::Note { text: "hello from B's execution".into() },
        };
        let routed_b = route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:02Z", &event_for_b);
        assert!(matches!(routed_b, RunEventRouted::Persisted { .. }), "got {routed_b:?}");
        let b_final = store::read_session(&root, "sess-b").unwrap();
        assert_eq!(b_final.messages.len(), 1);
        assert_eq!(b_final.messages[0].plain_content, "hello from B's execution");
    }

    /// A helper and its lead, in DIFFERENT sessions, sharing a repository
    /// (the same shape a "run a helper-style follow-up in a fresh chat on the
    /// same repo" scenario produces): each execution's events land only in
    /// its own session, with no cross-contamination in either direction.
    #[test]
    fn a_helper_and_its_lead_in_different_sessions_of_the_same_repo_do_not_cross_contaminate() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();

        let lead_session = AgentSession::new(header_for("sess-lead", "repo-shared"));
        let helper_session = AgentSession::new(header_for("sess-helper", "repo-shared"));
        store::write_session(&root, &lead_session).unwrap();
        store::write_session(&root, &helper_session).unwrap();

        links.link("exec-lead", &"sess-lead".to_string());
        links.link("exec-helper", &"sess-helper".to_string());

        let lead_event = RunEventKind {
            repo_id: "repo-shared".into(),
            session_id: "exec-lead".into(),
            state: AirunRunState::Working,
            summary: "lead".into(),
            step: RunStep::Note { text: "lead output".into() },
        };
        let helper_event = RunEventKind {
            repo_id: "repo-shared".into(),
            session_id: "exec-helper".into(),
            state: AirunRunState::Working,
            summary: "helper".into(),
            step: RunStep::Note { text: "helper output".into() },
        };

        // Interleaved, as a real concurrent lead+helper run would produce.
        route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:01Z", &helper_event);
        route_run_event(&root, &links, &locks, 1, "2026-01-01T00:00:02Z", &lead_event);

        let lead_after = store::read_session(&root, "sess-lead").unwrap();
        let helper_after = store::read_session(&root, "sess-helper").unwrap();
        assert_eq!(lead_after.messages.len(), 1);
        assert_eq!(lead_after.messages[0].plain_content, "lead output");
        assert_eq!(helper_after.messages.len(), 1);
        assert_eq!(helper_after.messages[0].plain_content, "helper output");
    }

    // -- Note coalescing (streamed-text chunks folding into one message) --

    #[test]
    fn consecutive_note_chunks_from_one_execution_coalesce_into_one_message() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");

        let first = run_event(AirunRunState::Working, RunStep::Note { text: "Hel".into() });
        let outcome1 = apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &first);
        let BridgeOutcome::Applied { session: after1, event: e1 } = outcome1 else {
            panic!("expected Applied");
        };
        session = after1;
        assert!(matches!(e1.kind, AgentSessionEventKind::MessageAppended { .. }));
        assert_eq!(session.messages.len(), 1);

        let second = run_event(AirunRunState::Working, RunStep::Note { text: "lo, ".into() });
        let outcome2 = apply_run_event(&mut session, &exec, 2, "2026-01-01T00:00:02Z", &second);
        let BridgeOutcome::Applied { session: after2, event: e2 } = outcome2 else {
            panic!("expected Applied");
        };
        session = after2;
        let AgentSessionEventKind::MessageUpdated { message: m2 } = e2.kind else {
            panic!("expected MessageUpdated for the second chunk");
        };
        assert_eq!(
            session.messages.len(),
            1,
            "a coalesced chunk must not add a second message"
        );
        assert_eq!(m2.message_id, session.messages[0].message_id);
        assert_eq!(m2.plain_content, "Hello, ");

        let third = run_event(AirunRunState::Working, RunStep::Note { text: "world".into() });
        let outcome3 = apply_run_event(&mut session, &exec, 3, "2026-01-01T00:00:03Z", &third);
        let BridgeOutcome::Applied { session: after3, event: e3 } = outcome3 else {
            panic!("expected Applied");
        };
        session = after3;
        let AgentSessionEventKind::MessageUpdated { message: m3 } = e3.kind else {
            panic!("expected MessageUpdated for the third chunk");
        };

        // ONE growing message, not three -- the whole point of coalescing.
        assert_eq!(session.messages.len(), 1);
        assert_eq!(m3.plain_content, "Hello, world");
        assert_eq!(session.messages[0].plain_content, "Hello, world");
        assert_eq!(
            session.messages[0].message_id, m2.message_id,
            "message id must stay stable across every coalesced chunk"
        );
        // Sequence numbering keeps advancing even though the row count does
        // not -- the coalesced message's own `sequence` tracks the latest
        // chunk, and the execution's `last_sequence` (dedup/gap tracking)
        // still moves forward per chunk.
        assert_eq!(session.messages[0].sequence, Some(3));
        assert_eq!(session.executions[0].last_sequence, 3);
    }

    #[test]
    fn a_non_note_step_between_chunks_ends_the_coalescing_run() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");

        let first = run_event(AirunRunState::Working, RunStep::Note { text: "part one".into() });
        let BridgeOutcome::Applied { session: after1, .. } =
            apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &first)
        else {
            panic!("expected Applied");
        };
        session = after1;

        // A non-Note step interrupts the run: e.g. a tool result summarized
        // as a Check.
        let check = run_event(
            AirunRunState::Working,
            RunStep::Check {
                name: "typecheck".into(),
                passed: true,
                detail: "ok".into(),
            },
        );
        let BridgeOutcome::Applied { session: after2, event: e2 } =
            apply_run_event(&mut session, &exec, 2, "2026-01-01T00:00:02Z", &check)
        else {
            panic!("expected Applied");
        };
        session = after2;
        assert!(matches!(e2.kind, AgentSessionEventKind::MessageAppended { .. }));
        assert_eq!(session.messages.len(), 2, "the Check step is its own message");

        // The next Note must start a NEW message, not fold into "part one"
        // across the Check.
        let second_note = run_event(AirunRunState::Working, RunStep::Note { text: "part two".into() });
        let BridgeOutcome::Applied { session: after3, event: e3 } =
            apply_run_event(&mut session, &exec, 3, "2026-01-01T00:00:03Z", &second_note)
        else {
            panic!("expected Applied");
        };
        session = after3;
        assert!(
            matches!(e3.kind, AgentSessionEventKind::MessageAppended { .. }),
            "a Note after a non-Note step must append, not update"
        );
        assert_eq!(session.messages.len(), 3);
        assert_eq!(session.messages[0].plain_content, "part one");
        assert_eq!(session.messages[2].plain_content, "part two");
        assert_ne!(session.messages[0].message_id, session.messages[2].message_id);
    }

    #[test]
    fn a_tool_call_produces_a_tool_kind_message_never_assistant_prose() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");
        let activity = run_event(
            AirunRunState::Working,
            RunStep::Activity {
                text: "Finding files matching **/tasks.md".into(),
            },
        );
        let outcome = apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &activity);
        let BridgeOutcome::Applied { session: applied, event: durable } = outcome else {
            panic!("expected Applied");
        };
        assert!(matches!(durable.kind, AgentSessionEventKind::MessageAppended { .. }));
        assert_eq!(
            applied.messages[0].kind,
            MessageKind::Tool,
            "a tool call must never be indistinguishable from the agent's own prose (MessageKind::Assistant)"
        );
    }

    #[test]
    fn a_tool_call_between_two_note_chunks_ends_the_coalescing_run() {
        let mut session = session();
        let exec = execution_id_for_run_session("run-1");

        let first = run_event(AirunRunState::Working, RunStep::Note { text: "I'm not sure what".into() });
        let BridgeOutcome::Applied { session: after1, .. } =
            apply_run_event(&mut session, &exec, 1, "2026-01-01T00:00:01Z", &first)
        else {
            panic!("expected Applied");
        };
        session = after1;

        // A tool call fires mid-reply, exactly like the screenshot's
        // "Finding files matching **/tasks.md" wedged into the wall of text.
        let activity = run_event(
            AirunRunState::Working,
            RunStep::Activity {
                text: "Finding files matching **/tasks.md".into(),
            },
        );
        let BridgeOutcome::Applied { session: after2, event: e2 } =
            apply_run_event(&mut session, &exec, 2, "2026-01-01T00:00:02Z", &activity)
        else {
            panic!("expected Applied");
        };
        session = after2;
        assert!(matches!(e2.kind, AgentSessionEventKind::MessageAppended { .. }));
        assert_eq!(session.messages.len(), 2, "the tool call is its own message");

        // The next Note must start a NEW message, not fold into the first
        // chunk across the tool call.
        let second = run_event(AirunRunState::Working, RunStep::Note { text: "\"that\" refers to.".into() });
        let BridgeOutcome::Applied { session: after3, event: e3 } =
            apply_run_event(&mut session, &exec, 3, "2026-01-01T00:00:03Z", &second)
        else {
            panic!("expected Applied");
        };
        session = after3;
        assert!(
            matches!(e3.kind, AgentSessionEventKind::MessageAppended { .. }),
            "a Note after a tool call must append, not update"
        );
        assert_eq!(session.messages.len(), 3);
        assert_eq!(session.messages[0].plain_content, "I'm not sure what");
        assert_eq!(session.messages[1].kind, MessageKind::Tool);
        assert_eq!(session.messages[2].plain_content, "\"that\" refers to.");
        assert_ne!(session.messages[0].message_id, session.messages[2].message_id);
    }

    #[test]
    fn a_note_chunk_for_a_different_execution_does_not_coalesce_into_the_previous_one() {
        let mut session = session();
        let exec1 = execution_id_for_run_session("run-1");
        let exec2 = execution_id_for_run_session("run-2");

        let first = run_event(AirunRunState::Working, RunStep::Note { text: "from exec1".into() });
        let BridgeOutcome::Applied { session: after1, .. } =
            apply_run_event(&mut session, &exec1, 1, "2026-01-01T00:00:01Z", &first)
        else {
            panic!("expected Applied");
        };
        session = after1;

        // A helper/retry execution's own first Note must not fold into the
        // lead's last message just because both happen to be Notes.
        let second = run_event(AirunRunState::Working, RunStep::Note { text: "from exec2".into() });
        let BridgeOutcome::Applied { event: e2, .. } =
            apply_run_event(&mut session, &exec2, 1, "2026-01-01T00:00:02Z", &second)
        else {
            panic!("expected Applied");
        };
        assert!(matches!(e2.kind, AgentSessionEventKind::MessageAppended { .. }));
    }

    #[test]
    fn a_coalesced_update_persists_by_replacing_not_appending() {
        let (_dir, root) = temp_store();
        let links = RunSessionLinks::new();
        let locks = SessionLocks::new();
        let session = session();
        store::write_session(&root, &session).unwrap();
        links.link("run-1", &session.header.session_id);

        for text in ["chunk one ", "chunk two ", "chunk three"] {
            let event = run_event(AirunRunState::Working, RunStep::Note { text: text.into() });
            let seq = links.next_sequence(&event.session_id);
            let routed = route_run_event(&root, &links, &locks, seq, "2026-01-01T00:00:01Z", &event);
            assert!(matches!(routed, RunEventRouted::Persisted { .. }));
        }

        let reread = store::read_session(&root, &session.header.session_id).unwrap();
        assert_eq!(
            reread.messages.len(),
            1,
            "coalescing must persist as one message on disk, not three"
        );
        assert_eq!(reread.messages[0].plain_content, "chunk one chunk two chunk three");
        assert_eq!(reread.executions[0].last_sequence, 3);
    }

    // -- R6.3/R6.7: a helper's own events must keep landing while the lead
    // (or a sibling helper) is the session's `active_execution_id`. --

    fn helper_record(execution_id: &str, lead_id: &str) -> ExecutionRecord {
        ExecutionRecord::minimal(
            execution_id.into(),
            "sess-1".into(),
            Some(lead_id.into()),
            SessionState::Ready,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        )
    }

    /// Before the fix, a helper record that already existed (created by
    /// `commit_started_graph_if_still_proposed` before it is launched) but
    /// was not `active_execution_id` would hit the "replaced execution"
    /// branch and be reported `ExecutionSuperseded` -- silently dropping
    /// every one of its events forever, since `active_execution_id` never
    /// names a helper at all.
    #[test]
    fn a_helpers_own_events_are_applied_even_though_the_lead_is_the_active_execution() {
        let mut session = session();
        session.header.active_execution_id = Some("lead-1".into());
        session.executions.push(ExecutionRecord::minimal(
            "lead-1".into(),
            "sess-1".into(),
            None,
            SessionState::Working,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        ));
        session.executions.push(helper_record("helper-1", "lead-1"));

        let event = run_event(AirunRunState::Working, RunStep::Note { text: "helper says hi".into() });
        let outcome = apply_run_event(&mut session, &"helper-1".to_string(), 1, "2026-01-01T00:00:01Z", &event);

        match outcome {
            BridgeOutcome::Applied { session: applied, .. } => {
                let helper = applied
                    .executions
                    .iter()
                    .find(|e| e.execution_id == "helper-1")
                    .unwrap();
                assert_eq!(helper.last_sequence, 1, "the helper's own event must be recorded, not dropped");
                assert_eq!(helper.state, SessionState::Working);
            }
            other => panic!("expected Applied, got {other:?}"),
        }
    }

    /// A helper reaching a terminal state (Finished) must never overwrite
    /// `session.header.state`/`active_execution_id` away from the lead --
    /// the header is the single session-wide status the sidebar/title bar
    /// show, and it must always reflect the LEAD, not whichever execution's
    /// event happened to land last.
    #[test]
    fn a_finished_helper_does_not_hijack_the_session_header_away_from_the_working_lead() {
        let mut session = session();
        session.header.active_execution_id = Some("lead-1".into());
        session.header.state = SessionState::Working;
        session.executions.push(ExecutionRecord::minimal(
            "lead-1".into(),
            "sess-1".into(),
            None,
            SessionState::Working,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        ));
        session.executions.push(helper_record("helper-1", "lead-1"));

        let event = run_event(
            AirunRunState::Finished,
            RunStep::Ended { state: AirunRunState::Finished, detail: "done".into() },
        );
        let outcome = apply_run_event(&mut session, &"helper-1".to_string(), 1, "2026-01-01T00:00:01Z", &event);

        match outcome {
            BridgeOutcome::Applied { session: applied, .. } => {
                assert_eq!(
                    applied.header.state,
                    SessionState::Working,
                    "the session header must keep tracking the lead, not the helper that just finished"
                );
                assert_eq!(applied.header.active_execution_id, Some("lead-1".to_string()));
                let helper = applied
                    .executions
                    .iter()
                    .find(|e| e.execution_id == "helper-1")
                    .unwrap();
                assert_eq!(helper.state, SessionState::Finished, "the helper's OWN record still reflects its finish");
            }
            other => panic!("expected Applied, got {other:?}"),
        }
    }

    /// The LEAD's own events still drive the session header exactly as
    /// before this change -- the guard only exempts helpers.
    #[test]
    fn the_leads_own_event_still_drives_the_session_header() {
        let mut session = session();
        session.header.active_execution_id = Some("lead-1".into());
        session.executions.push(ExecutionRecord::minimal(
            "lead-1".into(),
            "sess-1".into(),
            None,
            SessionState::Working,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        ));

        let event = run_event(
            AirunRunState::Finished,
            RunStep::Ended { state: AirunRunState::Finished, detail: "done".into() },
        );
        let outcome = apply_run_event(&mut session, &"lead-1".to_string(), 1, "2026-01-01T00:00:01Z", &event);

        match outcome {
            BridgeOutcome::Applied { session: applied, .. } => {
                assert_eq!(applied.header.state, SessionState::Finished);
            }
            other => panic!("expected Applied, got {other:?}"),
        }
    }
}
