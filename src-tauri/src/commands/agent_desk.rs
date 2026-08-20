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

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::agentdesk::model::{
    AgentSession, AgentSessionHeader, ContextAttachment, MessageId, MessageKind, MessageRole,
    MessageTarget, SegmentId, SessionId, SessionIntent, SessionMessage, SessionSource,
    SessionState, CURRENT_SCHEMA_VERSION,
};
use crate::agentdesk::store::{
    self, SessionListFilter, SessionListPage, SessionStoreRoot, StoreInitError, WriteError,
};
use crate::error::AppError;

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
}
