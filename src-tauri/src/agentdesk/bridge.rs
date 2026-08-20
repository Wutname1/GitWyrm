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
use std::sync::Mutex;

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

/// Which durable [`SessionId`] a repository's live `airun` runs feed into,
/// when one has been linked.
///
/// A run event only carries `repo_id` and its own `airun` session ID -- it
/// has no idea a durable Agent Desk session exists at all. Nothing populates
/// this registry yet (that is `agent_session_start_execution`, a later
/// task/command not in this change's scope); until something does,
/// `bridge_run_event` below finds no link and is a no-op, which is exactly
/// what keeps `ai-run-event` behavior unchanged per task 4.5 -- the durable
/// path exists and is fully tested, but produces nothing until a caller
/// opts a repository in.
#[derive(Default)]
pub struct RunSessionLinks {
    inner: Mutex<HashMap<String, SessionId>>,
    /// Per-execution sequence counters, keyed by the `airun` session ID
    /// (which doubles as the durable `execution_id`, see
    /// [`execution_id_for_run_session`]). A fresh counter per execution is
    /// what makes sequence numbers monotonic *per execution* rather than per
    /// repository -- two executions in the same repository (a retry after a
    /// stop) each start their own transcript at sequence 1.
    sequences: Mutex<HashMap<String, u32>>,
}

impl RunSessionLinks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Links `repo_id`'s future run events to durable session `session_id`.
    pub fn link(&self, repo_id: &str, session_id: &SessionId) {
        self.inner
            .lock()
            .unwrap()
            .insert(repo_id.to_string(), session_id.clone());
    }

    pub fn get(&self, repo_id: &str) -> Option<SessionId> {
        self.inner.lock().unwrap().get(repo_id).cloned()
    }

    pub fn unlink(&self, repo_id: &str) {
        self.inner.lock().unwrap().remove(repo_id);
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
    // "Event for a replaced execution": this execution has already appeared
    // in the session (it has a record) but is no longer the active one.
    // A never-before-seen execution ID is always allowed to start and become
    // active -- that is how a session's second, third, ... execution (a
    // retry, a follow-up) is meant to begin. It is only an event arriving
    // *after* its execution was superseded by another that is stale.
    let already_known = session
        .executions
        .iter()
        .any(|e| &e.execution_id == execution_id);
    if already_known {
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
    record.last_sequence = sequence;
    record.state = map_run_state(event.state);
    if !matches!(event.state, AirunRunState::Finished | AirunRunState::Stopped | AirunRunState::Failed) {
        record.ended_at = None;
    } else {
        record.ended_at = Some(occurred_at.to_string());
    }

    let kind = map_run_step(session, execution_id, sequence, occurred_at, event);

    if let AgentSessionEventKind::MessageAppended { message } = &kind {
        session.messages.push(message.clone());
    }
    // The session's own state always tracks its active execution's state,
    // not only on the `Ended` step that produces an explicit `StateChanged`
    // event -- a run entering `Preparing`/`Working`/`NeedsYou` moves the
    // session there too, it just does so alongside a transcript message
    // instead of as the event's sole content.
    session.header.state = map_run_state(event.state);
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
/// `links.get(&event.repo_id)` is called *before* the session lock is taken
/// and `RunSessionLinks` is never touched again inside this function, so
/// there is exactly one lock-acquisition order in play -- see
/// `agentdesk::locks`'s module doc for the full argument.
pub fn route_run_event(
    root: &SessionStoreRoot,
    links: &RunSessionLinks,
    locks: &SessionLocks,
    sequence: u32,
    occurred_at: &str,
    event: &RunEventKind,
) -> RunEventRouted {
    let Some(session_id) = links.get(&event.repo_id) else {
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
    // `RunStep::Ended` still fully determines `event.state` above, so no
    // field is lost, it just is not restated as a message.
    if let RunStep::Ended { state, .. } = &event.step {
        return AgentSessionEventKind::StateChanged {
            state: map_run_state(*state),
        };
    }

    let message = build_message(session, execution_id, sequence, occurred_at, event);
    AgentSessionEventKind::MessageAppended { message }
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
        RunStep::Adapted { .. } => MessageKind::Assistant,
        // Handled before this is reached (see `map_run_step`); kept here so
        // the match stays exhaustive against every `RunStep` variant rather
        // than relying on the caller's short-circuit, which would silently
        // stop being exhaustive if `map_run_step`'s early return were ever
        // removed.
        RunStep::Ended { .. } => MessageKind::System,
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
        assert_eq!(links.get("repo-1"), None);
        links.link("repo-1", &"sess-1".to_string());
        assert_eq!(links.get("repo-1"), Some("sess-1".to_string()));
        links.unlink("repo-1");
        assert_eq!(links.get("repo-1"), None);
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
        links.link("repo-1", &session.header.session_id);

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
        links.link("repo-1", &"does-not-exist".to_string());

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
        links.link("repo-1", &session.header.session_id);

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
        links.link("repo-1", &session.header.session_id);

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
        links.link("repo-1", &session.header.session_id);

        for i in 1..=3u32 {
            let event = run_event(
                AirunRunState::Working,
                RunStep::Note {
                    text: format!("step {i}"),
                },
            );
            let seq = links.next_sequence(&event.session_id);
            let routed = route_run_event(&root, &links, &locks, seq, "2026-01-01T00:00:01Z", &event);
            assert!(matches!(routed, RunEventRouted::Persisted { .. }));
        }

        let reread = store::read_session(&root, &session.header.session_id).unwrap();
        assert_eq!(reread.messages.len(), 3);
        assert_eq!(reread.executions[0].last_sequence, 3);
    }
}
