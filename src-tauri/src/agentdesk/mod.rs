//! Durable Agent Desk sessions: the domain model and (eventually) their
//! on-disk store, separate from `airun` which remains the execution engine.
//!
//! See `docs/agent-desk/architecture.md` sections 1-4 and
//! `openspec/changes/agent-desk-session-foundation/` for the contract this
//! module implements incrementally.

pub mod bridge;
pub mod events;
pub mod locks;
pub mod model;
pub mod store;

#[allow(unused_imports)]
pub use bridge::{
    apply_run_event, execution_id_for_run_session, route_run_event, BridgeOutcome,
    RunEventRouted, RunSessionLinks, AGENT_SESSION_EVENT,
};
#[allow(unused_imports)]
pub use locks::SessionLocks;
#[allow(unused_imports)]
pub use events::{AgentSessionEvent, AgentSessionEventKind};
#[allow(unused_imports)]
pub use model::{
    AgentSession, AgentSessionHeader, ContextAttachment, ConversationSegment, ExecutionId,
    ExecutionRecord, ImportProvenance, MessageId, MessageKind, MessageRole, MessageTarget,
    SegmentId, SessionId, SessionIntent, SessionLoadError, SessionMessage, SessionSource,
    SessionState, SourceSnapshot, CURRENT_SCHEMA_VERSION,
};
#[allow(unused_imports)]
pub use store::{
    find_duplicate_session_ids, list_sessions, load_or_rebuild_index, read_session, sort_headers,
    write_index, write_session, DuplicateSessionId, IndexLoadResult, SessionFileDiagnostic,
    SessionListFilter, SessionListPage, SessionStoreRoot, StoreInitError, WriteError,
};
