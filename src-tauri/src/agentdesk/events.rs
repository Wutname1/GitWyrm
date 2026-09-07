//! The durable session event envelope.
//!
//! This module defines the *shape* of the event that flows from an execution
//! into a session (architecture.md section 4). The bridge that produces these
//! from `airun::RunEventKind`/`RunStep` lives in `super::bridge`; the code
//! that persists then emits them over Tauri lives in
//! `src-tauri/src/commands/airun.rs`'s `emit()`. Kept separate so this shape
//! can be relied on by the domain model and its tests independently of that
//! bridge.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::model::{ExecutionId, SessionId, SessionMessage, SessionState};

/// One event on the way from an execution into a durable session.
///
/// `sequence` is monotonic per `execution_id`. It is what makes a duplicate
/// harmless (ignore it) and a gap visible (persist and flag it) instead of
/// silently reordering the transcript.
///
/// `u32`, matching [`super::model::SessionMessage::sequence`] and
/// [`super::model::ExecutionRecord::last_sequence`] -- specta's TypeScript
/// export refuses `u64`/`i64` outright (no built-in JS integer can represent
/// the full range without precision loss), so every field reachable from an
/// exported command or type has to stay within `u32` however unlikely
/// billions of events in one execution actually are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionEvent {
    pub session_id: SessionId,
    pub execution_id: Option<ExecutionId>,
    pub sequence: u32,
    /// RFC 3339 UTC timestamp.
    pub occurred_at: String,
    pub kind: AgentSessionEventKind,
}

/// What happened. Deliberately narrow: everything the UI needs to update
/// either the transcript or the session header, nothing it has to infer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentSessionEventKind {
    /// A new message was appended to the transcript.
    #[serde(rename_all = "camelCase")]
    MessageAppended { message: SessionMessage },
    /// An existing message's content was replaced in place, identified by
    /// `message.message_id`. This is how consecutive streamed-text steps
    /// within one execution ([`super::bridge::apply_run_event`]'s
    /// coalescing) grow a single transcript row instead of appending a new
    /// one per chunk -- the id, `segment_id`, `role`, `kind`, and `targets`
    /// never change across an update, only `plain_content`,
    /// `rendered_content`, `timestamp`, and `sequence` do. A listener that
    /// only knows `MessageAppended` (e.g. one written before this variant
    /// existed) would need to special-case this by id; see
    /// `agentSessionStore.ts`'s handling for the frontend's version of that
    /// same rule.
    #[serde(rename_all = "camelCase")]
    MessageUpdated { message: SessionMessage },
    /// The session (or one of its executions) changed state.
    #[serde(rename_all = "camelCase")]
    StateChanged { state: SessionState },
    /// The event's execution is no longer the session's active one, so it was
    /// recorded (if at all) without becoming visible. Sent so a listener that
    /// already rendered something optimistically can reconcile.
    #[serde(rename_all = "camelCase")]
    ExecutionSuperseded { execution_id: ExecutionId },
    /// This event could not be written to the session file, so the durable
    /// record is missing it.
    ///
    /// Carries no message content, deliberately. "Persist an event before
    /// emitting it to the UI" exists so nothing appears on screen that would
    /// vanish on reopening -- this notice describes the store rather than
    /// adding to the transcript, so it does not have that problem and does not
    /// need to survive a reopen.
    ///
    /// It exists because a listener cannot otherwise tell two very different
    /// situations apart. Both show up as a skipped sequence number. In one the
    /// window missed an event the file has, and reopening the chat loads it.
    /// In the other -- this one -- the file is what is missing it, and the
    /// only copy is the one already on screen. The advice for the first is
    /// actively wrong for the second.
    NotSaved,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_serializes_with_camel_case_keys() {
        let event = AgentSessionEvent {
            session_id: "sess-1".into(),
            execution_id: Some("exec-1".into()),
            sequence: 3,
            occurred_at: "2026-01-01T00:00:00Z".into(),
            kind: AgentSessionEventKind::StateChanged {
                state: SessionState::Working,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"sessionId\""));
        assert!(json.contains("\"executionId\""));
        assert!(json.contains("\"occurredAt\""));
        assert!(json.contains("\"stateChanged\""));
    }

    #[test]
    fn execution_superseded_fields_are_camel_case() {
        let kind = AgentSessionEventKind::ExecutionSuperseded {
            execution_id: "exec-1".into(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("\"executionId\""), "got: {json}");
        assert!(!json.contains("execution_id"), "got: {json}");

        let back: AgentSessionEventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind, "round trip changed the value");
    }
}
