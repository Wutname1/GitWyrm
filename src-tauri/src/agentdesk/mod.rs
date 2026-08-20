//! Durable Agent Desk sessions: the domain model and (eventually) their
//! on-disk store, separate from `airun` which remains the execution engine.
//!
//! See `docs/agent-desk/architecture.md` sections 1-4 and
//! `openspec/changes/agent-desk-session-foundation/` for the contract this
//! module implements incrementally.

pub mod adapters;
pub mod bridge;
pub mod events;
pub mod graph;
pub mod import_store;
pub mod locks;
pub mod model;
pub mod openspec_context;
pub mod policy;
pub mod reconcile;
pub mod result;
pub mod session_recovery;
pub mod store;

#[allow(unused_imports)]
pub use bridge::{
    apply_run_event, execution_id_for_run_session, route_run_event, BridgeOutcome,
    RunEventRouted, RunSessionLinks, AGENT_SESSION_EVENT,
};
#[allow(unused_imports)]
pub use locks::SessionLocks;
#[allow(unused_imports)]
pub use policy::{
    check_tool_capability, for_intent, ExecutionMode, ExecutionTeam, IntentPolicy, ToolCapability,
    ToolRefusal, WorktreePolicy,
};
#[allow(unused_imports)]
pub use events::{AgentSessionEvent, AgentSessionEventKind};
#[allow(unused_imports)]
pub use graph::{
    detect_conflict, project_graph, schedule, validate_graph, BlockedNode, CompletionCondition,
    GraphNodeView, GraphValidationError, HelperRole, IntegrationConflict, IntegrationState,
    JobBudget, ProposedGraph, ProposedHelperJob, ScheduleDecision, MAX_CONCURRENT_HELPERS,
};
#[allow(unused_imports)]
pub use model::{
    AgentSession, AgentSessionHeader, ContextAttachment, ConversationSegment, ExecutionId,
    ExecutionRecord, ImportProvenance, MessageId, MessageKind, MessageRole, MessageTarget,
    SegmentId, SessionId, SessionIntent, SessionLoadError, SessionMessage, SessionSource,
    SessionState, SourceSnapshot, CURRENT_SCHEMA_VERSION,
};
#[allow(unused_imports)]
pub use openspec_context::{
    context_for_change, context_for_task, is_draft_stale, locate_target_task,
    resolve_change_status, validate_draft_acyclic, GraphDraftValidation, OpenSpecChangeStatus,
    OpenSpecNodeRef, OpenSpecSourceContext, OpenSpecSourceStatus, ProposedGraphDraft,
    ProposedGraphNode, TargetTaskContext,
};
#[allow(unused_imports)]
pub use result::{
    find_result, read_results, upsert_result, write_results, CheckRunOutcome,
    ResultChangedPath, ResultCheckOutcome, ResultCommitRef, ResultOutcomeKind, ResultRecord,
    ResultState,
};
#[allow(unused_imports)]
pub use session_recovery::{
    is_live_process_state, reconcile_executions, reconcile_header, HeaderReconciliation,
    INTERRUPTED_REASON,
};
#[allow(unused_imports)]
pub use store::{
    find_duplicate_session_ids, list_sessions, list_sessions_reconciled, load_or_rebuild_index,
    read_session, sort_headers, write_index, write_session, DuplicateSessionId, IndexLoadResult,
    SessionFileDiagnostic, SessionListFilter, SessionListPage, SessionStoreRoot, StoreInitError,
    WriteError,
};
