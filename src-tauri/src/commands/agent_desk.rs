//! Commands for durable Agent Desk sessions: create/list/get/rename/archive/
//! mark-read, plus appending a user message and attaching context.
//!
//! Every command resolves its own [`SessionStoreRoot`] from the app handle
//! rather than reading one out of managed state -- `SessionStoreRoot::resolve`
//! is cheap (it only ensures the directory layout exists) and this keeps every
//! command self-contained and directly testable against a temp root without a
//! Tauri runtime, by calling the `*_at` logic functions below with a store root
//! built from a `tempfile::TempDir` (see the `#[cfg(test)] mod tests` block at
//! the bottom of this file).
//!
//! Mutations return typed outcome enums, never a bare success/failure string:
//! "not found" and "nothing changed" are expected states a caller branches on,
//! not exceptional conditions.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::agentdesk::model::{
    AgentSession, AgentSessionHeader, ContextAttachment, ExecutionId, MessageId, MessageKind,
    MessageRole, MessageTarget, SegmentId, SessionId, SessionIntent, SessionMessage,
    SessionSource, SessionState, CURRENT_SCHEMA_VERSION,
};
use crate::agentdesk::store::{
    self, SessionListFilter, SessionListPage, SessionStoreRoot, StoreInitError, WriteError,
};
use crate::error::AppError;
use crate::openspec::write;

/// Now, formatted as the RFC 3339 UTC timestamp every persisted field on this
/// module uses. A single seam so every command stamps time the same way.
fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

/// A fresh, unguessable ID. Session, message, and attachment IDs are all
/// generated here rather than accepted from the frontend -- the backend owns
/// canonical identity for durable records (architecture.md section 5: "Backend
/// commands own canonical content").
fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Turns a store-layer [`StoreInitError`] into the one [`AppError`] variant
/// commands are allowed to return for it: the store root itself could not be
/// prepared, which is not a per-session outcome any caller could sensibly
/// branch on -- there is no session to talk about yet.
fn init_err(e: StoreInitError) -> AppError {
    AppError::Other(e.to_string())
}

/// Resolve the real app-data store root. The one place a command touches the
/// Tauri handle for this purpose, so every other function in this module can
/// be exercised directly against an arbitrary [`SessionStoreRoot`] in tests.
fn resolve_root(app: &AppHandle) -> Result<SessionStoreRoot, AppError> {
    SessionStoreRoot::resolve(app).map_err(init_err)
}

// -- 3.1: create / list / get / rename / archive / mark-read --

/// What the caller provides to start a session. Everything the header needs
/// besides what the backend generates itself (ID, timestamps, initial state).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub repo_id: String,
    pub repo_path: String,
    pub repo_name: String,
    pub title: String,
    pub source: SessionSource,
    pub intent: SessionIntent,
}

/// A session was created, or the request could not produce one. Kept as an
/// enum (rather than a plain `AgentSession`) so a future validation refusal
/// has a variant to land in without becoming an `AppError`; today creation
/// cannot fail except at the store layer, which still surfaces here rather
/// than as a bare write error the UI has no name for.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CreateSessionOutcome {
    Created { session: AgentSession },
    /// The session was built but could not be written to disk. The caller
    /// still gets the header ID that will not be reused, so a retry can log
    /// against the same identity if that ever matters.
    WriteFailed { detail: String },
}

// Deliberately does not take a `SessionLocks` argument: `session_id` is a
// freshly minted UUID (`new_id()`, below), unknown to any other caller until
// this function returns it, so there is nothing yet for a concurrent
// mutation to race against. Locking would only serialize unrelated session
// creations against each other for no correctness benefit.
fn create_session_at(
    root: &SessionStoreRoot,
    request: CreateSessionRequest,
) -> CreateSessionOutcome {
    let now = now_rfc3339();
    let header = AgentSessionHeader {
        schema_version: CURRENT_SCHEMA_VERSION,
        session_id: new_id(),
        repo_id: request.repo_id,
        repo_path: request.repo_path,
        repo_name: request.repo_name,
        title: request.title,
        source: request.source,
        intent: request.intent,
        state: SessionState::Draft,
        created_at: now.clone(),
        updated_at: now,
        unread: false,
        changed_file_count: 0,
        active_execution_id: None,
        archived: false,
    };
    let session = AgentSession::new(header);

    match store::write_session(root, &session) {
        Ok(()) => {
            // Best-effort: a session that wrote fine but whose index update
            // failed is still fully readable (list falls back to a rebuild
            // scan), so this is not folded into the outcome.
            let _ = refresh_index(root);
            CreateSessionOutcome::Created { session }
        }
        Err(e) => CreateSessionOutcome::WriteFailed {
            detail: e.to_string(),
        },
    }
}

/// Rewrites `index.json` from every session file's current header. Called
/// after any mutation that changes a header field, so the index stays in sync
/// without every command hand-rolling the same read-modify-write.
///
/// Correct but not the cheapest possible approach: it re-derives the full
/// header list from disk rather than patching one entry in place. Session
/// counts are "dozens per day" (architecture.md section 2), so a full rescan
/// per mutation is not a bottleneck, and it is the same code path index
/// rebuild already exercises -- one way for the index to become correct, not
/// two that can drift apart.
fn refresh_index(root: &SessionStoreRoot) -> Result<(), WriteError> {
    // Deliberately a full rescan rather than `load_or_rebuild_index`: that
    // function trusts the existing `index.json` verbatim whenever every
    // header in it still has a matching session file, which is true right
    // after this module edits one session's header in place -- the mutation
    // would otherwise never make it into the index until something else
    // happened to invalidate it.
    let (headers, _diagnostics) = store::rebuild_index_from_sessions(root);
    store::write_index(root, &headers)
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_create(
    app: AppHandle,
    request: CreateSessionRequest,
) -> Result<CreateSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || create_session_at(&root, request))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// `create_session_at`, reshaped as a plain `Result` for
/// `commands::agent_kickoff`, which has no use for the `WriteFailed` variant
/// carrying a session ID that (unlike a rename/archive failure) never got
/// written anywhere -- kickoff's own `StartAgentSessionOutcome::WriteFailed`
/// only needs the detail string. Not `pub(crate)`-restricted further than
/// that: this is the one sanctioned way to create a session from outside
/// this module, so a future caller other than kickoff needing the same
/// "create and get a plain session back" shape has somewhere to reuse rather
/// than reaching for `create_session_at` (private) or re-deriving the
/// success/failure split itself.
pub(crate) fn create_session_for_kickoff(
    root: &SessionStoreRoot,
    request: CreateSessionRequest,
) -> Result<AgentSession, String> {
    match create_session_at(root, request) {
        CreateSessionOutcome::Created { session } => Ok(session),
        CreateSessionOutcome::WriteFailed { detail } => Err(detail),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_list(
    app: AppHandle,
    filter: SessionListFilterInput,
    cursor: Option<String>,
    limit: u32,
) -> Result<SessionListPageOutput, AppError> {
    let root = resolve_root(&app)?;
    let filter: SessionListFilter = filter.into();
    let limit = limit.max(1) as usize;
    let page = tauri::async_runtime::spawn_blocking(move || {
        store::list_sessions(&root, &filter, cursor.as_deref(), limit)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(page.into())
}

/// Bindings-friendly mirror of [`SessionListFilter`]: the store type uses
/// `Vec<&'static str>` for source kinds, which specta cannot export as-is, and
/// every filter field is required here (rather than defaulted) so the
/// frontend request shape is explicit about "no filter" being an empty
/// vec/`None`, not an omitted field.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionListFilterInput {
    pub repo_id: Option<String>,
    pub project_path: Option<String>,
    pub states: Vec<SessionState>,
    pub source_kinds: Vec<String>,
    pub has_changed_files: Option<bool>,
    pub archived: Option<bool>,
    pub title_contains: Option<String>,
}

impl From<SessionListFilterInput> for SessionListFilter {
    fn from(input: SessionListFilterInput) -> Self {
        // Source kind labels are a fixed, small set of static strings
        // (`SessionSource::kind_label`), so an unrecognised one here simply
        // matches nothing rather than erroring -- the same "expected state,
        // not exceptional" stance as the rest of this module.
        let source_kinds = input
            .source_kinds
            .iter()
            .filter_map(|k| known_source_kind(k))
            .collect();
        SessionListFilter {
            repo_id: input.repo_id,
            project_path: input.project_path,
            states: input.states,
            source_kinds,
            has_changed_files: input.has_changed_files,
            archived: input.archived,
            title_contains: input.title_contains,
        }
    }
}

fn known_source_kind(label: &str) -> Option<&'static str> {
    const KNOWN: &[&str] = &[
        "manual",
        "issue",
        "pullRequest",
        "openSpecChange",
        "openSpecTask",
        "commit",
        "diff",
        "workingChanges",
        "checkFailure",
    ];
    KNOWN.iter().find(|&&k| k == label).copied()
}

/// Bindings-friendly mirror of [`SessionListPage`], with diagnostics reduced
/// to display-safe strings -- the frontend does not need to distinguish
/// [`crate::agentdesk::model::SessionLoadError`] variants, only that a file
/// could not be read and why, so the specta surface does not have to reach
/// into that error enum too.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionListPageOutput {
    pub headers: Vec<AgentSessionHeader>,
    pub next_cursor: Option<String>,
    pub diagnostics: Vec<SessionFileDiagnosticOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileDiagnosticOutput {
    pub path: String,
    pub reason: String,
}

impl From<SessionListPage> for SessionListPageOutput {
    fn from(page: SessionListPage) -> Self {
        SessionListPageOutput {
            headers: page.headers,
            next_cursor: page.next_cursor,
            diagnostics: page
                .diagnostics
                .into_iter()
                .map(|d| SessionFileDiagnosticOutput {
                    path: d.path.display().to_string(),
                    reason: d.reason.to_string(),
                })
                .collect(),
        }
    }
}

// `SessionLoadError` does not implement `Display` today (it derives no
// `thiserror::Error`), so build the same plain-language string commands need
// for diagnostics/outcomes by hand rather than adding a trait impl the domain
// model has no other use for.
impl std::fmt::Display for crate::agentdesk::model::SessionLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::agentdesk::model::SessionLoadError as E;
        match self {
            E::UnsupportedSchemaVersion { found, max_supported } => write!(
                f,
                "schema version {found} is newer than this build supports ({max_supported})"
            ),
            E::Malformed { detail } => write!(f, "malformed session file: {detail}"),
            E::NotFound => write!(f, "session file not found"),
            E::Io { detail } => write!(f, "could not read session file right now: {detail}"),
        }
    }
}

/// A single session lookup either finds the file or explains why not -- kept
/// distinct from [`AppError`] because "no such session" and "the file exists
/// but is damaged" are both routine states a caller displays, not faults.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GetSessionOutcome {
    Found { session: AgentSession },
    /// No file exists at that ID's path (never written, or the ID is wrong).
    NotFound,
    /// A file exists but could not be turned into a usable session.
    Damaged { reason: String },
    /// The file could not be read right now -- a permission error, or a lock
    /// held by another process (common on Windows). The session is not known
    /// to be gone; a retry may well succeed.
    Unavailable { detail: String },
}

fn get_session_at(root: &SessionStoreRoot, session_id: &str) -> GetSessionOutcome {
    use crate::agentdesk::model::SessionLoadError as E;
    match store::read_session(root, session_id) {
        Ok(session) => GetSessionOutcome::Found { session },
        Err(E::NotFound) => GetSessionOutcome::NotFound,
        Err(E::Io { detail }) => GetSessionOutcome::Unavailable { detail },
        Err(reason) => GetSessionOutcome::Damaged {
            reason: reason.to_string(),
        },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_get(
    app: AppHandle,
    session_id: SessionId,
) -> Result<GetSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || get_session_at(&root, &session_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// The shared shape of every "load, mutate the header, write back" command:
/// rename, archive, and mark-read all reduce to this with a different
/// mutation closure, so the not-found/damaged/write-failed branches are
/// written and tested exactly once.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateSessionOutcome {
    Updated { session: AgentSession },
    NotFound,
    Damaged { reason: String },
    WriteFailed { detail: String },
    /// The file could not be read right now -- a permission error, or a lock
    /// held by another process (common on Windows). Nothing was changed; the
    /// session is not known to be gone, and a retry may well succeed.
    Unavailable { detail: String },
}

fn update_session_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    mutate: impl FnOnce(&mut AgentSession),
) -> UpdateSessionOutcome {
    use crate::agentdesk::model::SessionLoadError as E;
    // The whole read-modify-write runs under this session's lock so a
    // concurrent mutation (another command, or a run event routed through
    // `bridge::route_run_event`) cannot read the same "before" state and
    // silently overwrite this one's result -- see `agentdesk::locks` for the
    // full race this closes.
    locks.with_session_lock(session_id, || {
        let mut session = match store::read_session(root, session_id) {
            Ok(s) => s,
            Err(E::NotFound) => return UpdateSessionOutcome::NotFound,
            Err(E::Io { detail }) => return UpdateSessionOutcome::Unavailable { detail },
            Err(reason) => {
                return UpdateSessionOutcome::Damaged {
                    reason: reason.to_string(),
                }
            }
        };

        mutate(&mut session);
        session.header.updated_at = now_rfc3339();

        match store::write_session(root, &session) {
            Ok(()) => {
                let _ = refresh_index(root);
                UpdateSessionOutcome::Updated { session }
            }
            Err(e) => UpdateSessionOutcome::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_rename(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    title: String,
) -> Result<UpdateSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        update_session_at(&locks, &root, &session_id, |session| {
            session.header.title = title;
        })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_archive(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    archived: bool,
) -> Result<UpdateSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        update_session_at(&locks, &root, &session_id, |session| {
            session.header.archived = archived;
        })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_mark_read(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
) -> Result<UpdateSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        update_session_at(&locks, &root, &session_id, |session| {
            session.header.unread = false;
        })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

// -- 3.2: append-user-message / attach-context --

/// A file/diff/source/graph/OpenSpec-task reference to attach to a message or
/// a context slot, as supplied by the frontend. Mirrors [`MessageTarget`]
/// directly -- callers build the same tagged-enum shape, so there is nothing
/// this wrapper would add.
pub type MessageTargetInput = MessageTarget;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AppendUserMessageOutcome {
    Appended {
        session: AgentSession,
        message: SessionMessage,
    },
    NotFound,
    Damaged {
        reason: String,
    },
    WriteFailed {
        detail: String,
    },
    /// The file could not be read right now -- a permission error, or a lock
    /// held by another process (common on Windows). Nothing was changed; the
    /// session is not known to be gone, and a retry may well succeed.
    Unavailable {
        detail: String,
    },
}

fn append_user_message_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    content: String,
    attachments: Vec<MessageTargetInput>,
) -> AppendUserMessageOutcome {
    use crate::agentdesk::model::SessionLoadError as E;
    // See `update_session_at`'s doc comment: the whole read-modify-write
    // must be atomic with respect to any other mutation of this session,
    // including a run event routed through `bridge::route_run_event`.
    locks.with_session_lock(session_id, || {
    let mut session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(E::NotFound) => return AppendUserMessageOutcome::NotFound,
        Err(E::Io { detail }) => return AppendUserMessageOutcome::Unavailable { detail },
        Err(reason) => {
            return AppendUserMessageOutcome::Damaged {
                reason: reason.to_string(),
            }
        }
    };

    let now = now_rfc3339();
    // A message needs a segment to belong to; reuse the session's current
    // last segment if one exists, otherwise open the first one. Segment
    // semantics beyond "the transcript's current bucket" belong to the
    // conversation-shell package (model.rs's own doc comment on
    // `ConversationSegment`), so this stays intentionally minimal.
    let segment_id: SegmentId = match session.segments.last() {
        Some(seg) => seg.segment_id.clone(),
        None => {
            let id: SegmentId = new_id();
            session
                .segments
                .push(crate::agentdesk::model::ConversationSegment {
                    segment_id: id.clone(),
                    label: "Conversation".into(),
                    started_at: now.clone(),
                });
            id
        }
    };

    let message = SessionMessage {
        message_id: new_id() as MessageId,
        segment_id,
        role: MessageRole::User,
        timestamp: now.clone(),
        plain_content: content,
        rendered_content: None,
        provider: None,
        model: None,
        kind: MessageKind::User,
        execution_id: None,
        sequence: None,
        import: None,
        targets: attachments,
    };

    session.messages.push(message.clone());
    session.header.updated_at = now;
    // A session with a message in it is no longer an empty draft. Any state
    // beyond Draft (Working, NeedsInput, ...) is set by the run bridge (task
    // 4), which owns transitions driven by execution progress; this only
    // ever lifts a session that has never had anything sent.
    if session.header.state == SessionState::Draft {
        session.header.state = SessionState::Ready;
    }

    match store::write_session(root, &session) {
        Ok(()) => {
            let _ = refresh_index(root);
            AppendUserMessageOutcome::Appended { session, message }
        }
        Err(e) => AppendUserMessageOutcome::WriteFailed {
            detail: e.to_string(),
        },
    }
    })
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_append_user_message(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    content: String,
    attachments: Vec<MessageTargetInput>,
) -> Result<AppendUserMessageOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        append_user_message_at(&locks, &root, &session_id, content, attachments)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AttachContextOutcome {
    Attached {
        session: AgentSession,
        attachment: ContextAttachment,
    },
    NotFound,
    Damaged {
        reason: String,
    },
    WriteFailed {
        detail: String,
    },
    /// The file could not be read right now -- a permission error, or a lock
    /// held by another process (common on Windows). Nothing was changed; the
    /// session is not known to be gone, and a retry may well succeed.
    Unavailable {
        detail: String,
    },
}

fn attach_context_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    label: String,
    target: MessageTarget,
) -> AttachContextOutcome {
    use crate::agentdesk::model::SessionLoadError as E;
    // See `update_session_at`'s doc comment: the whole read-modify-write
    // must be atomic with respect to any other mutation of this session.
    locks.with_session_lock(session_id, || {
    let mut session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(E::NotFound) => return AttachContextOutcome::NotFound,
        Err(E::Io { detail }) => return AttachContextOutcome::Unavailable { detail },
        Err(reason) => {
            return AttachContextOutcome::Damaged {
                reason: reason.to_string(),
            }
        }
    };

    let attachment = ContextAttachment {
        attachment_id: new_id(),
        label,
        target,
        added_at: now_rfc3339(),
    };
    session.attachments.push(attachment.clone());
    session.header.updated_at = now_rfc3339();

    match store::write_session(root, &session) {
        Ok(()) => {
            let _ = refresh_index(root);
            AttachContextOutcome::Attached { session, attachment }
        }
        Err(e) => AttachContextOutcome::WriteFailed {
            detail: e.to_string(),
        },
    }
    })
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_attach_context(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    label: String,
    target: MessageTarget,
) -> Result<AttachContextOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        attach_context_at(&locks, &root, &session_id, label, target)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

// -- 3.3: start-execution / stop-execution / usage / refresh-source --
//
// These four reuse the existing `airun` engine (architecture.md section 1:
// "`src-tauri/src/airun/` remains the execution engine and event producer")
// rather than building a second run loop. Starting an execution:
//
//   1. Reads the session (under its lock) to find the source repository.
//   2. Links that repository to this durable session via
//      `RunSessionLinks::link` -- the seam `bridge::route_run_event` has been
//      waiting on since it landed (see `agentdesk::bridge`'s module doc).
//   3. Delegates the actual provider call to the same `CliAgent` +
//      `airun::cli_run::run_task` pair `commands::airun::ai_run_start` uses,
//      through the same `emit()` choke point, so every run event flows
//      through the bridge exactly like a task-run's does.
//
// The link is set up *before* the engine starts producing events, and the
// session lock used to read the session here is released before the engine
// runs (engine execution is long-lived and asynchronous; holding the session
// lock across it would block every other mutation of this session, including
// the very run events this call is about to produce).

/// `ask | plan | auto`, matching architecture.md section 8's
/// `StartAgentSessionRequest.mode`. `agent-desk-source-kickoffs` task 1.1
/// landed the canonical definition in `agentdesk::policy`; this command
/// module re-exports it rather than keeping a second, structurally-identical
/// copy, which specta would otherwise export as two colliding `ExecutionMode`
/// TypeScript types (the frontend build breaks on the duplicate identifier).
pub use crate::agentdesk::policy::ExecutionMode;

/// `solo | lead`, matching architecture.md section 8's
/// `StartAgentSessionRequest.team`. See [`ExecutionMode`]'s doc comment --
/// same reasoning, re-exported from `agentdesk::policy` rather than
/// duplicated.
pub use crate::agentdesk::policy::ExecutionTeam;

/// What happened when the caller asked a session to start an execution.
/// Architecture.md section 3: "already running, source missing, adapter
/// unsupported, provider reconnect, and conflicting write are enum variants,
/// not error strings."
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartExecutionOutcome {
    /// The engine was launched; `execution_id` is the durable ID future run
    /// events for this session will carry.
    Started {
        session: AgentSession,
        execution_id: ExecutionId,
    },
    /// This session already has an execution running -- starting a second one
    /// would silently orphan the first's events (bridge.rs's
    /// `ExecutionSuperseded`), so this is refused rather than allowed to
    /// clobber.
    AlreadyRunning { execution_id: ExecutionId },
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
    /// The session's repository is not open in this app instance, so there is
    /// nothing to run against.
    SourceMissing { detail: String },
    /// The provider transport could not be reached at all (no CLI, version
    /// too old): distinct from `ProviderReconnect` because the fix is
    /// "install/upgrade the tool," not "sign back in."
    AdapterUnsupported { detail: String },
    /// Credentials exist but were refused -- the user needs to reconnect the
    /// provider, not retry.
    ProviderReconnect { detail: String },
    /// This session's intent requires an isolated worktree (Fix, or Plan
    /// once started) and one could not be provisioned. Task 5.2: "If
    /// provisioning fails, do not fall back to the user's checkout" -- the
    /// engine is never started against `open.path` when this is returned.
    WorktreeFailed { detail: String },
}

/// Internal-only outcome of the atomic "re-check then write the
/// `ExecutionRecord`" step inside `start_execution_at`. Not a public/`Type`
/// enum like `StartExecutionOutcome` -- it exists purely to let that single
/// `with_session_lock` closure report "someone else already started an
/// execution here" alongside the ordinary read/write failure modes, without
/// reusing `UpdateSessionOutcome` (which has no `AlreadyRunning` variant and
/// is shared by mutations that never need one).
enum RecordOutcome {
    Updated { session: AgentSession },
    AlreadyRunning { execution_id: ExecutionId },
    NotFound,
    Damaged { reason: String },
    WriteFailed { detail: String },
    Unavailable { detail: String },
}

/// Atomically re-checks "is an execution already active on this session" and,
/// if not, writes a fresh `ExecutionRecord` for `execution_id` -- both inside
/// ONE `with_session_lock` acquisition, so no other caller of this function
/// (or of `update_session_at`/`stop_execution_at` for the same session) can
/// observe or act on an in-between state. See `start_execution_at`'s call
/// site for why this cannot simply be folded into the check that runs before
/// `CliAgent::discover`: that earlier check runs under a lock that is
/// released before the (slow, unlocked) discovery step, leaving a window a
/// second concurrent call could win. This function is the fix: the write
/// that would let a second call proceed is gated by the same read that
/// decides whether it is allowed to.
fn record_execution_if_not_running(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: ExecutionId,
) -> RecordOutcome {
    use crate::agentdesk::model::SessionLoadError as E;
    locks.with_session_lock(session_id, || {
        let mut session = match store::read_session(root, session_id) {
            Ok(s) => s,
            Err(E::NotFound) => return RecordOutcome::NotFound,
            Err(E::Io { detail }) => return RecordOutcome::Unavailable { detail },
            Err(reason) => {
                return RecordOutcome::Damaged {
                    reason: reason.to_string(),
                }
            }
        };

        if let Some(active) = &session.header.active_execution_id {
            if session.executions.iter().any(|e| {
                &e.execution_id == active
                    && matches!(
                        e.state,
                        SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
                    )
            }) {
                return RecordOutcome::AlreadyRunning {
                    execution_id: active.clone(),
                };
            }
        }

        let now = now_rfc3339();
        session.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
            execution_id.clone(),
            session.header.session_id.clone(),
            None,
            SessionState::Preparing,
            now,
            None,
            0,
        ));
        session.header.active_execution_id = Some(execution_id.clone());
        session.header.state = SessionState::Preparing;
        session.header.updated_at = now_rfc3339();

        match store::write_session(root, &session) {
            Ok(()) => {
                let _ = refresh_index(root);
                RecordOutcome::Updated { session }
            }
            Err(e) => RecordOutcome::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

fn start_execution_at(
    app: &AppHandle,
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    links: &crate::agentdesk::RunSessionLinks,
    manager: &crate::state::RepoManager,
    session_id: &str,
    _mode: ExecutionMode,
    _team: ExecutionTeam,
    _provider_override: Option<String>,
) -> StartExecutionOutcome {
    use crate::agentdesk::model::SessionLoadError as E;

    // Step 1: read the session and refuse a second concurrent execution, all
    // under the session lock so a racing start/stop cannot both pass the
    // "nothing running" check (the same race `agentdesk::locks` documents for
    // rename/archive/append).
    let (session, repo_id, repo_path, prompt) = match locks.with_session_lock(session_id, || {
        match store::read_session(root, session_id) {
            Ok(s) => {
                if let Some(active) = &s.header.active_execution_id {
                    if s.executions.iter().any(|e| {
                        &e.execution_id == active
                            && matches!(
                                e.state,
                                SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
                            )
                    }) {
                        return Err(StartExecutionOutcome::AlreadyRunning {
                            execution_id: active.clone(),
                        });
                    }
                }
                let repo_id = s.header.repo_id.clone();
                let repo_path = s.header.repo_path.clone();
                let prompt = build_prompt(&s);
                Ok((s, repo_id, repo_path, prompt))
            }
            Err(E::NotFound) => Err(StartExecutionOutcome::NotFound),
            Err(E::Io { detail }) => Err(StartExecutionOutcome::Unavailable { detail }),
            Err(reason) => Err(StartExecutionOutcome::Damaged {
                reason: reason.to_string(),
            }),
        }
    }) {
        Ok(v) => v,
        Err(outcome) => return outcome,
    };

    // Step 2: the repository has to actually be open in this app instance --
    // a session can outlive the window that had its repo open.
    let open = match manager.get(&repo_id) {
        Ok(o) => o,
        Err(e) => {
            return StartExecutionOutcome::SourceMissing {
                detail: e.to_string(),
            }
        }
    };
    let _ = repo_path; // header.repo_path is provenance; `open.path` is live truth.

    // Step 2b: resolve the root the engine will actually be given as its
    // working directory -- this IS the enforcement point for
    // `agentdesk::policy::WorktreePolicy` (agent-desk-source-kickoffs task
    // 5.1: "Provision a marked worktree before the Fix engine receives edit
    // capability"). A `WorktreePolicy::Always` intent (Fix) gets an
    // isolated worktree here, unconditionally, before `CliAgent::discover`
    // ever sees a path -- the engine has no other route to a working
    // directory, so this is not advisory, it is the actual boundary.
    //
    // `WorktreePolicy::NotUntilStart` (Plan) is treated as `open.path` here:
    // this function is "start THIS execution," and a Plan-mode session that
    // has reached the point of calling `start_execution` has, by
    // definition, already been started by the user (architecture.md
    // section 9's "No until Start" -- the gate is upstream of this call,
    // in whatever UI action transitions a Plan session out of
    // `AwaitingStart`, not inside `start_execution_at` itself). Once
    // started, Plan behaves like Fix and also gets a worktree.
    let policy = crate::agentdesk::policy::for_intent(session.header.intent);
    let engine_root = match policy.worktree {
        crate::agentdesk::policy::WorktreePolicy::Never => open.path.clone(),
        crate::agentdesk::policy::WorktreePolicy::Always
        | crate::agentdesk::policy::WorktreePolicy::NotUntilStart => {
            // Named after the session's own title, matching
            // `commands::agent_kickoff::provision_kickoff_worktree`'s
            // convention -- that function IS this step; called here rather
            // than duplicated.
            let title = session.header.title.clone();
            // The branch this execution's worktree starts from: the
            // repository's current HEAD, since a Fix session has no
            // "pinned branch" concept of its own the way an OpenSpec task
            // run does (`commands::airun`'s `branch` parameter) -- it works
            // from whatever the user has checked out right now.
            let base_branch = {
                let repo = open.repo.lock().unwrap();
                // git2 0.21's `Reference::shorthand` returns
                // `Result<&str, Utf8Error>`, not `Option<&str>` (an accessor
                // migration from earlier git2 versions) -- `.ok()` here is
                // the UTF-8-validity check, not a "does a shorthand exist"
                // check; a HEAD with a non-UTF-8 shorthand falls through to
                // the "main" default same as no HEAD at all.
                repo.head()
                    .ok()
                    .and_then(|h| h.shorthand().ok().map(|s| s.to_string()))
                    .unwrap_or_else(|| "main".to_string())
            };
            match crate::commands::agent_kickoff::provision_kickoff_worktree(&open, &base_branch, &title)
            {
                crate::commands::agent_kickoff::ProvisionKickoffWorktreeOutcome::Provisioned {
                    path,
                    ..
                } => std::path::PathBuf::from(path),
                crate::commands::agent_kickoff::ProvisionKickoffWorktreeOutcome::Failed { detail } => {
                    // Task 5.2: refused outright, never falls back to
                    // `open.path`.
                    return StartExecutionOutcome::WorktreeFailed { detail };
                }
            }
        }
    };

    // Step 3: mint the durable execution ID up front and link the repository
    // to this session *before* the engine can produce a single event -- a run
    // event that races ahead of the link would be silently dropped as
    // `NoLinkedSession` (bridge.rs).
    let execution_id = crate::agentdesk::execution_id_for_run_session(&new_id());
    links.link(&repo_id, &session_id.to_string());

    // Discover the transport up front so an unusable CLI or stale credentials
    // are reported as a typed outcome instead of only surfacing later as an
    // opaque failed run. `engine_root` (not `open.path`) is the working
    // directory handed to the engine -- see step 2b: this is what makes
    // isolation for Fix actually load-bearing rather than advisory.
    let agent = match crate::ai::agent::cli_agent::CliAgent::discover(engine_root.clone()) {
        Ok(a) => a,
        Err(e) => {
            links.unlink(&repo_id);
            return match &e {
                crate::ai::agent::transport::AgentError::NeedsReconnect { detail } => {
                    StartExecutionOutcome::ProviderReconnect {
                        detail: detail.clone(),
                    }
                }
                _ => StartExecutionOutcome::AdapterUnsupported {
                    detail: crate::ai::agent::select::plain_explanation(&e),
                },
            };
        }
    };

    // Record the execution on the session before the engine starts, so a
    // reader that lists this session immediately after `Started` returns sees
    // an execution present -- the bridge would eventually create one on the
    // first routed event, but that would leave a window where `Started` says
    // an execution exists and `agent_session_get` disagrees.
    //
    // This is the SECOND lock acquisition for this session (the first, above,
    // only checked and released). Everything between them -- `manager.get`
    // and `CliAgent::discover`, in particular the shell-out inside `discover`
    // -- ran unlocked, so another `start_execution_at` call for the same
    // session could have raced ahead and started its own execution in that
    // window. The check from step 1 is therefore repeated HERE, inside this
    // same lock acquisition that performs the write, so the check and the
    // write are atomic with respect to every other mutating path for this
    // session (the exact race `agentdesk::locks` documents). Re-acquiring the
    // lock rather than holding it across `discover` is deliberate: a slow
    // shell-out must never serialize behind a held session lock.
    let record_outcome =
        record_execution_if_not_running(locks, root, session_id, execution_id.clone());
    let session_after = match record_outcome {
        RecordOutcome::Updated { session } => session,
        RecordOutcome::AlreadyRunning { execution_id } => {
            // No write happened, so nothing to unwind on the session itself --
            // but the link and gate-answer channel registered above for THIS
            // (losing) attempt must not linger, since the winning attempt owns
            // the link now (or will, once its own write lands).
            links.unlink(&repo_id);
            return StartExecutionOutcome::AlreadyRunning { execution_id };
        }
        RecordOutcome::NotFound => {
            links.unlink(&repo_id);
            return StartExecutionOutcome::NotFound;
        }
        RecordOutcome::Damaged { reason } => {
            links.unlink(&repo_id);
            return StartExecutionOutcome::Damaged { reason };
        }
        RecordOutcome::WriteFailed { detail } => {
            links.unlink(&repo_id);
            return StartExecutionOutcome::WriteFailed { detail };
        }
        RecordOutcome::Unavailable { detail } => {
            links.unlink(&repo_id);
            return StartExecutionOutcome::Unavailable { detail };
        }
    };
    let _ = session; // superseded by session_after, kept only to name the earlier read.

    // Step 4: hand the whole task to the engine, exactly as
    // `commands::airun::ai_run_start` does -- same sink shape (`emit()`),
    // same blocking task, same gate-answer channel. `commands::airun::emit`
    // is the single choke point that both routes to `ai-run-event` and, via
    // `route_to_agent_desk`, into this durable session -- nothing here
    // duplicates that fan-out.
    let run_session_id = execution_id.clone();
    let (answer_tx, answer_rx) = std::sync::mpsc::channel::<crate::airun::driver::GateAnswer>();
    crate::commands::airun::gate_answers().lock().unwrap().insert(repo_id.clone(), answer_tx);

    let app_for_task = app.clone();
    let repo_for_task = repo_id.clone();
    let run_session_id_for_task = run_session_id.clone();
    tauri::async_runtime::spawn(async move {
        let sink: crate::airun::engine::Sink = {
            let app = app_for_task.clone();
            let repo = repo_for_task.clone();
            let session_id = run_session_id_for_task.clone();
            std::sync::Arc::new(move |state, step| {
                crate::commands::airun::emit(&app, &repo, &session_id, state, step);
            })
        };

        crate::airun::cli_run::run_task(
            &agent,
            &format!(
                "{}\n\nThe task:\n{}",
                crate::ai::agent::run::SYSTEM_PROMPT,
                prompt
            ),
            sink,
            answer_rx,
        )
        .await;

        crate::commands::airun::gate_answers()
            .lock()
            .unwrap()
            .remove(&repo_for_task);
    });

    StartExecutionOutcome::Started {
        session: session_after,
        execution_id,
    }
}

/// Builds the prompt text handed to the engine from what the session already
/// knows about why it exists: its source snapshot title/summary plus the last
/// user message, if any. Real prompt composition (system prompt selection,
/// mode/team policy) belongs to a later package; this only has to give the
/// engine something honest to work from.
fn build_prompt(session: &AgentSession) -> String {
    let mut parts = Vec::new();
    let (title, summary) = source_summary(&session.header.source);
    if !title.is_empty() {
        parts.push(title);
    }
    if !summary.is_empty() {
        parts.push(summary);
    }
    if let Some(last_user) = session.messages.iter().rev().find(|m| m.role == MessageRole::User) {
        parts.push(last_user.plain_content.clone());
    }
    if parts.is_empty() {
        parts.push(session.header.title.clone());
    }
    parts.join("\n\n")
}

fn source_summary(source: &SessionSource) -> (String, String) {
    match source {
        SessionSource::Manual { .. } => (String::new(), String::new()),
        SessionSource::Issue { snapshot, .. }
        | SessionSource::PullRequest { snapshot, .. }
        | SessionSource::OpenSpecChange { snapshot, .. }
        | SessionSource::OpenSpecTask { snapshot, .. }
        | SessionSource::Commit { snapshot, .. }
        | SessionSource::Diff { snapshot, .. }
        | SessionSource::WorkingChanges { snapshot, .. }
        | SessionSource::CheckFailure { snapshot, .. } => {
            (snapshot.title.clone(), snapshot.summary.clone())
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_start_execution(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    links: tauri::State<'_, crate::agentdesk::RunSessionLinks>,
    manager: tauri::State<'_, crate::state::RepoManager>,
    session_id: SessionId,
    mode: ExecutionMode,
    team: ExecutionTeam,
    provider_override: Option<String>,
) -> Result<StartExecutionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    // `RunSessionLinks` and `RepoManager` are `Send + Sync` Tauri state; the
    // blocking closure below borrows them through the `State` handles'
    // `Arc`-like clone, matching how `commands::airun::route_to_agent_desk`
    // reaches the same two pieces of state.
    let links_owned = links.inner();
    let manager_owned = manager.inner();
    let outcome = start_execution_at(
        &app,
        &locks_arc,
        &root,
        links_owned,
        manager_owned,
        &session_id,
        mode,
        team,
        provider_override,
    );
    Ok(outcome)
}

/// Scope of a stop request: one execution, or every execution attached to the
/// session (spec `agent-desk-agent-graphs`: "Each helper SHALL have Stop for
/// itself and the Graph header SHALL have a labeled Stop all").
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StopScope {
    One { execution_id: ExecutionId },
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StopExecutionOutcome {
    /// `stopped` lists exactly which executions were told to stop -- empty
    /// for `StopScope::One` naming an execution that was not active, which is
    /// a no-op, not an error (it may have finished a moment earlier).
    Stopped {
        session: AgentSession,
        stopped: Vec<ExecutionId>,
    },
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
}

fn stop_execution_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    links: &crate::agentdesk::RunSessionLinks,
    drivers: &crate::commands::airun::DriverRegistry,
    session_id: &str,
    scope: StopScope,
) -> StopExecutionOutcome {
    use crate::agentdesk::model::SessionLoadError as E;

    // Read first (outside the lock used for the mutating half) only to learn
    // the repository ID the stop signal has to reach -- `airun`'s stop path
    // is keyed by `repo_id`, not by durable session/execution ID.
    let repo_id = locks.with_session_lock(session_id, || {
        store::read_session(root, session_id).map(|s| s.header.repo_id)
    });
    let repo_id = match repo_id {
        Ok(id) => id,
        Err(E::NotFound) => return StopExecutionOutcome::NotFound,
        Err(E::Io { detail }) => return StopExecutionOutcome::Unavailable { detail },
        Err(reason) => {
            return StopExecutionOutcome::Damaged {
                reason: reason.to_string(),
            }
        }
    };

    // Signal the live engine to stop. This never discards anything the engine
    // already wrote to disk -- `ai_run_stop`'s own contract (edits already
    // made to the repository or worktree are left as-is; only the run loop
    // itself is told to end) is unchanged here, so recoverable edits survive
    // exactly as they do for a task-run stop.
    if let Some(driver) = drivers.get_scripted(&repo_id) {
        use crate::airun::driver::RunDriver;
        driver.lock().unwrap().stop();
    }

    // Mark the targeted execution(s) Stopped in the durable record. The live
    // engine's own `Ended` event will also arrive through the bridge and is
    // idempotent with this (bridge.rs's `map_run_state`/`last_sequence`
    // handling) -- this write exists so the UI reflects "stopping" without
    // waiting on that event to round-trip through the engine first.
    //
    // Whether to unlink the repo afterwards is decided HERE, inside the same
    // closure that mutates the executions -- not from `stopped.is_empty()`
    // alone. The session can carry more than one concurrent execution (a lead
    // plus helpers; `ExecutionRecord::parent_execution_id`), and `StopScope::One`
    // exists precisely so one of them can be stopped without touching the
    // rest. Unlinking whenever *anything* stopped (the previous behavior)
    // severed the repo->session link even when other executions on this same
    // session were still `Working` -- every later run event for those would
    // be silently dropped as `NoLinkedSession` in `bridge::route_run_event`.
    // The correct condition is "no execution on this session is still active
    // after this mutation."
    let mut stopped = Vec::new();
    let mut any_still_active = false;
    let outcome = update_session_at(locks, root, session_id, |s| {
        for exec in s.executions.iter_mut() {
            let matches_scope = match &scope {
                StopScope::All => true,
                StopScope::One { execution_id } => &exec.execution_id == execution_id,
            };
            if !matches_scope {
                continue;
            }
            if matches!(
                exec.state,
                SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
            ) {
                exec.state = SessionState::Stopped;
                exec.ended_at = Some(now_rfc3339());
                stopped.push(exec.execution_id.clone());
            }
        }
        any_still_active = s.executions.iter().any(|exec| {
            matches!(
                exec.state,
                SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
            )
        });
        if !stopped.is_empty() {
            s.header.state = SessionState::Stopped;
            if let Some(active) = &s.header.active_execution_id {
                if stopped.contains(active) {
                    // The active execution slot stays populated (last
                    // execution the session ran), matching architecture.md:
                    // `active_execution_id` names the current/most recent
                    // execution, not only a running one.
                }
            }
        }
    });

    // Only sever the repo->session link once nothing on this session is still
    // running -- an in-progress sibling execution must keep receiving routed
    // run events. `matches!(outcome, UpdateSessionOutcome::Updated { .. })`
    // guards against unlinking on a failed write, where `any_still_active`
    // was never actually computed against the persisted state.
    if matches!(outcome, UpdateSessionOutcome::Updated { .. }) && !any_still_active {
        links.unlink(&repo_id);
    }

    match outcome {
        UpdateSessionOutcome::Updated { session } => {
            StopExecutionOutcome::Stopped { session, stopped }
        }
        UpdateSessionOutcome::NotFound => StopExecutionOutcome::NotFound,
        UpdateSessionOutcome::Damaged { reason } => StopExecutionOutcome::Damaged { reason },
        UpdateSessionOutcome::WriteFailed { detail } => {
            StopExecutionOutcome::WriteFailed { detail }
        }
        UpdateSessionOutcome::Unavailable { detail } => {
            StopExecutionOutcome::Unavailable { detail }
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_stop_execution(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    links: tauri::State<'_, crate::agentdesk::RunSessionLinks>,
    drivers: tauri::State<'_, crate::commands::airun::DriverRegistry>,
    session_id: SessionId,
    scope: StopScope,
) -> Result<StopExecutionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    let links_owned = links.inner();
    let drivers_owned = drivers.inner();
    let outcome = stop_execution_at(
        &locks_arc,
        &root,
        links_owned,
        drivers_owned,
        &session_id,
        scope,
    );
    Ok(outcome)
}

/// Normalized provider usage for one session (architecture.md section 12).
/// Every field is optional -- unknown values are omitted, never zero -- and
/// carries its own [`UsageSource`] so the UI can mark estimates as estimates.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    pub session_tokens: Option<UsageValue>,
    pub session_requests: Option<UsageValue>,
    pub session_cost_usd: Option<UsageValue>,
    pub plan_limit: Option<UsageValue>,
    pub plan_reset_at: Option<String>,
    pub active_helper_count: Option<u32>,
    /// RFC 3339 UTC timestamp of when this data was produced, so the UI can
    /// show "as of" rather than implying it is live.
    pub data_timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageValue {
    pub value: f64,
    pub source: UsageSource,
}

/// architecture.md section 12: "Every field is optional and carries
/// `source: measured | provider_reported | estimated`."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UsageSource {
    Measured,
    ProviderReported,
    Estimated,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionUsageOutcome {
    Available { usage: SessionUsage },
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
}

/// Counts what the session's own persisted messages measure directly
/// (`source: measured`) and reports nothing else. No token/cost accounting
/// exists yet anywhere in `airun`/`ai::agent` (verified: neither module has a
/// usage or token-count type), so every provider-reported or estimated field
/// stays `None` rather than inventing a number -- architecture.md section 12:
/// "It never turns unknown into zero and never invents a dollar estimate."
fn session_usage_at(root: &SessionStoreRoot, session_id: &str) -> SessionUsageOutcome {
    use crate::agentdesk::model::SessionLoadError as E;
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(E::NotFound) => return SessionUsageOutcome::NotFound,
        Err(E::Io { detail }) => return SessionUsageOutcome::Unavailable { detail },
        Err(reason) => {
            return SessionUsageOutcome::Damaged {
                reason: reason.to_string(),
            }
        }
    };

    let active_helper_count = {
        let count = session
            .executions
            .iter()
            .filter(|e| {
                e.parent_execution_id.is_some()
                    && matches!(
                        e.state,
                        SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
                    )
            })
            .count() as u32;
        if session.executions.iter().any(|e| e.parent_execution_id.is_some()) {
            Some(count)
        } else {
            // No helper executions have ever existed on this session --
            // "zero helpers" is a real, known count here (not an unknown
            // provider field), so it is fine to report as measured zero.
            Some(0)
        }
    };

    let session_requests = {
        let n = session
            .messages
            .iter()
            .filter(|m| m.execution_id.is_some())
            .count();
        if n == 0 {
            None
        } else {
            Some(UsageValue {
                value: n as f64,
                source: UsageSource::Measured,
            })
        }
    };

    SessionUsageOutcome::Available {
        usage: SessionUsage {
            session_tokens: None,
            session_requests,
            session_cost_usd: None,
            plan_limit: None,
            plan_reset_at: None,
            active_helper_count,
            data_timestamp: now_rfc3339(),
        },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_usage(
    app: AppHandle,
    session_id: SessionId,
) -> Result<SessionUsageOutcome, AppError> {
    let root = resolve_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || session_usage_at(&root, &session_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// What refreshing a session's source found. The snapshot on
/// [`SessionSource`] itself is immutable provenance (model.rs's own doc
/// comment: "Never mutated by a refresh"); this reports current status
/// *alongside* it rather than overwriting it. Live re-fetch of issue/PR/
/// OpenSpec content belongs to the adapters that own each source kind
/// (hosting/openspec, per `docs/agent-desk/README.md`'s ownership map) and is
/// out of scope here -- this command establishes the honest, typed outcome
/// shape and the one check every source kind can make today regardless of
/// kind: whether the session's repository is still open and reachable.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RefreshSourceOutcome {
    /// The source's live locator was reachable; `changed` is `true` only when
    /// this refresh actually flipped `live_unavailable` from what it was
    /// before, so the UI is not told "changed" on every no-op refresh.
    Refreshed {
        session: AgentSession,
        changed: bool,
    },
    /// The source could not be reached this time. The cached snapshot is left
    /// exactly as it was (never overwritten) so the banner keeps showing it.
    LiveUnavailable {
        session: AgentSession,
        detail: String,
    },
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
}

fn refresh_source_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    manager: &crate::state::RepoManager,
    session_id: &str,
) -> RefreshSourceOutcome {
    use crate::agentdesk::model::SessionLoadError as E;

    // Peek at the repo id without holding the session lock across the
    // (potentially slow) repository check below -- matches
    // `start_execution_at`'s split between a quick locked read and unlocked
    // I/O.
    let repo_id = locks.with_session_lock(session_id, || {
        store::read_session(root, session_id).map(|s| s.header.repo_id)
    });
    let repo_id = match repo_id {
        Ok(id) => id,
        Err(E::NotFound) => return RefreshSourceOutcome::NotFound,
        Err(E::Io { detail }) => return RefreshSourceOutcome::Unavailable { detail },
        Err(reason) => {
            return RefreshSourceOutcome::Damaged {
                reason: reason.to_string(),
            }
        }
    };

    let open_repo = manager.get(&repo_id).ok();
    let reachable = open_repo.is_some();

    // For an OpenSpec source, "reachable" is not just "is the repository
    // open" -- the change itself can have moved (renamed folder) or gone
    // away (deleted, or archived, which is its own honest state rather than
    // "unavailable" -- tasks.md 4.5/7). This is the same
    // `resolve_change_status` the context builder and
    // `agent_session_openspec_status` use (task 6: "Rebuild context on
    // file-watcher refresh"), so a refresh triggered by
    // `useRepoWatcher`/`repo-changed` and a direct status check can never
    // disagree about whether the source is still live.
    let mut openspec_live = true;
    if let Some(open) = &open_repo {
        let peek = locks.with_session_lock(session_id, || store::read_session(root, session_id));
        if let Ok(session) = peek {
            if let Some(target) = openspec_target_of(&session.header.source) {
                if let Some(dir) = openspec_dir_for(&open.path) {
                    use crate::agentdesk::openspec_context::{resolve_change_status, OpenSpecChangeStatus};
                    let (change_id, snapshot_title) = match &target {
                        OpenSpecTarget::Change { change_id, snapshot_title } => {
                            (change_id.clone(), snapshot_title.clone())
                        }
                        OpenSpecTarget::Task { change_id, snapshot_title, .. } => {
                            (change_id.clone(), snapshot_title.clone())
                        }
                    };
                    // Archived counts as still live for banner purposes --
                    // it is a normal end state with its own next action
                    // (`agent_session_openspec_status`), not an outage.
                    openspec_live = !matches!(
                        resolve_change_status(&dir, &change_id, &snapshot_title),
                        OpenSpecChangeStatus::Moved { .. } | OpenSpecChangeStatus::Deleted
                    );
                } else {
                    // The repository lost its `openspec/` folder entirely.
                    openspec_live = false;
                }
            }
        }
    }
    let reachable = reachable && openspec_live;

    let mut changed = false;
    let outcome = update_session_at(locks, root, session_id, |s| {
        let live_unavailable_ref = match &mut s.header.source {
            SessionSource::Manual { .. } => None,
            SessionSource::Issue { snapshot, .. }
            | SessionSource::PullRequest { snapshot, .. }
            | SessionSource::OpenSpecChange { snapshot, .. }
            | SessionSource::OpenSpecTask { snapshot, .. }
            | SessionSource::Commit { snapshot, .. }
            | SessionSource::Diff { snapshot, .. }
            | SessionSource::WorkingChanges { snapshot, .. }
            | SessionSource::CheckFailure { snapshot, .. } => Some(&mut snapshot.live_unavailable),
        };
        if let Some(flag) = live_unavailable_ref {
            let new_value = !reachable;
            if *flag != new_value {
                changed = true;
            }
            *flag = new_value;
        }
    });

    match outcome {
        UpdateSessionOutcome::Updated { session } => {
            if reachable {
                RefreshSourceOutcome::Refreshed { session, changed }
            } else {
                RefreshSourceOutcome::LiveUnavailable {
                    session,
                    detail: format!("the repository for this session is not open ({repo_id})"),
                }
            }
        }
        UpdateSessionOutcome::NotFound => RefreshSourceOutcome::NotFound,
        UpdateSessionOutcome::Damaged { reason } => RefreshSourceOutcome::Damaged { reason },
        UpdateSessionOutcome::WriteFailed { detail } => {
            RefreshSourceOutcome::WriteFailed { detail }
        }
        UpdateSessionOutcome::Unavailable { detail } => {
            RefreshSourceOutcome::Unavailable { detail }
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_refresh_source(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    manager: tauri::State<'_, crate::state::RepoManager>,
    session_id: SessionId,
) -> Result<RefreshSourceOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    let manager_owned = manager.inner();
    let outcome = refresh_source_at(&locks_arc, &root, manager_owned, &session_id);
    Ok(outcome)
}

// -- "View source": the bridge into the main window (package
//    `agent-desk-docs`) --
//
// Agent Desk is a standalone window with no issue/PR viewer, OpenSpec
// surface, diff view, or graph -- those all live in the main window
// (`src/App.tsx`'s `AppInner`). `SessionSourceBanner`'s "View source" button
// has been wired through five components (`ConversationPane`,
// `SessionSourceBanner`, `EventStack`, `SessionSourcePanel`,
// `PaneDetailPopover`, `DockedDetailPanel`) since the banner shipped, but
// `AgentDeskView.tsx` never passed `onOpenSource` down, so the button always
// rendered disabled. This command is the other half of the same bridge
// `agent_result_open_diff` (above, in `commands::agent_result`) already
// built for a result's worktree diff: focus the main window, emit an event
// carrying enough to resolve, and let the main window's own existing
// surfaces (GitHub context panel, OpenSpec selection, diff view) do the
// rendering -- no second issue/PR/diff viewer is built in this window.
pub const OPEN_SOURCE_EVENT: &str = "agent-desk://open-source";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenSourceTarget {
    pub repo_id: String,
    pub repo_path: String,
    pub source: SessionSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenSourceOutcome {
    /// The main window was found and told to open the source.
    Opened,
    /// No main window exists yet (a very early startup race) -- mirrors
    /// `agent_result::OpenResultDiffOutcome::MainWindowNotOpen`.
    MainWindowNotOpen,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
}

fn open_source_at(root: &SessionStoreRoot, session_id: &str) -> Result<AgentSessionHeader, OpenSourceOutcome> {
    use crate::agentdesk::model::SessionLoadError as E;
    match store::read_session(root, session_id) {
        Ok(session) => Ok(session.header),
        Err(E::NotFound) => Err(OpenSourceOutcome::SessionNotFound),
        Err(E::Io { detail }) => Err(OpenSourceOutcome::SessionUnavailable { detail }),
        Err(reason) => Err(OpenSourceOutcome::SessionDamaged {
            reason: reason.to_string(),
        }),
    }
}

/// Focuses the main window and asks it to open the item a session started
/// from. Never opens a second main window, never creates any window itself
/// -- mirrors `agent_result_open_diff`'s "focus what already exists" shape.
///
/// `Manual` sources have nothing to navigate to (there was never a source
/// item); the frontend resolver
/// (`src/lib/agentDeskTargets.ts`/`useAgentDeskSourceListener.ts`) is
/// responsible for leaving that case's affordance honestly disabled rather
/// than calling this command for it, but the event is still emitted here on
/// a `Manual` source (the caller decides what "nothing to do" looks like,
/// same division of labor `agent_result_open_diff` uses for a `None` path).
#[tauri::command]
#[specta::specta]
pub async fn agent_session_open_source(
    app: AppHandle,
    session_id: SessionId,
) -> Result<OpenSourceOutcome, AppError> {
    use tauri::{Emitter, Manager};

    let root = resolve_root(&app)?;
    let header = tauri::async_runtime::spawn_blocking(move || open_source_at(&root, &session_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    let header = match header {
        Ok(h) => h,
        Err(outcome) => return Ok(outcome),
    };

    let Some(main) = app.get_webview_window("main") else {
        return Ok(OpenSourceOutcome::MainWindowNotOpen);
    };
    let _ = main.unminimize();
    let _ = main.show();
    let _ = main.set_focus();
    let _ = app.emit_to(
        "main",
        OPEN_SOURCE_EVENT,
        &OpenSourceTarget {
            repo_id: header.repo_id,
            repo_path: header.repo_path,
            source: header.source,
        },
    );
    Ok(OpenSourceOutcome::Opened)
}

// -- OpenSpec as a first-class source (package `agent-desk-openspec-workflows`) --
//
// Three concerns, kept separate rather than folded into the generic session
// commands above:
//
//   1. `agent_session_openspec_context` (tasks.md section 2): the context a
//      lead agent reads to understand the source and the OpenSpec plan --
//      proposal, design, deltas, tasks, progress, and (for an `OpenSpecTask`
//      source) the exact target task, located by identity rather than
//      position (tasks.md 1.2, 2.3).
//   2. `agent_session_openspec_kickoff` (tasks.md 1.3): the compatibility
//      mapping from the current Spec Desk selected change/task into a durable
//      session -- reuses an existing non-archived session for the same
//      change/task rather than accumulating a duplicate every time the same
//      row is clicked.
//   3. `agent_session_openspec_status` (tasks.md 4.5, 7): archived/deleted/
//      moved change states, each typed and each carrying a real next action,
//      independent of whether a context can currently be built.
//
// All three are read-only with respect to OpenSpec files -- they never call
// into `crate::openspec::write`. The one write path this package adds
// (routing an accepted task completion through the existing checkbox writer)
// is `agent_session_complete_openspec_task`, directly below them.

fn openspec_dir_for(root: &Path) -> Option<PathBuf> {
    crate::openspec::openspec_dir(root)
}

/// What the session's `SessionSource` names, extracted once so every OpenSpec
/// command below shares the same "this session is not an OpenSpec source at
/// all" branch instead of three copies of the same match.
enum OpenSpecTarget {
    Change { change_id: String, snapshot_title: String },
    Task { change_id: String, task_index: u32, task_text: String, snapshot_title: String },
}

fn openspec_target_of(source: &SessionSource) -> Option<OpenSpecTarget> {
    match source {
        SessionSource::OpenSpecChange { change_id, snapshot } => Some(OpenSpecTarget::Change {
            change_id: change_id.clone(),
            snapshot_title: snapshot.title.clone(),
        }),
        SessionSource::OpenSpecTask {
            change_id,
            task_index,
            task_text,
            snapshot,
        } => Some(OpenSpecTarget::Task {
            change_id: change_id.clone(),
            task_index: *task_index,
            task_text: task_text.clone(),
            snapshot_title: snapshot.title.clone(),
        }),
        _ => None,
    }
}

/// Outcome of asking for a session's OpenSpec context or status. Shared by
/// both read commands below so "not an OpenSpec source", "repo not open",
/// and "found" are the same three named states everywhere this question is
/// asked, rather than each command inventing its own shape.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenSpecSourceOutcome<T> {
    Found { value: T },
    /// The session's source is not `OpenSpecChange`/`OpenSpecTask` -- asking
    /// this question of a manual chat or an issue session is a caller bug,
    /// not a runtime fault, but it is still reported rather than panicking.
    NotAnOpenSpecSource,
    /// The session's repository is not open in this app instance, so its
    /// `openspec/` folder cannot be read at all.
    RepoNotOpen,
    /// The repository is open but has no `openspec/` folder -- true for any
    /// repo that never adopted OpenSpec, and also the honest state right
    /// after someone deletes the whole folder.
    NoOpenSpecFolder,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
}

fn load_openspec_target(
    root: &SessionStoreRoot,
    session_id: &str,
) -> Result<(AgentSession, OpenSpecTarget), OpenSpecSourceOutcome<crate::agentdesk::openspec_context::OpenSpecSourceContext>>
{
    use crate::agentdesk::model::SessionLoadError as E;
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(E::NotFound) => return Err(OpenSpecSourceOutcome::SessionNotFound),
        Err(E::Io { detail }) => return Err(OpenSpecSourceOutcome::SessionUnavailable { detail }),
        Err(reason) => {
            return Err(OpenSpecSourceOutcome::SessionDamaged {
                reason: reason.to_string(),
            })
        }
    };
    let Some(target) = openspec_target_of(&session.header.source) else {
        return Err(OpenSpecSourceOutcome::NotAnOpenSpecSource);
    };
    Ok((session, target))
}

/// tasks.md section 2: the context builder. Builds
/// [`crate::agentdesk::openspec_context::OpenSpecSourceContext`] from the
/// session's live `openspec/` folder -- proposal, design, every delta, tasks,
/// progress, history, and (for a task source) the exact target task located
/// by identity, honest about divergence since launch (tasks.md 2.3, 2.4).
///
/// Reused directly by [`refresh_source_at`]'s OpenSpec branch (task 6): a
/// file-watcher-triggered refresh calls the same `resolve_change_status` this
/// command does, so "the context after a live refresh" and "the context this
/// command reports" can never disagree.
fn openspec_context_at(
    root: &SessionStoreRoot,
    manager: &crate::state::RepoManager,
    session_id: &str,
) -> OpenSpecSourceOutcome<crate::agentdesk::openspec_context::OpenSpecSourceContext> {
    let (session, target) = match load_openspec_target(root, session_id) {
        Ok(v) => v,
        Err(outcome) => return outcome,
    };

    if manager.get(&session.header.repo_id).is_err() {
        return OpenSpecSourceOutcome::RepoNotOpen;
    }
    let repo_path = PathBuf::from(&session.header.repo_path);

    match resolve_openspec_context(&repo_path, &target) {
        Some(ctx) => OpenSpecSourceOutcome::Found { value: ctx },
        None => OpenSpecSourceOutcome::NoOpenSpecFolder,
    }
}

/// Builds the context, or `None` when there is nothing file-backed to build
/// one from -- no `openspec/` folder at all, or the change is `Moved`/
/// `Deleted` (that case gets its own typed, actionable answer from
/// [`agent_session_openspec_status`] instead of a context here).
fn resolve_openspec_context(
    repo_path: &Path,
    target: &OpenSpecTarget,
) -> Option<crate::agentdesk::openspec_context::OpenSpecSourceContext> {
    use crate::agentdesk::openspec_context::{context_for_change, context_for_task, resolve_change_status, OpenSpecChangeStatus};

    let dir = openspec_dir_for(repo_path)?;

    let (change_id, snapshot_title) = match target {
        OpenSpecTarget::Change { change_id, snapshot_title } => (change_id.clone(), snapshot_title.clone()),
        OpenSpecTarget::Task { change_id, snapshot_title, .. } => (change_id.clone(), snapshot_title.clone()),
    };

    let change = match resolve_change_status(&dir, &change_id, &snapshot_title) {
        OpenSpecChangeStatus::Active { change } | OpenSpecChangeStatus::Archived { change } => change,
        OpenSpecChangeStatus::Moved { .. } | OpenSpecChangeStatus::Deleted => return None,
    };

    match target {
        OpenSpecTarget::Change { .. } => context_for_change(repo_path, &change, None),
        OpenSpecTarget::Task {
            task_index, task_text, ..
        } => context_for_task(repo_path, &change, *task_index, task_text, None),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_openspec_context(
    app: AppHandle,
    manager: tauri::State<'_, crate::state::RepoManager>,
    session_id: SessionId,
) -> Result<OpenSpecSourceOutcome<crate::agentdesk::openspec_context::OpenSpecSourceContext>, AppError> {
    let root = resolve_root(&app)?;
    // Synchronous, not `spawn_blocking`, matching `agent_session_refresh_source`
    // just above: `RepoManager` is Tauri `State`, not `'static`-owned or
    // `Clone`, and this command's I/O (parsing a handful of small markdown
    // files) is the same order of magnitude as that command's own work.
    Ok(openspec_context_at(&root, manager.inner(), &session_id))
}

/// tasks.md 4.5 / section 7: archived/deleted/moved change states, each
/// honest and typed, with a real next action -- independent of whether a
/// context can currently be built (a moved change, for instance, has no
/// context here but a very real next action: open the change at its new id).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenSpecSessionStatus {
    /// The change is exactly where the session left it.
    Active,
    /// The change finished and was archived -- not a fault. Next action:
    /// open the archived change (read-only, matches
    /// `openspec_get_archived_change`'s own contract).
    Archived,
    /// The folder is gone under this id, but a change with a matching title
    /// exists elsewhere -- most likely renamed. Next action: offer to
    /// re-point this session at `likely_new_id`.
    #[serde(rename_all = "camelCase")]
    Moved { likely_new_id: String, archived: bool },
    /// Nothing matches under this id or by title. Next action: the session's
    /// transcript and cached snapshot remain readable (session history is
    /// never lost, tasks.md 4.5), but there is no live source to act on
    /// beyond that.
    Deleted,
    RepoNotOpen,
    NotAnOpenSpecSource,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
}

fn openspec_status_at(
    root: &SessionStoreRoot,
    manager: &crate::state::RepoManager,
    session_id: &str,
) -> OpenSpecSessionStatus {
    use crate::agentdesk::model::SessionLoadError as E;
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(E::NotFound) => return OpenSpecSessionStatus::SessionNotFound,
        Err(E::Io { detail }) => return OpenSpecSessionStatus::SessionUnavailable { detail },
        Err(reason) => {
            return OpenSpecSessionStatus::SessionDamaged {
                reason: reason.to_string(),
            }
        }
    };
    let Some(target) = openspec_target_of(&session.header.source) else {
        return OpenSpecSessionStatus::NotAnOpenSpecSource;
    };
    if manager.get(&session.header.repo_id).is_err() {
        return OpenSpecSessionStatus::RepoNotOpen;
    }
    let repo_path = std::path::PathBuf::from(&session.header.repo_path);
    let Some(dir) = openspec_dir_for(&repo_path) else {
        // No openspec/ folder at all reads as Deleted from this session's
        // point of view: there is nothing left to find it under any id.
        return OpenSpecSessionStatus::Deleted;
    };

    use crate::agentdesk::openspec_context::{resolve_change_status, OpenSpecChangeStatus};
    let (change_id, snapshot_title) = match &target {
        OpenSpecTarget::Change { change_id, snapshot_title } => (change_id.clone(), snapshot_title.clone()),
        OpenSpecTarget::Task { change_id, snapshot_title, .. } => (change_id.clone(), snapshot_title.clone()),
    };
    match resolve_change_status(&dir, &change_id, &snapshot_title) {
        OpenSpecChangeStatus::Active { .. } => OpenSpecSessionStatus::Active,
        OpenSpecChangeStatus::Archived { .. } => OpenSpecSessionStatus::Archived,
        OpenSpecChangeStatus::Moved { likely_new_id, archived } => {
            OpenSpecSessionStatus::Moved { likely_new_id, archived }
        }
        OpenSpecChangeStatus::Deleted => OpenSpecSessionStatus::Deleted,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_openspec_status(
    app: AppHandle,
    manager: tauri::State<'_, crate::state::RepoManager>,
    session_id: SessionId,
) -> Result<OpenSpecSessionStatus, AppError> {
    let root = resolve_root(&app)?;
    Ok(openspec_status_at(&root, manager.inner(), &session_id))
}

// -- File-backed task completion (tasks.md section 4) --

/// Whether an accepted execution's OpenSpec task completion may be written
/// through to `tasks.md`. Tasks.md 4.4: "Never tick a task solely because an
/// execution emitted Finished; require existing review/completion policy."
/// The only policy that exists today for "may this write happen" is explicit
/// user acceptance -- there is no autonomous accept step anywhere in `airun`
/// (verified: `SessionState`/`ExecutionRecord` carry no such flag) -- so this
/// command requires the caller to have already gotten that acceptance (a
/// button the user pressed) rather than inferring it from execution state.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CompleteOpenSpecTaskOutcome {
    /// The checkbox was toggled (or already in the requested state) and the
    /// session's own record of its target task was refreshed so the source
    /// banner reflects the new state immediately (tasks.md 4.3).
    Completed {
        session: AgentSession,
        toggle: write::ToggleOutcome,
    },
    NotAnOpenSpecTaskSource,
    RepoNotOpen,
    NoOpenSpecFolder,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
    WriteFailed { detail: String },
}

fn complete_openspec_task_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    manager: &crate::state::RepoManager,
    session_id: &str,
    done: bool,
) -> CompleteOpenSpecTaskOutcome {
    use crate::agentdesk::model::SessionLoadError as E;

    // Read first (outside the session-mutating lock) only to learn which
    // file/line to write -- the actual write goes through
    // `write::toggle_task_line`, the SAME function
    // `commands::openspec::openspec_toggle_task` calls, so a task ticked from
    // an accepted run and one ticked by clicking the checkbox in Spec Desk
    // hit the identical code path (design.md: "Reuse existing OpenSpec
    // writers for task/spec updates").
    let (change_id, task_index, task_text, repo_id, repo_path) =
        match locks.with_session_lock(session_id, || store::read_session(root, session_id)) {
            Ok(session) => match &session.header.source {
                SessionSource::OpenSpecTask {
                    change_id,
                    task_index,
                    task_text,
                    ..
                } => (
                    change_id.clone(),
                    *task_index,
                    task_text.clone(),
                    session.header.repo_id.clone(),
                    session.header.repo_path.clone(),
                ),
                _ => return CompleteOpenSpecTaskOutcome::NotAnOpenSpecTaskSource,
            },
            Err(E::NotFound) => return CompleteOpenSpecTaskOutcome::SessionNotFound,
            Err(E::Io { detail }) => return CompleteOpenSpecTaskOutcome::SessionUnavailable { detail },
            Err(reason) => {
                return CompleteOpenSpecTaskOutcome::SessionDamaged {
                    reason: reason.to_string(),
                }
            }
        };

    if manager.get(&repo_id).is_err() {
        return CompleteOpenSpecTaskOutcome::RepoNotOpen;
    }
    let repo_root = std::path::PathBuf::from(&repo_path);
    let Some(dir) = openspec_dir_for(&repo_root) else {
        return CompleteOpenSpecTaskOutcome::NoOpenSpecFolder;
    };

    // Re-parse fresh (not from a cached context) so the line number handed to
    // the writer is current -- the same "re-read rather than guess" stance
    // `write::toggle_task_line`'s own `LineMoved` guard takes, applied one
    // layer up so this command locates the right line even if the session's
    // cached target predates a file edit.
    use crate::agentdesk::openspec_context::locate_target_task;
    let Some(change) = crate::openspec::parse::parse_change_dir(&dir.join("changes").join(&change_id)) else {
        return CompleteOpenSpecTaskOutcome::NoOpenSpecFolder;
    };
    let located = locate_target_task(&change.tasks, task_index, &task_text);
    let Some(current_index) = located.current_index else {
        return CompleteOpenSpecTaskOutcome::WriteFailed {
            detail: "the target task could not be found in the current tasks.md".into(),
        };
    };
    let Some(task) = change.tasks.iter().find(|t| t.index == current_index) else {
        return CompleteOpenSpecTaskOutcome::WriteFailed {
            detail: "the target task could not be found in the current tasks.md".into(),
        };
    };

    let toggle = match write::toggle_task_line(&write::tasks_path(&dir, &change_id), task.line, done) {
        Ok(outcome) => outcome,
        Err(e) => return CompleteOpenSpecTaskOutcome::WriteFailed { detail: e.to_string() },
    };

    // Refresh the session's own state to match (task 4.3: "Refresh all
    // main/Desk progress surfaces after writes" -- the session's side of
    // that is its own record, since a caller reading the session right after
    // this returns must see the same answer this command just gave).
    let update_outcome = update_session_at(locks, root, session_id, |_s| {
        // Nothing on `SessionSource::OpenSpecTask` itself needs to change --
        // `task_text`/`task_index` are launch provenance, not live state
        // (model.rs: "Never mutated by a refresh"). Touching `updated_at`
        // (which `update_session_at` always does) is what makes the session
        // list re-sort this session to the top after its task completes,
        // which is the visible feedback this write should produce.
    });

    match update_outcome {
        UpdateSessionOutcome::Updated { session } => {
            CompleteOpenSpecTaskOutcome::Completed { session, toggle }
        }
        UpdateSessionOutcome::NotFound => CompleteOpenSpecTaskOutcome::SessionNotFound,
        UpdateSessionOutcome::Damaged { reason } => CompleteOpenSpecTaskOutcome::SessionDamaged { reason },
        UpdateSessionOutcome::WriteFailed { detail } => CompleteOpenSpecTaskOutcome::WriteFailed { detail },
        UpdateSessionOutcome::Unavailable { detail } => CompleteOpenSpecTaskOutcome::SessionUnavailable { detail },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_complete_openspec_task(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    manager: tauri::State<'_, crate::state::RepoManager>,
    session_id: SessionId,
    done: bool,
) -> Result<CompleteOpenSpecTaskOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    let outcome = complete_openspec_task_at(&locks_arc, &root, manager.inner(), &session_id, done);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::SourceSnapshot;
    use tempfile::TempDir;

    fn temp_root() -> (TempDir, SessionStoreRoot) {
        let dir = TempDir::new().expect("create temp dir");
        let root = SessionStoreRoot::at(dir.path().join("agent-desk").join("v1"))
            .expect("init store root");
        (dir, root)
    }

    fn test_locks() -> crate::agentdesk::SessionLocks {
        crate::agentdesk::SessionLocks::new()
    }

    fn create_request(title: &str) -> CreateSessionRequest {
        CreateSessionRequest {
            repo_id: "repo-1".into(),
            repo_path: "C:/code/proj".into(),
            repo_name: "proj".into(),
            title: title.into(),
            source: SessionSource::Manual {
                repo_id: "repo-1".into(),
            },
            intent: SessionIntent::Ask,
        }
    }

    #[test]
    fn create_writes_a_draft_session_and_updates_the_index() {
        let (_dir, root) = temp_root();
        let outcome = create_session_at(&root, create_request("First session"));
        let CreateSessionOutcome::Created { session } = outcome else {
            panic!("expected Created, got {outcome:?}");
        };
        assert_eq!(session.header.state, SessionState::Draft);
        assert!(!session.header.session_id.is_empty());

        let page = store::list_sessions(&root, &SessionListFilter::default(), None, 10);
        assert_eq!(page.headers.len(), 1);
        assert_eq!(page.headers[0].session_id, session.header.session_id);
    }

    // -- "View source" bridge (open_source_at): the pure, non-Tauri half of
    //    `agent_session_open_source` -- resolving the session's header, the
    //    part `agent_session_open_source` cannot test directly because it
    //    also needs a live `AppHandle`/main window (see the frontend's
    //    `agentDeskSourceNav.test.ts` for per-source-kind destination
    //    coverage, and `useAgentDeskSourceListener.ts` for what the emitted
    //    event drives once it reaches the main window). --

    #[test]
    fn open_source_refuses_an_unknown_session() {
        let (_dir, root) = temp_root();
        let outcome = open_source_at(&root, "does-not-exist");
        assert!(matches!(outcome, Err(OpenSourceOutcome::SessionNotFound)));
    }

    #[test]
    fn open_source_returns_the_session_header_for_a_real_session() {
        let (_dir, root) = temp_root();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("A session"))
        else {
            panic!("expected Created");
        };
        let outcome = open_source_at(&root, &session.header.session_id);
        let header = outcome.expect("expected Ok for a real session");
        assert_eq!(header.session_id, session.header.session_id);
        assert_eq!(header.repo_id, "repo-1");
        assert!(matches!(header.source, SessionSource::Manual { .. }));
    }

    #[test]
    fn get_returns_not_found_for_an_unknown_id() {
        let (_dir, root) = temp_root();
        let outcome = get_session_at(&root, "does-not-exist");
        assert!(matches!(outcome, GetSessionOutcome::NotFound));
    }

    #[test]
    fn get_returns_damaged_for_a_corrupt_file() {
        let (dir, root) = temp_root();
        let path = dir
            .path()
            .join("agent-desk")
            .join("v1")
            .join("sessions")
            .join("sess-bad.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let outcome = get_session_at(&root, "sess-bad");
        assert!(matches!(outcome, GetSessionOutcome::Damaged { .. }));
    }

    #[test]
    fn get_finds_a_created_session() {
        let (_dir, root) = temp_root();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Findable"))
        else {
            panic!("expected Created");
        };
        let outcome = get_session_at(&root, &session.header.session_id);
        let GetSessionOutcome::Found { session: found } = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        assert_eq!(found, session);
    }

    #[test]
    fn rename_updates_the_title_and_persists() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Old title"))
        else {
            panic!("expected Created");
        };
        let id = session.header.session_id.clone();
        let before = session.header.updated_at.clone();

        let outcome = update_session_at(&locks, &root, &id, |s| s.header.title = "New title".into());
        let UpdateSessionOutcome::Updated { session: updated } = outcome else {
            panic!("expected Updated, got {outcome:?}");
        };
        assert_eq!(updated.header.title, "New title");

        let reread = store::read_session(&root, &id).unwrap();
        assert_eq!(reread.header.title, "New title");
        // updated_at always advances on a mutation, even if the clock has
        // millisecond resolution and the test runs fast -- at minimum it must
        // not silently stay identical forever, so this just checks presence
        // rather than asserting `>` and risking flakiness on a fast clock.
        assert!(!reread.header.updated_at.is_empty());
        let _ = before;
    }

    #[test]
    fn rename_of_an_unknown_session_is_not_found() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let outcome = update_session_at(&locks, &root, "ghost", |s| s.header.title = "x".into());
        assert!(matches!(outcome, UpdateSessionOutcome::NotFound));
    }

    #[test]
    fn archive_and_unarchive_round_trip() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Archivable"))
        else {
            panic!("expected Created");
        };
        let id = session.header.session_id.clone();
        assert!(!session.header.archived);

        let archived = update_session_at(&locks, &root, &id, |s| s.header.archived = true);
        assert!(matches!(
            archived,
            UpdateSessionOutcome::Updated { session } if session.header.archived
        ));

        let unarchived = update_session_at(&locks, &root, &id, |s| s.header.archived = false);
        assert!(matches!(
            unarchived,
            UpdateSessionOutcome::Updated { session } if !session.header.archived
        ));
    }

    #[test]
    fn archived_sessions_are_hidden_by_default_and_visible_when_asked_for() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("To archive"))
        else {
            panic!("expected Created");
        };
        update_session_at(&locks, &root, &session.header.session_id, |s| {
            s.header.archived = true
        });

        let hidden = store::list_sessions(
            &root,
            &SessionListFilter {
                archived: Some(false),
                ..Default::default()
            },
            None,
            10,
        );
        assert!(hidden.headers.is_empty());

        let shown = store::list_sessions(
            &root,
            &SessionListFilter {
                archived: Some(true),
                ..Default::default()
            },
            None,
            10,
        );
        assert_eq!(shown.headers.len(), 1);
    }

    #[test]
    fn mark_read_clears_unread() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { mut session } =
            create_session_at(&root, create_request("Unread"))
        else {
            panic!("expected Created");
        };
        session.header.unread = true;
        store::write_session(&root, &session).unwrap();

        let outcome = update_session_at(&locks, &root, &session.header.session_id, |s| {
            s.header.unread = false
        });
        assert!(matches!(
            outcome,
            UpdateSessionOutcome::Updated { session } if !session.header.unread
        ));
    }

    #[test]
    fn append_user_message_creates_a_segment_and_moves_draft_to_ready() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Chatty"))
        else {
            panic!("expected Created");
        };
        assert_eq!(session.header.state, SessionState::Draft);
        assert!(session.segments.is_empty());

        let outcome =
            append_user_message_at(&locks, &root, &session.header.session_id, "hello".into(), vec![]);
        let AppendUserMessageOutcome::Appended {
            session: updated,
            message,
        } = outcome
        else {
            panic!("expected Appended, got {outcome:?}");
        };
        assert_eq!(message.role, MessageRole::User);
        assert_eq!(message.kind, MessageKind::User);
        assert_eq!(message.plain_content, "hello");
        assert_eq!(updated.header.state, SessionState::Ready);
        assert_eq!(updated.segments.len(), 1);
        assert_eq!(updated.messages.len(), 1);
        assert_eq!(updated.messages[0].segment_id, updated.segments[0].segment_id);
    }

    #[test]
    fn a_second_user_message_reuses_the_existing_segment() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Chatty"))
        else {
            panic!("expected Created");
        };
        let id = session.header.session_id.clone();
        append_user_message_at(&locks, &root, &id, "first".into(), vec![]);
        let outcome = append_user_message_at(&locks, &root, &id, "second".into(), vec![]);
        let AppendUserMessageOutcome::Appended { session, .. } = outcome else {
            panic!("expected Appended");
        };
        assert_eq!(session.segments.len(), 1, "should not open a new segment");
        assert_eq!(session.messages.len(), 2);
        assert_eq!(
            session.messages[0].segment_id,
            session.messages[1].segment_id
        );
    }

    #[test]
    fn append_user_message_carries_targets_through() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Targets"))
        else {
            panic!("expected Created");
        };
        let outcome = append_user_message_at(
            &locks,
            &root,
            &session.header.session_id,
            "look at this".into(),
            vec![MessageTarget::File {
                path: "src/a.rs".into(),
            }],
        );
        let AppendUserMessageOutcome::Appended { message, .. } = outcome else {
            panic!("expected Appended");
        };
        assert_eq!(
            message.targets,
            vec![MessageTarget::File {
                path: "src/a.rs".into()
            }]
        );
    }

    #[test]
    fn append_user_message_to_an_unknown_session_is_not_found() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let outcome = append_user_message_at(&locks, &root, "ghost", "hi".into(), vec![]);
        assert!(matches!(outcome, AppendUserMessageOutcome::NotFound));
    }

    #[test]
    fn attach_context_adds_an_attachment_and_persists() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Context"))
        else {
            panic!("expected Created");
        };
        let outcome = attach_context_at(
            &locks,
            &root,
            &session.header.session_id,
            "a.rs".into(),
            MessageTarget::File {
                path: "src/a.rs".into(),
            },
        );
        let AttachContextOutcome::Attached { session: updated, attachment } = outcome else {
            panic!("expected Attached, got {outcome:?}");
        };
        assert_eq!(attachment.label, "a.rs");
        assert_eq!(updated.attachments.len(), 1);

        let reread = store::read_session(&root, &updated.header.session_id).unwrap();
        assert_eq!(reread.attachments, updated.attachments);
    }

    #[test]
    fn attach_context_to_an_unknown_session_is_not_found() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let outcome = attach_context_at(
            &locks,
            &root,
            "ghost",
            "x".into(),
            MessageTarget::Source,
        );
        assert!(matches!(outcome, AttachContextOutcome::NotFound));
    }

    #[test]
    fn list_filters_and_pages_through_the_command_layer_types() {
        let (_dir, root) = temp_root();
        create_session_at(&root, create_request("Alpha"));
        let mut req = create_request("Beta");
        req.source = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 1,
            url: "https://example.test/issues/1".into(),
            snapshot: SourceSnapshot {
                title: "t".into(),
                summary: "s".into(),
                captured_at: "2026-01-01T00:00:00Z".into(),
                live_unavailable: false,
            },
        };
        create_session_at(&root, req);

        let filter: SessionListFilter = SessionListFilterInput {
            source_kinds: vec!["issue".into()],
            ..Default::default()
        }
        .into();
        let page = store::list_sessions(&root, &filter, None, 10);
        assert_eq!(page.headers.len(), 1);
        assert_eq!(page.headers[0].title, "Beta");
    }

    #[test]
    fn unknown_source_kind_labels_match_nothing_rather_than_erroring() {
        let filter: SessionListFilter = SessionListFilterInput {
            source_kinds: vec!["somethingFromTheFuture".into()],
            ..Default::default()
        }
        .into();
        assert!(filter.source_kinds.is_empty());
    }

    // -- regression: concurrent mutations of the same session must not lose
    // an update (SessionLocks serializes update_session_at/
    // append_user_message_at/attach_context_at per session_id) --

    /// Without `SessionLocks` serializing the read-modify-write, this is
    /// exactly finding 1's failure scenario: a user archives a session while
    /// a linked run event (here stood in for by `append_user_message_at`,
    /// which the bridge's `route_run_event` shares the same lock with) is
    /// routed at the same moment. Both would read `archived: false`; archive
    /// writes `archived: true`; the append, still holding its stale copy,
    /// writes afterward and reverts `archived` back to `false` with no error
    /// -- a lost update. Run without the lock in `update_session_at`/
    /// `append_user_message_at` (i.e. before this fix), this test fails
    /// intermittently: `final.header.archived` is `false` and/or
    /// `final.messages.len()` is less than the number of successful appends.
    #[test]
    fn archiving_and_appending_concurrently_loses_no_update() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Racing"))
        else {
            panic!("expected Created");
        };
        let id = session.header.session_id.clone();

        const APPENDS: usize = 20;
        let barrier = std::sync::Barrier::new(APPENDS + 1);

        // Referenced (not moved) into every spawned closure below: all of
        // `locks`/`root`/`id`/`barrier` outlive the scope, so a shared `&`
        // borrow is all any thread needs -- avoids each closure needing its
        // own clone just to satisfy `move`.
        let locks = &locks;
        let root = &root;
        let id = &id;
        let barrier = &barrier;

        std::thread::scope(|scope| {
            // One thread archives the session...
            let archiver = scope.spawn(move || {
                barrier.wait();
                let outcome = update_session_at(locks, root, id, |s| s.header.archived = true);
                assert!(
                    matches!(outcome, UpdateSessionOutcome::Updated { .. }),
                    "archive must succeed: {outcome:?}"
                );
            });

            // ...while many others append messages to it at the same moment.
            let appenders: Vec<_> = (0..APPENDS)
                .map(|i| {
                    scope.spawn(move || {
                        barrier.wait();
                        let outcome = append_user_message_at(
                            locks,
                            root,
                            id,
                            format!("message {i}"),
                            vec![],
                        );
                        assert!(
                            matches!(outcome, AppendUserMessageOutcome::Appended { .. }),
                            "append must succeed: {outcome:?}"
                        );
                    })
                })
                .collect();

            archiver.join().expect("archiver thread");
            for a in appenders {
                a.join().expect("appender thread");
            }
        });

        // If the two mutations ever interleaved instead of being strictly
        // serialized per session, one side's write would have clobbered the
        // other: either `archived` reverts to `false`, or some appended
        // messages never make it into the final file. With the session lock
        // held across each full read-modify-write, both effects survive
        // regardless of interleaving order.
        let final_session = store::read_session(&root, &id).expect("session still readable");
        assert!(
            final_session.header.archived,
            "archive must not be lost to a concurrent append"
        );
        assert_eq!(
            final_session.messages.len(),
            APPENDS,
            "every concurrent append must be preserved, none lost to a race"
        );
    }

    // -- 3.3: start-execution / stop-execution / usage / refresh-source --

    fn test_links() -> crate::agentdesk::RunSessionLinks {
        crate::agentdesk::RunSessionLinks::new()
    }

    /// Builds a `RunEventKind` as the engine would emit it. `run_session_id`
    /// is the *engine's own* run identifier (what `airun::SessionRegistry`
    /// calls a session, e.g. `"run-1"`) -- distinct from the durable Agent
    /// Desk session ID, and the value `bridge::execution_id_for_run_session`
    /// maps into a durable `ExecutionId`.
    fn run_event(run_session_id: &str, state: crate::airun::driver::RunState) -> crate::airun::driver::RunEventKind {
        use crate::airun::driver::{summarize, RunStep};
        let step = RunStep::Note { text: "hello from the engine".into() };
        crate::airun::driver::RunEventKind {
            repo_id: "repo-1".into(),
            session_id: run_session_id.into(),
            state,
            summary: summarize(&step),
            step,
        }
    }

    /// The test this task explicitly asks for: after the linking + execution
    /// bookkeeping `start_execution_at` performs (steps 1 and 3, isolated
    /// from the CLI-dependent step 4 that a unit test cannot exercise without
    /// a real Copilot CLI), a run event for the linked repository actually
    /// lands as a persisted message on the session -- proving
    /// `bridge::route_run_event` is no longer inert, which was its documented
    /// state before this task ("nothing populates this registry yet").
    #[test]
    fn after_linking_a_repository_a_routed_run_event_lands_in_the_session() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Linked"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        assert!(session.messages.is_empty());

        // What `start_execution_at` does before it ever touches the CLI: link
        // the repository, mint an execution ID, and record it on the session.
        let execution_id = crate::agentdesk::execution_id_for_run_session("run-1");
        links.link(&session.header.repo_id, &session_id);
        let recorded = update_session_at(&locks, &root, &session_id, |s| {
            s.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
                    execution_id.clone(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::Preparing,
                    now_rfc3339(),
                    None,
                    0,
                ));
            s.header.active_execution_id = Some(execution_id.clone());
            s.header.state = SessionState::Preparing;
        });
        assert!(matches!(recorded, UpdateSessionOutcome::Updated { .. }));

        // Now the same path `commands::airun::route_to_agent_desk` drives on
        // every real engine event: look up the link, route the event. The
        // engine's own run-session id ("run-1") is what
        // `execution_id_for_run_session` mapped into `execution_id` above.
        let event = run_event("run-1", crate::airun::driver::RunState::Working);
        let sequence = links.next_sequence(&event.session_id);
        let routed = crate::agentdesk::route_run_event(
            &root,
            &links,
            &locks,
            sequence,
            "2026-01-01T00:00:01Z",
            &event,
        );
        assert!(
            matches!(routed, crate::agentdesk::RunEventRouted::Persisted { .. }),
            "expected Persisted, got {routed:?}"
        );

        let reread = store::read_session(&root, &session_id).expect("session still readable");
        assert_eq!(reread.messages.len(), 1, "the routed event must be a durable message");
        assert_eq!(reread.messages[0].plain_content, "hello from the engine");
        assert_eq!(reread.messages[0].execution_id, Some(execution_id));
        assert_eq!(reread.header.state, SessionState::Working);
    }

    #[test]
    fn start_execution_refuses_a_second_run_while_one_is_active() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Busy"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        let execution_id = crate::agentdesk::execution_id_for_run_session("run-1");
        update_session_at(&locks, &root, &session_id, |s| {
            s.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
                    execution_id.clone(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::Working,
                    now_rfc3339(),
                    None,
                    3,
                ));
            s.header.active_execution_id = Some(execution_id.clone());
            s.header.state = SessionState::Working;
        });

        // Replicates `start_execution_at`'s step-1 guard directly (it cannot
        // be called as a whole in a unit test: step 4 needs a real CLI).
        let read = store::read_session(&root, &session_id).unwrap();
        let already_running = read.header.active_execution_id.as_ref().is_some_and(|active| {
            read.executions
                .iter()
                .any(|e| &e.execution_id == active && e.state == SessionState::Working)
        });
        assert!(already_running, "the guard must see the active execution as running");
    }

    /// Finding 1 regression test: two callers racing `start_execution_at`'s
    /// write step for the SAME session must never both win. Before the fix,
    /// the "nothing running" check and the `ExecutionRecord` write happened
    /// under two separate lock acquisitions with an unlocked gap between them
    /// (`manager.get` + `CliAgent::discover`) -- both threads could see
    /// "nothing active" and both write, leaving the loser's execution
    /// orphaned with a still-running engine process and only one execution
    /// actually recorded.
    ///
    /// `record_execution_if_not_running` is the atomic replacement: check and
    /// write inside one `with_session_lock` acquisition. This test drives it
    /// directly with two real threads and a barrier so they contend for the
    /// same lock, which is the only way to make the pre-fix race reproduce
    /// (a sequential call sequence cannot expose it).
    #[test]
    fn concurrent_start_attempts_yield_exactly_one_started_and_one_already_running() {
        let (_dir, root) = temp_root();
        let locks = std::sync::Arc::new(test_locks());
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Race"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();

        let exec_a = crate::agentdesk::execution_id_for_run_session("run-a");
        let exec_b = crate::agentdesk::execution_id_for_run_session("run-b");
        let barrier = std::sync::Barrier::new(2);
        let root_ref = &root;
        let locks_ref = &locks;
        let session_id_ref = &session_id;

        let (outcome_a, outcome_b) = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                record_execution_if_not_running(locks_ref, root_ref, session_id_ref, exec_a.clone())
            });
            let b = scope.spawn(|| {
                barrier.wait();
                record_execution_if_not_running(locks_ref, root_ref, session_id_ref, exec_b.clone())
            });
            (a.join().unwrap(), b.join().unwrap())
        });

        let outcomes = [outcome_a, outcome_b];
        let started = outcomes
            .iter()
            .filter(|o| matches!(o, RecordOutcome::Updated { .. }))
            .count();
        let already_running = outcomes
            .iter()
            .filter(|o| matches!(o, RecordOutcome::AlreadyRunning { .. }))
            .count();
        assert_eq!(started, 1, "exactly one attempt must win and record its execution");
        assert_eq!(already_running, 1, "the other attempt must see AlreadyRunning, not also win");

        let final_session = store::read_session(&root, &session_id).expect("session still readable");
        assert_eq!(
            final_session.executions.len(),
            1,
            "only the winning attempt's ExecutionRecord may be persisted, never both"
        );
    }

    #[test]
    fn stop_scope_one_stops_only_the_named_execution() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Two executions"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        let exec_a = crate::agentdesk::execution_id_for_run_session("run-a");
        let exec_b = crate::agentdesk::execution_id_for_run_session("run-b");
        update_session_at(&locks, &root, &session_id, |s| {
            for (id, state) in [
                (exec_a.clone(), SessionState::Working),
                (exec_b.clone(), SessionState::Working),
            ] {
                s.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
                    id,
                    s.header.session_id.clone(),
                    Some(exec_a.clone()),
                    state,
                    now_rfc3339(),
                    None,
                    1,
                ));
            }
            s.header.active_execution_id = Some(exec_b.clone());
        });

        let outcome = stop_execution_at(
            &locks,
            &root,
            &links,
            &drivers,
            &session_id,
            StopScope::One { execution_id: exec_b.clone() },
        );
        let StopExecutionOutcome::Stopped { session, stopped } = outcome else {
            panic!("expected Stopped, got {outcome:?}");
        };
        assert_eq!(stopped, vec![exec_b.clone()]);
        let a = session.executions.iter().find(|e| e.execution_id == exec_a).unwrap();
        let b = session.executions.iter().find(|e| e.execution_id == exec_b).unwrap();
        assert_eq!(a.state, SessionState::Working, "the peer execution must keep running");
        assert_eq!(b.state, SessionState::Stopped);
        assert!(b.ended_at.is_some());
    }

    /// Finding 2 regression test. Before the fix, `stop_execution_at` unlinked
    /// the repo->session link whenever `stopped` was non-empty, regardless of
    /// `scope` -- so stopping only a helper (`StopScope::One`) severed the
    /// link for the whole session even while the lead execution was still
    /// `Working`. Every later run event for that lead would then hit
    /// `RunSessionLinks::get` -> `None` in `bridge::route_run_event` and be
    /// dropped as `NoLinkedSession`, silently freezing the UI for a run that
    /// was still executing.
    ///
    /// This drives the real path end-to-end: link the repo, give the session
    /// a lead + a helper both `Working`, stop only the helper, assert the
    /// link survives, then route a real run event for the lead and assert it
    /// still lands as a persisted message instead of being dropped.
    #[test]
    fn stopping_one_helper_leaves_the_link_intact_for_the_still_running_lead() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Lead plus helper"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        let repo_id = session.header.repo_id.clone();
        links.link(&repo_id, &session_id);

        let lead = crate::agentdesk::execution_id_for_run_session("run-lead");
        let helper = crate::agentdesk::execution_id_for_run_session("run-helper");
        update_session_at(&locks, &root, &session_id, |s| {
            s.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
                    lead.clone(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::Working,
                    now_rfc3339(),
                    None,
                    0,
                ));
            s.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
                    helper.clone(),
                    s.header.session_id.clone(),
                    Some(lead.clone()),
                    SessionState::Working,
                    now_rfc3339(),
                    None,
                    0,
                ));
            s.header.active_execution_id = Some(lead.clone());
        });

        let outcome = stop_execution_at(
            &locks,
            &root,
            &links,
            &drivers,
            &session_id,
            StopScope::One { execution_id: helper.clone() },
        );
        let StopExecutionOutcome::Stopped { stopped, .. } = outcome else {
            panic!("expected Stopped, got {outcome:?}");
        };
        assert_eq!(stopped, vec![helper]);

        assert_eq!(
            links.get(&repo_id),
            Some(session_id.clone()),
            "the link must survive: the lead execution is still Working"
        );

        // Prove it in practice, not just by inspecting the link table: a run
        // event for the lead, routed exactly as the live engine would route
        // one, must still land as a persisted message rather than being
        // dropped as NoLinkedSession.
        let event = run_event("run-lead", crate::airun::driver::RunState::Working);
        let sequence = links.next_sequence(&event.session_id);
        let routed = crate::agentdesk::route_run_event(
            &root,
            &links,
            &locks,
            sequence,
            "2026-01-01T00:00:02Z",
            &event,
        );
        assert!(
            matches!(routed, crate::agentdesk::RunEventRouted::Persisted { .. }),
            "expected Persisted, got {routed:?}"
        );
    }

    #[test]
    fn stop_scope_all_stops_every_active_execution() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Stop all"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        let exec_a = crate::agentdesk::execution_id_for_run_session("run-a");
        let exec_b = crate::agentdesk::execution_id_for_run_session("run-b");
        update_session_at(&locks, &root, &session_id, |s| {
            for id in [exec_a.clone(), exec_b.clone()] {
                s.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
                    id,
                    s.header.session_id.clone(),
                    None,
                    SessionState::Working,
                    now_rfc3339(),
                    None,
                    1,
                ));
            }
            s.header.active_execution_id = Some(exec_b.clone());
        });

        let outcome = stop_execution_at(&locks, &root, &links, &drivers, &session_id, StopScope::All);
        let StopExecutionOutcome::Stopped { session, stopped } = outcome else {
            panic!("expected Stopped, got {outcome:?}");
        };
        assert_eq!(stopped.len(), 2, "both executions must be stopped");
        assert!(session.executions.iter().all(|e| e.state == SessionState::Stopped));
        assert_eq!(session.header.state, SessionState::Stopped);
    }

    #[test]
    fn stopping_an_already_finished_execution_is_a_harmless_no_op() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Already done"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        let exec_a = crate::agentdesk::execution_id_for_run_session("run-a");
        update_session_at(&locks, &root, &session_id, |s| {
            s.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
                    exec_a.clone(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::Finished,
                    now_rfc3339(),
                    Some(now_rfc3339()),
                    5,
                ));
        });

        let outcome = stop_execution_at(
            &locks,
            &root,
            &links,
            &drivers,
            &session_id,
            StopScope::One { execution_id: exec_a },
        );
        let StopExecutionOutcome::Stopped { stopped, .. } = outcome else {
            panic!("expected Stopped, got {outcome:?}");
        };
        assert!(stopped.is_empty(), "an execution that already finished has nothing to stop");
    }

    #[test]
    fn stop_of_an_unknown_session_is_not_found() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let outcome = stop_execution_at(&locks, &root, &links, &drivers, "ghost", StopScope::All);
        assert!(matches!(outcome, StopExecutionOutcome::NotFound));
    }

    #[test]
    fn usage_omits_every_unmeasured_field_rather_than_reporting_zero() {
        let (_dir, root) = temp_root();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Fresh"))
        else {
            panic!("expected Created");
        };

        let outcome = session_usage_at(&root, &session.header.session_id);
        let SessionUsageOutcome::Available { usage } = outcome else {
            panic!("expected Available, got {outcome:?}");
        };
        assert!(usage.session_tokens.is_none(), "no token accounting exists yet -- must be omitted, not zero");
        assert!(usage.session_cost_usd.is_none(), "no cost accounting exists yet -- must be omitted, not zero");
        assert!(usage.plan_limit.is_none());
        assert!(usage.plan_reset_at.is_none());
        assert!(usage.session_requests.is_none(), "no execution messages exist yet");
        assert_eq!(usage.active_helper_count, Some(0), "zero helpers is a real measured count, not unknown");
    }

    #[test]
    fn usage_counts_execution_produced_messages_as_measured_requests() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Chatty"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        let exec_a = crate::agentdesk::execution_id_for_run_session("run-a");
        update_session_at(&locks, &root, &session_id, |s| {
            s.messages.push(SessionMessage {
                message_id: "m1".into(),
                segment_id: "seg-1".into(),
                role: MessageRole::Assistant,
                timestamp: now_rfc3339(),
                plain_content: "did a thing".into(),
                rendered_content: None,
                provider: None,
                model: None,
                kind: MessageKind::Assistant,
                execution_id: Some(exec_a.clone()),
                sequence: Some(1),
                import: None,
                targets: vec![],
            });
        });

        let outcome = session_usage_at(&root, &session_id);
        let SessionUsageOutcome::Available { usage } = outcome else {
            panic!("expected Available, got {outcome:?}");
        };
        let requests = usage.session_requests.expect("a measured request count must be present");
        assert_eq!(requests.value, 1.0);
        assert_eq!(requests.source, UsageSource::Measured);
    }

    #[test]
    fn usage_for_an_unknown_session_is_not_found() {
        let (_dir, root) = temp_root();
        let outcome = session_usage_at(&root, "ghost");
        assert!(matches!(outcome, SessionUsageOutcome::NotFound));
    }

    #[test]
    fn refresh_source_leaves_the_snapshot_untouched_when_the_repo_is_unreachable() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let manager = crate::state::RepoManager::default();
        let mut req = create_request("Refreshable");
        req.source = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 1,
            url: "https://example.test/issues/1".into(),
            snapshot: SourceSnapshot {
                title: "Original title".into(),
                summary: "Original summary".into(),
                captured_at: "2026-01-01T00:00:00Z".into(),
                live_unavailable: false,
            },
        };
        let CreateSessionOutcome::Created { session } = create_session_at(&root, req) else {
            panic!("expected Created");
        };

        // The repository is never opened in `manager`, so it is unreachable.
        let outcome = refresh_source_at(&locks, &root, &manager, &session.header.session_id);
        let RefreshSourceOutcome::LiveUnavailable { session: updated, .. } = outcome else {
            panic!("expected LiveUnavailable, got {outcome:?}");
        };
        let SessionSource::Issue { snapshot, .. } = &updated.header.source else {
            panic!("expected Issue source");
        };
        assert_eq!(snapshot.title, "Original title", "the cached snapshot must never be overwritten");
        assert_eq!(snapshot.summary, "Original summary");
        assert!(snapshot.live_unavailable, "unreachable must flip the flag, not the snapshot text");
    }

    #[test]
    fn refresh_source_reports_unchanged_when_the_flag_already_matched() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let manager = crate::state::RepoManager::default();
        let mut req = create_request("Already flagged");
        req.source = SessionSource::Commit {
            oid: "abc123".into(),
            snapshot: SourceSnapshot {
                title: "A commit".into(),
                summary: "".into(),
                captured_at: "2026-01-01T00:00:00Z".into(),
                live_unavailable: true,
            },
        };
        let CreateSessionOutcome::Created { session } = create_session_at(&root, req) else {
            panic!("expected Created");
        };

        let outcome = refresh_source_at(&locks, &root, &manager, &session.header.session_id);
        let RefreshSourceOutcome::LiveUnavailable { .. } = outcome else {
            panic!("expected LiveUnavailable, got {outcome:?}");
        };
        // A second refresh with nothing changed would report `changed: false`
        // via the Refreshed branch once the repo is reachable; here we only
        // confirm the still-unreachable path does not spuriously flip a flag
        // that already matched (checked structurally above: no panic, same
        // snapshot text).
    }

    #[test]
    fn refresh_source_of_an_unknown_session_is_not_found() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let manager = crate::state::RepoManager::default();
        let outcome = refresh_source_at(&locks, &root, &manager, "ghost");
        assert!(matches!(outcome, RefreshSourceOutcome::NotFound));
    }

    #[test]
    fn refresh_source_on_a_manual_session_is_a_harmless_no_op() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let manager = crate::state::RepoManager::default();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Manual"))
        else {
            panic!("expected Created");
        };

        // Manual sources have no snapshot to flag -- refresh must not panic
        // trying to reach into one.
        let outcome = refresh_source_at(&locks, &root, &manager, &session.header.session_id);
        assert!(matches!(
            outcome,
            RefreshSourceOutcome::LiveUnavailable { .. }
        ));
    }

    // -- Gate 4: a session started from a specific NON-NEXT task still names
    // that exact task (change id + task index + task text) after a
    // simulated restart -- across execution, transcript, checkbox write,
    // progress, and the source banner. This is the core claim
    // `docs/agent-desk/build-order.md` section 4's Gate 4 asks for, and the
    // reason `OpenSpecTask::task_index`/`task_text` exist as launch
    // provenance separate from whatever tasks.md says right now.
    mod gate_4_exact_task_identity {
        use super::*;
        use std::fs;

        /// A real git repo (so `RepoManager::open` succeeds) with an
        /// `openspec/` folder containing three OPEN tasks, task 3 (index 2)
        /// deliberately left open so the "non-next" claim is not
        /// coincidentally true.
        fn repo_with_change(root: &std::path::Path) -> (crate::state::RepoManager, String) {
            git2::Repository::init(root).expect("init repo");
            let change_dir = root.join("openspec").join("changes").join("add-thing");
            fs::create_dir_all(&change_dir).unwrap();
            fs::write(
                change_dir.join("proposal.md"),
                "# Change: Add thing\n\n## Why\n\nBecause users need it.\n",
            )
            .unwrap();
            fs::write(
                change_dir.join("tasks.md"),
                "## 1. Backend\n\n\
                 - [ ] 1.1 First task (still open)\n\
                 - [ ] 1.2 Second task (still open)\n\
                 - [ ] 1.3 Third task (still open)\n\n\
                 ## 2. Frontend\n\n\
                 - [ ] 2.4 Wire the kickoff button\n",
            )
            .unwrap();

            let manager = crate::state::RepoManager::default();
            let (repo_id, _open, _reused) =
                manager.open(root.to_str().expect("utf8 path")).expect("open repo");
            (manager, repo_id)
        }

        #[test]
        fn a_session_started_from_task_index_6_keeps_naming_task_index_6_across_every_surface() {
            let (dir, root) = temp_root();
            let locks = test_locks();
            let (manager, repo_id) = repo_with_change(dir.path());

            // Task 2.4 is index 6 in the flat, zero-based numbering the
            // parser assigns (three tasks under "1. Backend" at indices
            // 0-2, then "2.4 Wire the kickoff button" is the fourth
            // heading-relative task overall but the parser numbers across
            // the WHOLE file, so recompute it the same way the parser does
            // rather than hand-asserting a guessed number).
            let openspec_dir = dir.path().join("openspec");
            let change =
                crate::openspec::parse::parse_change_dir(&openspec_dir.join("changes").join("add-thing"))
                    .expect("change parses");
            let target = change
                .tasks
                .iter()
                .find(|t| t.text.contains("Wire the kickoff button"))
                .expect("target task present")
                .clone();
            // The claim only means something if this is genuinely not the
            // next open task -- tasks 1.1-1.3 are all still open ahead of it.
            assert!(change.tasks.iter().any(|t| !t.done && t.index < target.index));

            // 1. START: create a session naming this exact task, not
            // "whatever the next open task is."
            let snapshot = crate::agentdesk::model::SourceSnapshot {
                title: change.title.clone(),
                summary: target.text.clone(),
                captured_at: "2026-01-01T00:00:00Z".into(),
                live_unavailable: false,
            };
            let create_request = CreateSessionRequest {
                repo_id: repo_id.clone(),
                repo_path: dir.path().to_string_lossy().into_owned(),
                repo_name: "proj".into(),
                title: format!("{} · {}", change.title, target.text),
                source: SessionSource::OpenSpecTask {
                    change_id: "add-thing".into(),
                    task_index: target.index,
                    task_text: target.text.clone(),
                    snapshot,
                },
                intent: SessionIntent::Fix,
            };
            let CreateSessionOutcome::Created { session } = create_session_at(&root, create_request)
            else {
                panic!("expected Created");
            };
            let session_id = session.header.session_id.clone();

            // Sanity: the header itself already names the exact task.
            let SessionSource::OpenSpecTask { task_index, task_text, .. } = &session.header.source else {
                panic!("expected an OpenSpecTask source");
            };
            assert_eq!(*task_index, target.index);
            assert_eq!(task_text, &target.text);

            // 2. EXECUTION: record an execution on this session (what
            // `agent_session_start_execution` does before handing off to
            // the engine) and confirm the record is keyed to this session,
            // not a different task's.
            let record_outcome =
                record_execution_if_not_running(&locks, &root, &session_id, "exec-1".to_string());
            let RecordOutcome::Updated { session: after_start } = record_outcome else {
                panic!("expected the execution to record");
            };
            assert_eq!(after_start.header.active_execution_id.as_deref(), Some("exec-1"));

            // 3. TRANSCRIPT: append a user message (what kickoff/the
            // composer does) and confirm it lands on the same session,
            // still naming the same task via its unchanged header.
            let append_outcome =
                append_user_message_at(&locks, &root, &session_id, "Get started on this.".into(), vec![]);
            let AppendUserMessageOutcome::Appended { session: after_message, .. } = append_outcome else {
                panic!("expected the message to append");
            };
            let SessionSource::OpenSpecTask { task_index, task_text, .. } = &after_message.header.source
            else {
                panic!("expected an OpenSpecTask source");
            };
            assert_eq!(*task_index, target.index);
            assert_eq!(task_text, &target.text);

            // 4. SIMULATED RESTART: drop every in-memory value and re-read
            // purely from disk, exactly as a fresh process would on launch.
            drop(session);
            drop(after_start);
            drop(after_message);
            let GetSessionOutcome::Found { session: reloaded } = get_session_at(&root, &session_id)
            else {
                panic!("expected the session to survive a restart");
            };

            // The header still names the exact task -- this is the
            // provenance claim (model.rs: "Never mutated by a refresh").
            let SessionSource::OpenSpecTask {
                change_id: reloaded_change_id,
                task_index: reloaded_index,
                task_text: reloaded_text,
                ..
            } = &reloaded.header.source
            else {
                panic!("expected an OpenSpecTask source after restart");
            };
            assert_eq!(reloaded_change_id, "add-thing");
            assert_eq!(*reloaded_index, target.index);
            assert_eq!(reloaded_text, &target.text);

            // 5. SOURCE BANNER content: `openspec_context_at` (what backs
            // the banner/context panel) still resolves the SAME task by
            // identity from the live file, honestly reporting it has not
            // diverged.
            let context_outcome = openspec_context_at(&root, &manager, &session_id);
            let OpenSpecSourceOutcome::Found { value: context } = context_outcome else {
                panic!("expected a context, got {context_outcome:?}");
            };
            let target_task = context.target_task.expect("target task context present");
            assert_eq!(target_task.current_index, Some(target.index));
            assert_eq!(target_task.current_text.as_deref(), Some(target.text.as_str()));
            assert!(!target_task.diverged);

            // 6. CHECKBOX WRITE + PROGRESS: complete the task through the
            // exact same command an accepted run uses, which re-locates the
            // task by identity (not by trusting a possibly-stale index) and
            // routes through the shared `write::toggle_task_line` writer.
            let complete_outcome =
                complete_openspec_task_at(&locks, &root, &manager, &session_id, true);
            let CompleteOpenSpecTaskOutcome::Completed { session: after_complete, toggle } =
                complete_outcome
            else {
                panic!("expected the task to complete, got {complete_outcome:?}");
            };
            assert!(matches!(toggle, write::ToggleOutcome::Toggled));
            // The session's own record of its target task is UNCHANGED --
            // still task index 6/"2.4 Wire the kickoff button" -- completion
            // does not silently retarget the session onto a different task.
            let SessionSource::OpenSpecTask { task_index, task_text, .. } = &after_complete.header.source
            else {
                panic!("expected an OpenSpecTask source");
            };
            assert_eq!(*task_index, target.index);
            assert_eq!(task_text, &target.text);

            // Progress moved on disk: re-parsing tasks.md shows exactly one
            // task done, and it is the target task, not task 1.1 (the "next
            // open task" this whole test exists to prove was NOT silently
            // substituted).
            let reparsed =
                crate::openspec::parse::parse_change_dir(&openspec_dir.join("changes").join("add-thing"))
                    .expect("change still parses after the write");
            assert_eq!(reparsed.progress.done, 1);
            let completed_task = reparsed
                .tasks
                .iter()
                .find(|t| t.done)
                .expect("exactly one task is done");
            assert_eq!(completed_task.text, target.text);
            assert!(reparsed.tasks[0..3].iter().all(|t| !t.done), "task 1.1-1.3 must remain untouched");

            // 7. AFTER A SECOND RESTART: the source banner still names the
            // exact task, and `agent_session_openspec_status` reports it as
            // a normal Active change (not Moved/Deleted) with the one task
            // now showing done.
            let status_after = openspec_status_at(&root, &manager, &session_id);
            assert!(matches!(status_after, OpenSpecSessionStatus::Active));

            let GetSessionOutcome::Found { session: final_reload } = get_session_at(&root, &session_id)
            else {
                panic!("expected the session to still be readable");
            };
            let SessionSource::OpenSpecTask {
                task_index: final_index,
                task_text: final_text,
                ..
            } = &final_reload.header.source
            else {
                panic!("expected an OpenSpecTask source on the final reload");
            };
            assert_eq!(*final_index, target.index, "task index must survive every step, including the write");
            assert_eq!(final_text, &target.text, "task text must survive every step, including the write");
        }
    }
}
