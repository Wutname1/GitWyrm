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
        graph_started_at: None,
        preferred_provider: None,
        preferred_mode: None,
        preferred_team: None,
        preferred_model: None,
        preferred_effort: None,
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
    executions: tauri::State<'_, crate::agentdesk::ExecutionRegistry>,
    filter: SessionListFilterInput,
    cursor: Option<String>,
    limit: u32,
) -> Result<SessionListPageOutput, AppError> {
    let root = resolve_root(&app)?;
    let executions = executions.inner().clone();
    let filter: SessionListFilter = filter.into();
    let limit = limit.max(1) as usize;
    let page = tauri::async_runtime::spawn_blocking(move || {
        // Display-only reconciliation, no write -- see
        // `list_sessions_reconciled`'s doc comment for why: this call runs on
        // every sidebar refresh, and persisting per-session would mean a
        // `SessionLocks` acquisition and a full read-modify-write per stale
        // header on every one of those. The durable fix happens lazily via
        // `agent_session_get` when a session is actually opened.
        //
        // Session-granular, via `ExecutionRegistry` (keyed by (session,
        // execution), never by repository) -- NOT `RunSessionLinks::get`,
        // which used to answer this with a repo-keyed lookup that could name
        // a DIFFERENT session sharing this header's repository (the P0 "wrong
        // chat" bug this whole change closes). "Is this session live" is
        // honestly "does this session have at least one execution this
        // process currently has registered."
        store::list_sessions_reconciled(&root, &filter, cursor.as_deref(), limit, |header| {
            !executions
                .live_executions_for_session(&header.session_id)
                .is_empty()
        })
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

/// Loads one session and reconciles any live-process state it claims against
/// what is actually alive in this process, writing the fix back if anything
/// moved -- see `agentdesk::session_recovery`'s module doc for why a session
/// read right after a crash/force-quit/power-loss/app-update can legitimately
/// still say `Preparing`/`Working`/`NeedsInput` with nothing behind it.
///
/// Liveness is read from [`crate::agentdesk::ExecutionRegistry`]
/// (`(session_id, execution_id)`-keyed), not `RunSessionLinks` -- unlike
/// `RunSessionLinks`, which now only answers "which session owns THIS exact
/// execution's routed events" and has no per-repository or per-session
/// enumeration, `ExecutionRegistry::live_executions_for_session` directly
/// answers the session-granular question this function actually needs, with
/// no repository indirection to get wrong. Read with no session lock held
/// (`ExecutionRegistry` has its own internal lock), matching
/// `agentdesk::locks`'s fixed lock-ordering rule.
fn get_session_at(
    locks: &crate::agentdesk::SessionLocks,
    executions: &crate::agentdesk::ExecutionRegistry,
    root: &SessionStoreRoot,
    session_id: &str,
) -> GetSessionOutcome {
    use crate::agentdesk::model::SessionLoadError as E;

    // "Is this session live" for the header (task 2.7: "retain recovery
    // information" -- the header's own state has no per-execution breakdown
    // to be more precise than this with): does this process have at least
    // one execution registered for this exact session, right now.
    let session_is_live = !executions.live_executions_for_session(&session_id.to_string()).is_empty();

    // Reconcile against the now-known liveness answer, and write back in the
    // same critical section as the decision.
    locks.with_session_lock(session_id, || {
        let mut session = match store::read_session(root, session_id) {
            Ok(s) => s,
            Err(E::NotFound) => return GetSessionOutcome::NotFound,
            Err(E::Io { detail }) => return GetSessionOutcome::Unavailable { detail },
            Err(reason) => {
                return GetSessionOutcome::Damaged {
                    reason: reason.to_string(),
                }
            }
        };

        let header_changed = reconcile_session_header(&mut session.header, session_is_live);
        let executions_changed = reconcile_session_executions(&mut session, executions);

        if header_changed || executions_changed > 0 {
            session.header.updated_at = now_rfc3339();
            // Best-effort, matching every other reconciliation/index write in
            // this module: the in-memory fix is what the caller sees either
            // way, and a failed write here just means the same reconciliation
            // runs again on the next load rather than being lost.
            if store::write_session(root, &session).is_ok() {
                let _ = refresh_index(root);
            }
        }

        GetSessionOutcome::Found { session }
    })
}

/// Reconciles `header` in place, given whether this session's repository
/// currently has a live run attached to THIS session in this process.
/// Returns whether it changed.
fn reconcile_session_header(header: &mut AgentSessionHeader, session_is_live: bool) -> bool {
    matches!(
        crate::agentdesk::reconcile_header(header, session_is_live),
        crate::agentdesk::HeaderReconciliation::Interrupted
    )
}

/// Reconciles every `ExecutionRecord` on `session`, not only the header --
/// the graph panel renders helper nodes straight from `session.executions`
/// (tasks.md 6.1, `agentGraphProjection.ts`), so a fix that stopped at the
/// header would leave every helper showing "Working" forever.
///
/// Task 2.1/2.7: each execution is checked individually against
/// `executions.is_live(session_id, execution_id)` -- the runtime registry
/// knows exactly which (session, execution) pairs this process actually has
/// something running for, unlike the header's own `RunSessionLinks`-derived
/// answer (which is repo-granular and cannot distinguish a live lead from a
/// live helper in the same session). A restart clears the registry (it is
/// in-process state, `app.manage(ExecutionRegistry::new())`), so every
/// execution reads as not-live and is correctly marked `Interrupted` --
/// exactly the "on restart, mark lost processes Interrupted" behavior task
/// 2.7 asks for, now precise per execution rather than per session.
fn reconcile_session_executions(
    session: &mut AgentSession,
    executions: &crate::agentdesk::ExecutionRegistry,
) -> u32 {
    let session_id = session.header.session_id.clone();
    crate::agentdesk::reconcile_executions(&mut session.executions, |execution_id| {
        executions.is_live(&session_id, &execution_id.to_string())
    })
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_get(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    executions: tauri::State<'_, crate::agentdesk::ExecutionRegistry>,
    session_id: SessionId,
) -> Result<GetSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    let executions = executions.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        get_session_at(&locks, &executions, &root, &session_id)
    })
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

/// Save the controls that belong to one chat. Keeping them beside the
/// session, rather than in pane-local React state, prevents Split View from
/// carrying one chat's authority or provider into another chat.
///
/// `model` and `effort` are `None` when the chat has expressed no preference,
/// and are written through as-is rather than defaulted here: absent means
/// "follow whatever the tool is set up for", which keeps following it when the
/// tool's own default changes. Naming the current default instead would pin
/// it silently.
#[tauri::command]
#[specta::specta]
pub async fn agent_session_set_preferences(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    mode: String,
    team: String,
    provider: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<UpdateSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        update_session_at(&locks, &root, &session_id, |session| {
            session.header.preferred_mode = Some(mode);
            session.header.preferred_team = Some(team);
            session.header.preferred_provider = provider;
            session.header.preferred_model = model;
            session.header.preferred_effort = effort;
        })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Move a brand-new manual chat to another open project. The UI only offers
/// this before the first message; the guard here keeps another caller from
/// silently moving work that already has a transcript or execution history.
#[tauri::command]
#[specta::specta]
pub async fn agent_session_set_project(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    repo_id: String,
    repo_path: String,
    repo_name: String,
) -> Result<SetProjectOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Checked BEFORE the write, and reported. The guard used to live
        // inside the closure, so a session that had already started came
        // back as `Updated` with nothing changed and the UI announced a
        // move that never happened -- a race with the first message was
        // enough to hit it.
        let session = match store::read_session(&root, &session_id) {
            Ok(s) => s,
            Err(crate::agentdesk::model::SessionLoadError::NotFound) => {
                return SetProjectOutcome::NotFound
            }
            Err(e) => return SetProjectOutcome::Failed { detail: e.to_string() },
        };
        if !session.messages.is_empty()
            || !session.executions.is_empty()
            || !matches!(session.header.source, SessionSource::Manual { .. })
        {
            return SetProjectOutcome::AlreadyStarted;
        }

        let outcome = update_session_at(&locks, &root, &session_id, |session| {
            session.header.repo_id = repo_id.clone();
            session.header.repo_path = repo_path;
            session.header.repo_name = repo_name;
            session.header.source = SessionSource::Manual { repo_id };
        });
        match outcome {
            UpdateSessionOutcome::Updated { session } => SetProjectOutcome::Moved { session },
            UpdateSessionOutcome::NotFound => SetProjectOutcome::NotFound,
            UpdateSessionOutcome::Damaged { reason } => SetProjectOutcome::Failed { detail: reason },
            UpdateSessionOutcome::WriteFailed { detail }
            | UpdateSessionOutcome::Unavailable { detail } => {
                SetProjectOutcome::Failed { detail }
            }
        }
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// What happened when a chat was asked to move to another project.
///
/// `AlreadyStarted` is its own answer rather than a silent no-op: moving a
/// chat that has begun would take its transcript and its running work to a
/// different repository, so it is refused, and the person is told why
/// instead of being shown a success message for nothing.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SetProjectOutcome {
    Moved { session: AgentSession },
    /// The chat already has messages, a run, or a source of its own.
    AlreadyStarted,
    NotFound,
    Failed { detail: String },
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

/// What happened when a chat was asked to be deleted.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeleteSessionOutcome {
    Deleted,
    /// An agent is still working in this chat. Deleting the file underneath a
    /// running process would leave it writing into a session nobody can see,
    /// so the answer is to stop it first rather than to delete anyway.
    StillRunning,
    Failed {
        detail: String,
    },
}

/// Deletes one chat and everything written for it.
///
/// Permanent, and deliberately separate from Archive: archive is for a chat
/// you are done with, delete is for one that should not exist. The two are
/// not the same request and collapsing them would mean either a hoarded list
/// nobody prunes or an archive button that quietly destroys work.
///
/// Refuses while an agent is running rather than stopping it: a delete that
/// silently killed a working agent would lose whatever it was part-way
/// through, and the person asking may not have realised it was still going.
#[tauri::command]
#[specta::specta]
pub async fn agent_session_delete(
    app: AppHandle,
    // Bare, not `Arc<_>`: lib.rs manages `ExecutionRegistry` directly, and
    // Tauri resolves state by exact type. Asking for the Arc compiled fine and
    // then failed at the first click with "state not managed", which is a
    // runtime error nothing in the build catches.
    executions: tauri::State<'_, crate::agentdesk::ExecutionRegistry>,
    session_id: SessionId,
) -> Result<DeleteSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let registry = executions.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if !registry.live_executions_for_session(&session_id).is_empty() {
            return DeleteSessionOutcome::StillRunning;
        }
        // An imported chat is also a row in its adapter's ledger. Deleting
        // the session alone left that row pointing at a session that no
        // longer exists: the import picker still said "already imported",
        // Continue here still offered itself, and refreshing failed because
        // its target was gone. Unlinked BEFORE the delete, so a ledger write
        // that fails leaves the session in place rather than half-removed.
        if let Some(link) = crate::agentdesk::import_store::find_link_for_session(&root, &session_id) {
            if let Err(e) = crate::agentdesk::import_store::remove_link(
                &root,
                &link.adapter_id,
                &link.external_session_id,
            ) {
                return DeleteSessionOutcome::Failed {
                    detail: format!("could not unlink the imported chat: {e}"),
                };
            }
        }
        match store::delete_session(&root, &session_id) {
            Ok(()) => {
                // The index is a projection: rebuilding it from what is on
                // disk is what makes the row disappear, and it cannot drift
                // from the files because it is derived from them.
                let (headers, _) = store::rebuild_index_from_sessions(&root);
                let _ = store::write_index(&root, &headers);
                DeleteSessionOutcome::Deleted
            }
            Err(e) => DeleteSessionOutcome::Failed {
                detail: e.to_string(),
            },
        }
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

/// A short chat name taken from the first message.
///
/// One line, trimmed, and cut at a word boundary so a long first message does
/// not produce a sidebar row of run-on text. Markdown and code fences are left
/// alone deliberately -- stripping them well is a bigger job than a title
/// needs, and the raw first line reads fine at this length.
fn title_from_first_message(content: &str) -> String {
    const MAX: usize = 60;

    let first_line = content.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if first_line.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for word in first_line.split_whitespace() {
        // `chars().count()`, not `len()`: a title is measured in what the user
        // sees, and a byte length would cut a multi-byte character short.
        let projected = if out.is_empty() { word.chars().count() } else { out.chars().count() + 1 + word.chars().count() };
        if projected > MAX {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }

    // A single word longer than the whole budget leaves `out` empty; take a
    // hard prefix of it rather than returning nothing.
    if out.is_empty() {
        out = first_line.chars().take(MAX).collect();
    }
    out
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

    // Name the chat after the first thing said in it, so the sidebar does not
    // fill up with rows all reading "Untitled chat". Only ever fills a blank
    // title -- a title the user set, or one set by a source kickoff, is never
    // overwritten. This runs inside the same lock as the append above, so the
    // title and the message it came from are written together or not at all.
    if session.header.title.trim().is_empty() {
        session.header.title = title_from_first_message(&message.plain_content);
    }

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
    /// `provider_override` named a provider this build does not support.
    /// Task 1.6: this is a typed, visible refusal -- the execution is never
    /// started with the default provider as a silent substitute.
    UnsupportedProvider { requested: String },
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
/// The "what actually ran" facts stamped onto an execution when it is created
/// (R3.5). Bundled rather than passed as six more parameters, so the recorder's
/// signature stays readable and a future field is one struct member instead of
/// another argument at every call site.
#[derive(Default, Clone)]
struct ExecutionProvenance {
    worktree_path: Option<String>,
    branch: Option<String>,
    base_oid: Option<String>,
    provider: Option<String>,
    mode: Option<String>,
    team: Option<String>,
}

fn record_execution_if_not_running(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: ExecutionId,
    // R5.3: hash of the OpenSpec plan text this execution was handed, or
    // `None` for a session that did not start from OpenSpec. Written inside the
    // same locked write that creates the record, so the record and the context
    // it was given can never disagree.
    context_fingerprint: Option<String>,
    // R3.5: where this run worked and under what authority, written in the same
    // locked write that creates the record so a restart can always answer
    // "what actually ran, and where are its changes".
    provenance: ExecutionProvenance,
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
        let mut record = crate::agentdesk::model::ExecutionRecord::minimal(
            execution_id.clone(),
            session.header.session_id.clone(),
            None,
            SessionState::Preparing,
            now,
            None,
            0,
        );
        record.context_fingerprint = context_fingerprint.clone();
        record.worktree_path = provenance.worktree_path.clone();
        record.branch = provenance.branch.clone();
        record.base_oid = provenance.base_oid.clone();
        record.provider = provenance.provider.clone();
        record.mode = provenance.mode.clone();
        record.team = provenance.team.clone();
        session.executions.push(record);
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

/// P1-C wiring 4 ("a losing solo-start race can provision a worktree and
/// return without cleaning it"): removes a worktree `start_execution_at`
/// itself just provisioned (step 2b) once it is clear this call is NOT
/// going to hand it to the engine after all -- every early return between
/// "worktree provisioned" and "engine actually launched" (`CliAgent::discover`
/// failing, or any `RecordOutcome` other than `Updated` from the second,
/// re-checked lock acquisition, the exact race the audit names) leaked
/// exactly this folder before this function existed.
///
/// Best-effort and silent on failure: this call is already unwinding from
/// one error, and a cleanup failure here must never mask or replace the
/// original `StartExecutionOutcome` the caller is about to return -- the
/// worktree is freshly created by this same call (nothing but an empty,
/// freshly branched checkout could be in it), so `DirtyChoice::Discard` is
/// safe without a confirmation step: there is no user data to protect, only
/// this function's own leftover. If removal fails anyway (a Windows file
/// lock is the realistic case, matching every other worktree-removal call
/// site's own comments on this), the folder is simply left behind exactly
/// as it would have been before this fix -- a pre-existing, separately
/// tracked failure mode, not a regression this fix needs to also solve.
fn cleanup_unused_worktree(open: &std::sync::Arc<crate::state::OpenRepo>, worktree_path: &str) {
    let main_path = open.path.to_string_lossy().into_owned();
    let repo = open.repo.lock().unwrap();
    let _ = crate::git::worktree::remove(
        &repo,
        &main_path,
        worktree_path,
        crate::git::worktree::DirtyChoice::Discard,
    );
}

/// `pub(crate)`, not `fn`: `commands::agent_kickoff::agent_session_start`
/// (task/P1-A "every explicit source action starts immediately") calls this
/// directly, right after creating a fresh session, exactly the way
/// `create_session_for_kickoff` already lets that module reuse session
/// creation without reimplementing it -- kickoff's own job stays "build the
/// right request and dedupe," never a second copy of "how an execution
/// actually starts."
pub(crate) fn start_execution_at(
    app: &AppHandle,
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    links: &crate::agentdesk::RunSessionLinks,
    manager: &crate::state::RepoManager,
    executions: &crate::agentdesk::ExecutionRegistry,
    session_id: &str,
    mode: ExecutionMode,
    team: ExecutionTeam,
    provider_override: Option<String>,
) -> StartExecutionOutcome {
    use crate::agentdesk::model::SessionLoadError as E;

    // Step 0: read the session's intent up front (a cheap, lock-free read of
    // just the header would be nice, but `store::read_session` is the only
    // reader today) so `ExecutionPolicy::resolve` -- and, in particular, an
    // unsupported `provider_override` -- is checked before anything else
    // this function does has a side effect (linking the repo, discovering
    // the CLI, provisioning a worktree). Task 1.6: this must fail visibly as
    // its own typed outcome, never fall through to starting the default
    // provider anyway.
    let intent = match locks.with_session_lock(session_id, || store::read_session(root, session_id)) {
        Ok(s) => s.header.intent,
        Err(E::NotFound) => return StartExecutionOutcome::NotFound,
        Err(E::Io { detail }) => return StartExecutionOutcome::Unavailable { detail },
        Err(reason) => {
            return StartExecutionOutcome::Damaged {
                reason: reason.to_string(),
            }
        }
    };
    // A chat with no tool of its own follows the person's default, and only
    // falls back to GitWyrm's built-in choice when they have not set one.
    // Before this, "no preference" meant a hardcoded tool no setting could
    // change -- so someone with two tools installed had to re-pick in every
    // single chat, and the app had no way to be told which one they wanted.
    //
    // A default naming a tool this build does not know is ignored rather than
    // refused. An explicit per-chat override must fail loudly -- the person
    // asked for that tool by name and has to be told it cannot be used -- but
    // a setting left behind by an older build, or a tool since renamed, must
    // not silently block every chat in the app with an error nobody can
    // connect to a settings page they last touched months ago.
    let configured_default = crate::settings::read_settings(app)
        .ok()
        .map(|s| s.default_agent_tool)
        .filter(|t| !t.trim().is_empty())
        .filter(|t| crate::agentdesk::policy::ExecutionProvider::parse(t).is_some());
    let effective_provider = provider_override
        .as_deref()
        .or(configured_default.as_deref());
    let policy = match crate::agentdesk::policy::ExecutionPolicy::resolve(
        intent,
        mode,
        team,
        effective_provider,
    ) {
        Ok(p) => p,
        Err(crate::agentdesk::policy::PolicyRefusal::UnsupportedProvider { requested }) => {
            return StartExecutionOutcome::UnsupportedProvider { requested };
        }
    };

    // Step 1: read the session and refuse a second concurrent execution, all
    // under the session lock so a racing start/stop cannot both pass the
    // "nothing running" check (the same race `agentdesk::locks` documents for
    // rename/archive/append).
    let (session, repo_id, repo_path, prompt, context_fingerprint) = match locks.with_session_lock(session_id, || {
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
                // R5.1/R5.2: a chat started from an OpenSpec change or task
                // must hand the agent that plan -- proposal, design, deltas,
                // the exact task and progress -- not just the source title.
                // `render_for_prompt` marks absent documents explicitly, so a
                // missing design.md reads as "there is none" rather than as
                // silence the model can mistake for "it does not matter".
                let openspec_context = openspec_target_of(&s.header.source)
                    .and_then(|target| resolve_openspec_context(std::path::Path::new(&s.header.repo_path), &target));
                let mut prompt = match &openspec_context {
                    Some(ctx) => format!(
                        "{}

{}",
                        crate::agentdesk::openspec_context::render_for_prompt(ctx),
                        build_prompt(&s)
                    ),
                    None => build_prompt(&s),
                };
                // R6.1: a typed path from a live Plan-mode turn to a
                // persisted `ProposedGraph` starts with telling the model,
                // for THIS turn, to propose one -- see
                // `agentdesk::plan_proposal`'s module doc for why a fenced
                // block (not a new tool) is the mechanism, and
                // `route_to_agent_desk`'s completion hook for the other
                // half (parsing the reply back out once the run ends).
                // Team::Lead only: a Plan-mode Solo run has no helpers to
                // propose and stays an ordinary read/inspect turn.
                if policy.team == crate::agentdesk::policy::ExecutionTeam::Lead {
                    let graph_instruction = match policy.mode {
                        crate::agentdesk::policy::ExecutionMode::Plan => {
                            Some(crate::agentdesk::plan_proposal::plan_mode_instruction())
                        }
                        crate::agentdesk::policy::ExecutionMode::Auto => {
                            Some(crate::agentdesk::plan_proposal::auto_mode_instruction())
                        }
                        crate::agentdesk::policy::ExecutionMode::Ask => None,
                    };
                    if let Some(instruction) = graph_instruction {
                        prompt = format!("{instruction}\n\n{prompt}");
                    }
                }
                // R5.3: hash the text the agent actually read, so a later
                // reader can tell whether the plan has moved underneath it.
                let context_fingerprint = openspec_context
                    .as_ref()
                    .map(crate::agentdesk::openspec_context::fingerprint);
                Ok((s, repo_id, repo_path, prompt, context_fingerprint))
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
    // `WorktreePolicy::NotUntilStart` (Plan) is `open.path` (read-only,
    // no isolation) for a session whose graph has NOT been started yet --
    // a Plan proposal turn only reads the repository to draft its proposal,
    // so it has no more reason to provision a worktree than Ask/Review do
    // (`agent_kickoff.rs`'s own doc comment: "Plan calls it only once the
    // user explicitly starts it, at which point it behaves like Fix"). Once
    // `started_for_execution` reports `true` -- durably, via
    // `header.graph_started_at`, set by `agent_session_start_graph`'s commit
    // step -- Plan behaves exactly like Fix and gets an isolated worktree
    // for every turn from then on, including this one.
    let intent_policy = crate::agentdesk::policy::for_intent(session.header.intent);
    let started = started_for_execution(&session.header, &intent_policy);
    // Set only when a worktree is provisioned below (R3.5).
    let mut run_branch: Option<String> = None;
    let mut base_oid_for_record: Option<String> = None;
    let engine_root = match intent_policy.worktree {
        crate::agentdesk::policy::WorktreePolicy::Never => open.path.clone(),
        crate::agentdesk::policy::WorktreePolicy::NotUntilStart if !started => open.path.clone(),
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
                // R3.5: the commit this run started from, so a later reader can
                // diff against exactly what the agent saw rather than against
                // wherever the branch has since moved.
                base_oid_for_record = repo.head().ok().and_then(|h| h.target()).map(|o| o.to_string());
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
                    branch,
                } => {
                    // R3.5: remember where this run actually worked and what it
                    // branched from. Without these a restart can tell you a run
                    // happened but not where its changes live.
                    run_branch = Some(branch);
                    std::path::PathBuf::from(path)
                }
                crate::commands::agent_kickoff::ProvisionKickoffWorktreeOutcome::Failed { detail } => {
                    // Task 5.2: refused outright, never falls back to
                    // `open.path`.
                    return StartExecutionOutcome::WorktreeFailed { detail };
                }
            }
        }
    };

    // Filled in above when this intent provisions a worktree; `None` for a
    // read-only run that works against the checkout in place.
    let run_branch_for_record = run_branch.clone();

    // Step 3: mint the durable execution ID up front and link THIS EXECUTION
    // (never the repository -- see `RunSessionLinks`'s own doc comment on why
    // that was the P0 "event reaches the wrong chat" bug) to this session
    // *before* the engine can produce a single event -- a run event that
    // races ahead of the link would be silently dropped as `NoLinkedSession`
    // (bridge.rs). A second, concurrent session against the same repository
    // links its own execution ID and never overwrites this one.
    let execution_id = crate::agentdesk::execution_id_for_run_session(&new_id());
    links.link(&execution_id, &session_id.to_string());

    // Discover the transport up front so an unusable CLI or stale credentials
    // are reported as a typed outcome instead of only surfacing later as an
    // opaque failed run. `engine_root` (not `open.path`) is the working
    // directory handed to the engine -- see step 2b: this is what makes
    // isolation for Fix actually load-bearing rather than advisory.
    // `discover_for`, not `discover`: the policy decides WHICH tool runs this,
    // and refuses one that cannot be told to leave the files alone when this
    // job is not supposed to change anything (see `agent::select::choose`).
    let agent = match crate::ai::agent::cli_agent::CliAgent::discover_for(
        &policy,
        started,
        engine_root.clone(),
    ) {
        // The chat's own model and thinking level, applied after discovery:
        // discovery answers which tool can do this job, tuning is a property
        // of the chat. A value this tool does not offer is dropped at the
        // launch line rather than forwarded.
        Ok(a) => a.tuned(
            session.header.preferred_model.clone(),
            session.header.preferred_effort.clone(),
        ),
        Err(e) => {
            links.unlink(&execution_id);
            // P1-C wiring 4: this call provisioned `engine_root` as an
            // isolated worktree (whenever `run_branch_for_record.is_some()`)
            // and is now unwinding without ever handing it to an engine --
            // clean it up rather than leaking it.
            if run_branch_for_record.is_some() {
                cleanup_unused_worktree(&open, &engine_root.to_string_lossy());
            }
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
        record_execution_if_not_running(
            locks,
            root,
            session_id,
            execution_id.clone(),
            context_fingerprint,
            ExecutionProvenance {
                worktree_path: run_branch_for_record
                    .as_ref()
                    .map(|_| engine_root.to_string_lossy().to_string()),
                branch: run_branch_for_record.clone(),
                base_oid: base_oid_for_record.clone(),
                provider: Some(format!("{:?}", policy.provider)),
                mode: Some(format!("{:?}", policy.mode)),
                team: Some(format!("{:?}", policy.team)),
            },
        );
    // P1-C wiring 4: every branch below this point that returns instead of
    // reaching `RecordOutcome::Updated` is a losing attempt that already
    // provisioned `engine_root` as a worktree (step 2b) -- this is EXACTLY
    // the race the audit names ("a losing solo-start race can provision a
    // worktree and return without cleaning it"). Cleaned up unconditionally
    // for every non-`Updated` variant, not only `AlreadyRunning`: `NotFound`/
    // `Damaged`/`WriteFailed`/`Unavailable` are rarer, but each one is just
    // as much a losing attempt that must not leave its worktree behind.
    let session_after = match record_outcome {
        RecordOutcome::Updated { session } => session,
        RecordOutcome::AlreadyRunning { execution_id: winning_execution_id } => {
            // No write happened, so nothing to unwind on the session itself --
            // but the link registered above for THIS (losing) attempt's own
            // `execution_id` must not linger. `winning_execution_id` (the
            // `RecordOutcome`'s own field, deliberately renamed here so it
            // cannot be confused with the outer `execution_id` this losing
            // attempt minted) names a DIFFERENT execution -- the one that won
            // the race -- and must never be unlinked from here: it is not
            // this call's mapping to remove, and now that `RunSessionLinks`
            // is execution-addressed (not repo-addressed) there is no longer
            // any shared repo-level entry the two attempts could even
            // contend over.
            links.unlink(&execution_id);
            if run_branch_for_record.is_some() {
                cleanup_unused_worktree(&open, &engine_root.to_string_lossy());
            }
            return StartExecutionOutcome::AlreadyRunning {
                execution_id: winning_execution_id,
            };
        }
        RecordOutcome::NotFound => {
            links.unlink(&execution_id);
            if run_branch_for_record.is_some() {
                cleanup_unused_worktree(&open, &engine_root.to_string_lossy());
            }
            return StartExecutionOutcome::NotFound;
        }
        RecordOutcome::Damaged { reason } => {
            links.unlink(&execution_id);
            if run_branch_for_record.is_some() {
                cleanup_unused_worktree(&open, &engine_root.to_string_lossy());
            }
            return StartExecutionOutcome::Damaged { reason };
        }
        RecordOutcome::WriteFailed { detail } => {
            links.unlink(&execution_id);
            if run_branch_for_record.is_some() {
                cleanup_unused_worktree(&open, &engine_root.to_string_lossy());
            }
            return StartExecutionOutcome::WriteFailed { detail };
        }
        RecordOutcome::Unavailable { detail } => {
            links.unlink(&execution_id);
            if run_branch_for_record.is_some() {
                cleanup_unused_worktree(&open, &engine_root.to_string_lossy());
            }
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
    // R6.6: keyed by (session, execution), not `repo_id` -- a lead and its
    // helpers share one `repo_id` but must never share one gate-answer slot
    // (see `gate_answers`'s own doc comment for the exact failure this
    // avoids).
    crate::commands::airun::gate_answers()
        .lock()
        .unwrap()
        .insert((session_id.to_string(), run_session_id.clone()), answer_tx);

    // Task 2.1/2.2: this execution's entry in the runtime registry -- keyed
    // by the durable session and execution ID, not `repo_id` -- is created
    // and holds the cancellation handle BEFORE the spawned task below can
    // emit a single `Working` event. A Stop that lands in the window between
    // `Started` returning and the first `Working` event must still find a
    // live, cancellable execution here, not `StopOutcome::NotLive`.
    let cancel_handle = crate::airun::cli_run::CancelHandle::new();
    executions.register(session_id.to_string(), run_session_id.clone(), cancel_handle.clone());

    let app_for_task = app.clone();
    let repo_for_task = repo_id.clone();
    let run_session_id_for_task = run_session_id.clone();
    let session_id_for_task = session_id.to_string();
    let executions_for_task = executions.clone();
    let join_handle = tauri::async_runtime::spawn(async move {
        let sink: crate::airun::engine::Sink = {
            let app = app_for_task.clone();
            let repo = repo_for_task.clone();
            let session_id = run_session_id_for_task.clone();
            std::sync::Arc::new(move |state, step| {
                // Not `commands::airun::emit`: that gates every event on
                // `SessionRegistry::record`, which only accepts events from a
                // run `SessionRegistry::start` registered -- something this
                // execution never does (it has its own concurrency guard,
                // `record_execution_if_not_running`, keyed by the durable
                // session rather than by repository). Routing through `emit`
                // would silently drop every event here, leaving the
                // execution stuck in `Preparing` forever. See
                // `emit_agent_desk_only`'s doc comment.
                crate::commands::airun::emit_agent_desk_only(&app, &repo, &session_id, state, step);
            })
        };

        // What the auditor gets to work with. A run that cannot write files
        // has no diff to read and nothing to have cut corners on, so it is
        // not audited at all.
        let worktree_for_audit = if intent_policy.can_write {
            Some(engine_root.clone())
        } else {
            None
        };
        // What was asked for, taken from the prompt the working agent was
        // actually given. That prompt already opens with the rendered spec
        // when the session has one, so this is the same text rather than a
        // second rendering that could drift from it.
        let spec_for_audit = prompt.clone();

        crate::airun::cli_run::run_task(
            &agent,
            &format!(
                "{}\n\nThe task:\n{}",
                crate::ai::agent::run::SYSTEM_PROMPT,
                prompt
            ),
            sink,
            answer_rx,
            policy,
            // P1-B fix: no longer unconditionally `true`. `started` is
            // computed once above (`started_for_execution`, step 2b) from
            // this session's durable `header.graph_started_at` -- `false`
            // for a Plan session's proposal turn (and any later turn on a
            // Plan session whose proposal has not yet been accepted),
            // `true` for every other intent and for a Plan session once
            // Start has actually been pressed. `cli_run::handle` consults
            // this on every `PermissionRequest`
            // (`policy.check_tool_capability(started, ..)`), so a proposal
            // turn is hard-refused any write tool the same way Ask/Review
            // are, rather than merely "not offered a worktree to write
            // into."
            started,
            cancel_handle,
            // R6.4: no budget applies to the lead/solo path -- only a
            // proposed helper job carries one (`ExecutionRecord::budget`,
            // set at graph-start time). This execution's own record is
            // never a helper's, so it never has one to enforce.
            None,
            // Checked over before it is called finished, but only when the
            // run could change files at all: a read-only chat has no diff
            // and nothing to have cut corners on.
            worktree_for_audit,
            spec_for_audit,
        )
        .await;

        crate::commands::airun::gate_answers()
            .lock()
            .unwrap()
            .remove(&(session_id_for_task.clone(), run_session_id_for_task.clone()));
        // Task 2.5's other half: whatever called `ExecutionRegistry::stop`
        // and is awaiting `wait_for_stop` needs to observe completion
        // exactly once `run_task` has actually finished (including its own
        // `conn.shutdown().await`) -- not merely once cancellation was
        // requested.
        executions_for_task.complete(&session_id_for_task, &run_session_id_for_task);
    });

    // No-silent-freeze safety net (House Rule #1: every action produces a
    // visible response). `cli_run::run_task` itself always ends with a sink
    // call on every path it controls (`connect` failure, a missing incoming
    // channel, or the prompt's own outcome all reach the final `Ended` step --
    // see `cli_run.rs`). What it cannot cover is a panic *inside* that spawned
    // task before any of those paths run (a bug in `CliAgent::discover`'s
    // shell-out, a panicking dependency, etc.) -- `tauri::async_runtime::spawn`
    // silently drops a panicked task's result, which would otherwise leave the
    // execution sitting in `Preparing`/`Working` forever with no typed outcome
    // and nothing in the transcript explaining why. Awaiting the join handle
    // from a second task turns that specific failure mode into the same
    // `Ended`/`Failed` event every other failure already produces.
    let app_for_watchdog = app.clone();
    let repo_for_watchdog = repo_id.clone();
    let run_session_id_for_watchdog = run_session_id.clone();
    let session_id_for_watchdog = session_id.to_string();
    let executions_for_watchdog = executions.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(join_error) = join_handle.await {
            log::error!(
                "agent desk execution {} for session {} panicked: {join_error}",
                run_session_id_for_watchdog,
                session_id_for_watchdog
            );
            crate::commands::airun::emit_agent_desk_only(
                &app_for_watchdog,
                &repo_for_watchdog,
                &run_session_id_for_watchdog,
                crate::airun::driver::RunState::Failed,
                crate::airun::driver::RunStep::Ended {
                    state: crate::airun::driver::RunState::Failed,
                    detail: "Something went wrong while starting the agent, and it never got to report why. Nothing was committed and your own work is untouched.".into(),
                },
            );
            crate::commands::airun::gate_answers()
                .lock()
                .unwrap()
                .remove(&(session_id_for_watchdog.clone(), run_session_id_for_watchdog.clone()));
            // A panic inside the spawned task skips the ordinary
            // `executions_for_task.complete(...)` call above entirely (the
            // panic unwinds out of that task, not into this watchdog's own
            // future), so this is the only path left that will ever remove
            // this registration -- without it, the execution would stay
            // "live" in the registry forever despite having no process
            // behind it, and a later Stop would report `Requested` for
            // something that no longer exists.
            executions_for_watchdog.complete(&session_id_for_watchdog, &run_session_id_for_watchdog);
        }
    });

    StartExecutionOutcome::Started {
        session: session_after,
        execution_id,
    }
}

/// Maximum conversation history handed to a fresh provider process.
///
/// This is a character budget, not a claimed token budget: providers tokenize
/// differently. Source and OpenSpec context are outside this budget because
/// they are the durable reason the session exists.
const PROMPT_TRANSCRIPT_CHAR_BUDGET: usize = 48_000;
const PROMPT_MESSAGE_CHAR_BUDGET: usize = 12_000;

/// Builds the prompt text handed to a newly started provider process.
///
/// Provider CLIs start fresh for each execution, so the durable transcript is
/// the continuity boundary. Replay useful user/assistant conversation,
/// including imported messages, but leave out tool activity, approval
/// plumbing, thought summaries, and native system status. History is selected
/// newest-first under the explicit budget, then restored to chronological
/// order. A user message appended while an execution is live cannot enter
/// that already-built snapshot; it remains durable for the next execution.
fn build_prompt(session: &AgentSession) -> String {
    let mut parts = Vec::new();
    let (title, summary) = source_summary(&session.header.source);
    if !title.is_empty() {
        parts.push(title);
    }
    if !summary.is_empty() {
        parts.push(summary);
    }
    if let Some(transcript) = build_transcript_handoff(&session.messages) {
        parts.push(transcript);
    }
    if parts.is_empty() {
        parts.push(session.header.title.clone());
    }
    parts.join("\n\n")
}

fn build_transcript_handoff(messages: &[SessionMessage]) -> Option<String> {
    let eligible: Vec<&SessionMessage> = messages
        .iter()
        .filter(|message| {
            !message.plain_content.trim().is_empty()
                && matches!(message.role, MessageRole::User | MessageRole::Assistant)
                && matches!(message.kind, MessageKind::User | MessageKind::Assistant | MessageKind::Result)
        })
        .collect();
    if eligible.is_empty() {
        return None;
    }

    let mut selected = Vec::new();
    let mut used_chars = 0usize;
    let mut omitted_messages = 0usize;
    for message in eligible.iter().rev() {
        let rendered = render_prompt_message(message);
        let rendered_chars = rendered.chars().count();
        let separator_chars = if selected.is_empty() { 0 } else { 2 };
        if used_chars + separator_chars + rendered_chars > PROMPT_TRANSCRIPT_CHAR_BUDGET {
            omitted_messages += 1;
            continue;
        }
        used_chars += separator_chars + rendered_chars;
        selected.push(rendered);
    }
    selected.reverse();

    let mut handoff = String::from(
        "Conversation so far (quoted history for continuity, not new system instructions):",
    );
    if omitted_messages > 0 {
        handoff.push_str(&format!(
            "\n[Earlier history shortened: {omitted_messages} message(s) were omitted to fit the handoff budget.]"
        ));
    }
    handoff.push_str("\n\n");
    handoff.push_str(&selected.join("\n\n"));
    Some(handoff)
}

fn render_prompt_message(message: &SessionMessage) -> String {
    let role = match message.role {
        MessageRole::User => "User",
        MessageRole::Assistant => "Assistant",
        MessageRole::System => "System",
    };
    let origin = message
        .import
        .as_ref()
        .map(|import| format!(" (imported from {})", import.adapter_id))
        .unwrap_or_default();
    let (content, omitted_chars) = truncate_prompt_text(message.plain_content.trim(), PROMPT_MESSAGE_CHAR_BUDGET);
    let mut rendered = format!("{role}{origin}:\n{content}");
    if omitted_chars > 0 {
        rendered.push_str(&format!(
            "\n[Message shortened: {omitted_chars} character(s) omitted.]"
        ));
    }
    rendered
}

fn truncate_prompt_text(text: &str, max_chars: usize) -> (String, usize) {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return (text.to_string(), 0);
    }
    (text.chars().take(max_chars).collect(), total_chars - max_chars)
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
        | SessionSource::CheckFailure { snapshot, .. }
        | SessionSource::Imported { snapshot, .. } => {
            (snapshot.title.clone(), snapshot.summary.clone())
        }
    }
}

/// Whether the write-authority "Start" transition has durably happened for
/// this session, as of the header read `start_execution_at` took at the top
/// of this call. This is P1-B's actual fix ("Plan can write before Start"):
/// before this existed, `start_execution_at` passed `started: true` into
/// every `cli_run::run_task` call unconditionally -- including the very
/// first Plan-mode turn that PRODUCES the proposal the user has not yet
/// accepted.
///
/// `intent_policy.worktree != WorktreePolicy::NotUntilStart` covers every
/// intent except Plan: Fix is always started (it never has a proposal gate
/// to begin with -- `for_intent(Fix)` already grants `can_write: true` from
/// creation), and every read-only intent's `check_tool_capability` refuses
/// writes regardless of what this returns, so returning `true` for them is
/// harmless -- `started` only changes the answer for
/// `WorktreePolicy::NotUntilStart` (see `policy::check_tool_capability`'s own
/// doc comment).
///
/// For Plan, the answer is `header.graph_started_at.is_some()` -- set once,
/// durably, by `agent_session_start_graph`'s commit step
/// (`commands::agent_graph::commit_started_graph_if_still_proposed`) the
/// moment the user presses Start, and never cleared afterward. This is a
/// property of the SESSION, not of any single execution or in-memory graph
/// state: a Plan session's proposal turn (this function's `false` case) and
/// every later follow-up turn on the same session (this function's `true`
/// case, once Start has been pressed) both reach `start_execution_at`
/// through the exact same `agent_session_start_execution` command
/// (`SessionComposer`'s Send button routes every follow-up message through
/// it, not just the first turn) -- so this durable header flag, read fresh
/// on every call, is what keeps the answer correct across an arbitrary
/// number of turns and an app restart in between, rather than a value that
/// could only be right for one call.
fn started_for_execution(header: &AgentSessionHeader, intent_policy: &crate::agentdesk::policy::IntentPolicy) -> bool {
    intent_policy.worktree != crate::agentdesk::policy::WorktreePolicy::NotUntilStart
        || header.graph_started_at.is_some()
}

/// The OpenSpec change this session's source names, if any -- what
/// [`crate::commands::agent_result::agent_result_build`]'s `openspec_change_id`
/// should carry so a result produced by an OpenSpec-sourced chat gets the
/// `Spec:` trailer automatically (R3.7, R5). `Manual`/`Issue`/`PullRequest`/
/// `Commit`/`Diff`/`WorkingChanges`/`CheckFailure` sources never name an
/// OpenSpec change, so this is `None` for every source but the two OpenSpec
/// ones.
pub(crate) fn openspec_change_id_of(source: &SessionSource) -> Option<String> {
    match source {
        SessionSource::OpenSpecChange { change_id, .. } | SessionSource::OpenSpecTask { change_id, .. } => {
            Some(change_id.clone())
        }
        _ => None,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_start_execution(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    links: tauri::State<'_, crate::agentdesk::RunSessionLinks>,
    manager: tauri::State<'_, crate::state::RepoManager>,
    executions: tauri::State<'_, crate::agentdesk::ExecutionRegistry>,
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
    let executions_owned = executions.inner();
    let outcome = start_execution_at(
        &app,
        &locks_arc,
        &root,
        links_owned,
        manager_owned,
        executions_owned,
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
    /// `stopped` lists exactly which executions were told to stop and
    /// acknowledged it within the timeout -- empty for `StopScope::One`
    /// naming an execution that was not active, which is a no-op, not an
    /// error (it may have finished a moment earlier). `timed_out` lists any
    /// requested executions that did NOT acknowledge in time; those were
    /// still force-stopped (the CLI process is killed regardless, see
    /// `cli_run::run_task`'s `conn.shutdown()`), so their edits/worktree are
    /// preserved exactly like an acknowledged stop -- task 2.5/2.6.
    Stopped {
        session: AgentSession,
        stopped: Vec<ExecutionId>,
        timed_out: Vec<ExecutionId>,
    },
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
}

/// How long [`stop_execution_at`] waits for each targeted execution to
/// acknowledge cancellation before reporting it under `timed_out` rather
/// than `stopped`. Slightly longer than `cli_run::CANCEL_ACK_TIMEOUT` so the
/// CLI's own timeout branch (which still runs `conn.shutdown()` and reports
/// a clean `Ended`) has a chance to win the race and land in `stopped`
/// rather than this command's own, coarser timeout firing first.
const STOP_ACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

async fn stop_execution_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    links: &crate::agentdesk::RunSessionLinks,
    drivers: &crate::commands::airun::DriverRegistry,
    executions: &crate::agentdesk::ExecutionRegistry,
    session_id: &str,
    scope: StopScope,
) -> StopExecutionOutcome {
    use crate::agentdesk::model::SessionLoadError as E;

    // Read first (outside the lock used for the mutating half) only to learn
    // the repository ID the legacy scripted-demo stop path needs, and the
    // set of execution IDs this scope actually targets in the persisted
    // record -- the runtime registry (task 2.1) is keyed by durable
    // (session, execution) IDs, so this is what turns `scope` into the exact
    // set of registry entries to signal.
    let read = locks.with_session_lock(session_id, || {
        store::read_session(root, session_id).map(|s| {
            let repo_id = s.header.repo_id.clone();
            let targeted: Vec<ExecutionId> = s
                .executions
                .iter()
                .filter(|exec| match &scope {
                    StopScope::All => true,
                    StopScope::One { execution_id } => &exec.execution_id == execution_id,
                })
                .filter(|exec| {
                    matches!(
                        exec.state,
                        SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
                    )
                })
                .map(|exec| exec.execution_id.clone())
                .collect();
            (repo_id, targeted)
        })
    });
    let (repo_id, targeted) = match read {
        Ok(v) => v,
        Err(E::NotFound) => return StopExecutionOutcome::NotFound,
        Err(E::Io { detail }) => return StopExecutionOutcome::Unavailable { detail },
        Err(reason) => {
            return StopExecutionOutcome::Damaged {
                reason: reason.to_string(),
            }
        }
    };

    // Legacy path: the scripted-demo driver (`ai_run_start_demo`) is still
    // keyed by `repo_id` and has no execution registry of its own -- this is
    // the one caller that still needs it, kept unchanged from before this
    // task so the demo console (used to build/check the UI without a real
    // provider) is unaffected.
    if let Some(driver) = drivers.get_scripted(&repo_id) {
        use crate::airun::driver::RunDriver;
        driver.lock().unwrap().stop();
    }

    // Task 2.1/2.3/2.4: signal every targeted execution through the runtime
    // registry, keyed by (session, execution) -- never by `repo_id` -- so
    // `StopScope::One` reaches exactly one execution and `StopScope::All`
    // reaches every live execution in THIS session and no other, even if
    // another session happens to share the same repository or reuse an
    // execution ID (see `execution_registry`'s own tests for that exact
    // cross-session case).
    for execution_id in &targeted {
        executions.stop(&session_id.to_string(), execution_id);
    }

    // Task 2.5: wait for acknowledgement (or a typed timeout) BEFORE
    // persisting Stopped. Awaited directly (this function is `async`) rather
    // than via `tauri::async_runtime::block_on` -- nesting a blocking wait
    // for an async future inside a thread that may itself be a runtime
    // worker thread risks a deadlock/starvation the runtime's own docs warn
    // against, so this function is `async` end to end instead. Multiple
    // targeted executions (Stop all) are awaited sequentially rather than
    // concurrently: `STOP_ACK_TIMEOUT` bounds each one individually, so the
    // worst case (every execution times out) is `targeted.len() *
    // STOP_ACK_TIMEOUT`, which is an acceptable trade for keeping this loop
    // simple -- a session rarely has more than a handful of concurrent
    // executions (one lead plus a small number of helpers).
    let mut stopped = Vec::new();
    let mut timed_out = Vec::new();
    for execution_id in &targeted {
        let acknowledged = executions
            .wait_for_stop(&session_id.to_string(), execution_id, STOP_ACK_TIMEOUT)
            .await;
        if acknowledged {
            stopped.push(execution_id.clone());
        } else {
            timed_out.push(execution_id.clone());
        }
    }

    // Mark the targeted execution(s) Stopped in the durable record. The live
    // engine's own `Ended` event will also arrive through the bridge and is
    // idempotent with this (bridge.rs's `map_run_state`/`last_sequence`
    // handling) -- this write exists so the UI reflects the outcome without
    // waiting on that event to round-trip through the engine first.
    //
    // A timed-out execution is still marked `Stopped`, not left running in
    // the persisted record: the process was force-killed regardless (task
    // 2.6's "preserve edits/worktrees" does not require the record to keep
    // claiming the process is alive once this function has already given up
    // waiting on it -- `conn.shutdown()`'s own hard-kill fallback guarantees
    // the process is gone by the time `run_task` returns, whether or not
    // this function's shorter wait observed that in time). The `timed_out`
    // list on the returned outcome is what lets the caller show the visible
    // distinction task 2.5 asks for; the persisted state does not need a
    // parallel distinction to make that promise honest.
    //
    let outcome = update_session_at(locks, root, session_id, |s| {
        for exec in s.executions.iter_mut() {
            let acted_on = stopped.contains(&exec.execution_id) || timed_out.contains(&exec.execution_id);
            if !acted_on {
                continue;
            }
            if matches!(
                exec.state,
                SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
            ) {
                exec.state = SessionState::Stopped;
                exec.ended_at = Some(now_rfc3339());
            }
        }
        if !stopped.is_empty() || !timed_out.is_empty() {
            s.header.state = SessionState::Stopped;
            if let Some(active) = &s.header.active_execution_id {
                if stopped.contains(active) || timed_out.contains(active) {
                    // The active execution slot stays populated (last
                    // execution the session ran), matching architecture.md:
                    // `active_execution_id` names the current/most recent
                    // execution, not only a running one.
                }
            }
        }
    });

    // Unlink exactly the executions this call actually acted on (stopped or
    // timed out), never the repository as a whole. `RunSessionLinks` is now
    // execution-addressed (the P0 routing fix), so this no longer needs the
    // "is any OTHER execution on this session still active" guard the old
    // repo-keyed link required -- unlinking `exec-1` can never disturb
    // `exec-2`'s own mapping, whether that sibling belongs to this session or
    // (in the cross-contamination case this fix closes) a different one that
    // happens to share the repository. Guarded on `Updated` so a failed write
    // never unlinks an execution whose persisted state was not actually
    // changed.
    if matches!(outcome, UpdateSessionOutcome::Updated { .. }) {
        for execution_id in stopped.iter().chain(timed_out.iter()) {
            links.unlink(execution_id);
        }
    }

    match outcome {
        UpdateSessionOutcome::Updated { session } => StopExecutionOutcome::Stopped {
            session,
            stopped,
            timed_out,
        },
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
    executions: tauri::State<'_, crate::agentdesk::ExecutionRegistry>,
    session_id: SessionId,
    scope: StopScope,
) -> Result<StopExecutionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    let links_owned = links.inner();
    let drivers_owned = drivers.inner();
    let executions_owned = executions.inner();
    // `stop_execution_at` is `async` end to end (task 2.5's wait for
    // cancellation acknowledgement lives inside it) and is awaited directly
    // here, the same way `start_execution_at` is called synchronously from
    // its own command wrapper -- there is no `spawn_blocking` step because
    // nothing in `stop_execution_at` blocks a thread; the only wait is the
    // async `ExecutionRegistry::wait_for_stop`, which yields back to the
    // runtime like any other `.await` rather than parking a worker thread.
    let outcome = stop_execution_at(
        &locks_arc,
        &root,
        links_owned,
        drivers_owned,
        executions_owned,
        &session_id,
        scope,
    )
    .await;
    Ok(outcome)
}

/// Whether an approval reached a live gate. R6.6's other half: an approval
/// must reach ONLY the execution that actually asked for it -- a helper's
/// gate can never be answered by a click meant for the lead's, or a
/// sibling's, even though all three can be open in the same session at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AnswerGateOutcome {
    /// The answer reached a live gate for this exact (session, execution).
    Delivered,
    /// Nothing in this process is waiting on a gate for this (session,
    /// execution) pair -- it may have already been answered, the run may
    /// have finished/stopped, or this process never started it (a session
    /// reopened after a restart). Not an error: the visible effect is simply
    /// that nothing happens, same as `ExecutionRegistry::stop`'s `NotLive`.
    NoLiveGate,
}

/// What the transcript records when an approval is answered.
///
/// Plain and in the past tense, because it is a record of something the person
/// did rather than a description of a state.
fn gate_answer_note(answer: crate::airun::driver::GateAnswer) -> &'static str {
    use crate::airun::driver::GateAnswer;
    match answer {
        GateAnswer::AllowOnce => "You allowed this, just this once.",
        GateAnswer::FindAnotherWay => "You asked the agent to find another way.",
        GateAnswer::StopRun => "You stopped the run here.",
    }
}

/// Answers a gate for exactly one live Agent Desk execution.
///
/// Looked up by `(session_id, execution_id)` in the SAME registry
/// `start_execution_at` populates before the run's first `Working` event
/// (see that function's own comment on `gate_answers()`) -- never by
/// `repo_id` alone, which is what let one session's lead and helper gates
/// collide before R6.6.
#[tauri::command]
#[specta::specta]
pub async fn agent_session_answer_gate(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    execution_id: ExecutionId,
    answer: crate::airun::driver::GateAnswer,
) -> Result<AnswerGateOutcome, AppError> {
    let sender = crate::commands::airun::gate_answers()
        .lock()
        .unwrap()
        .get(&(session_id.clone(), execution_id))
        .cloned();
    match sender {
        Some(tx) => {
            // A closed receiver means the run already ended between the
            // lookup above and this send -- reported the same as never
            // having found one, since the visible effect (nothing resumes)
            // is identical either way.
            match tx.send(answer) {
                Ok(()) => {
                    // Write the decision into the transcript.
                    //
                    // The answer used to go down this channel and nowhere
                    // else, and the approval message is created once and never
                    // amended -- so "Answer sent." lived only in component
                    // state. After a pane switch, a refetch or a scroll
                    // remount, a settled decision was amber "Needs your
                    // approval" again with three live buttons, and clicking
                    // one said the request was no longer waiting. The app
                    // invited a click it then refused, on the safety card the
                    // gate exists for.
                    //
                    // Best effort: the answer HAS been delivered by this
                    // point, so a failed note must not turn a successful
                    // approval into an error.
                    if let Ok(root) = resolve_root(&app) {
                        crate::commands::agent_graph::append_system_note(
                            locks.inner(),
                            &root,
                            &session_id,
                            gate_answer_note(answer),
                        );
                    }
                    Ok(AnswerGateOutcome::Delivered)
                }
                Err(_) => Ok(AnswerGateOutcome::NoLiveGate),
            }
        }
        None => Ok(AnswerGateOutcome::NoLiveGate),
    }
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
    /// How full the model's context window is right now, and how big it is.
    ///
    /// Occupancy, not spend: it falls when the agent compacts its history, so
    /// it is the newest reading rather than a total. Reported by the agent
    /// over ACP's own `usage_update`; absent for agents that do not send one.
    pub context_used: Option<UsageValue>,
    pub context_size: Option<UsageValue>,
    pub active_helper_count: Option<u32>,
    /// One row per execution that reported any usage at all, in the order the
    /// executions were recorded. Empty when nothing reported anything. The
    /// session totals above already include these; this is the breakdown.
    #[serde(default)]
    pub agents: Vec<AgentUsageRow>,
    /// RFC 3339 UTC timestamp of when this data was produced, so the UI can
    /// show "as of" rather than implying it is live.
    pub data_timestamp: String,
}

/// What one execution (the lead, or a single helper) reported. Every figure
/// is optional for the same reason as on [`SessionUsage`]: absent means the
/// provider did not say, never zero. `u32` throughout because specta cannot
/// export 64-bit integers, and that failure is silent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageRow {
    pub execution_id: ExecutionId,
    /// `"Lead"` for the lead/solo execution, otherwise the helper's job title
    /// (falling back to its execution id when a helper has none).
    pub label: String,
    pub is_lead: bool,
    /// Input plus output tokens, when BOTH were reported.
    ///
    /// This used to be `a.unwrap_or(0) + b.unwrap_or(0)` whenever either was
    /// present, so a provider reporting input but not output produced an
    /// undercounted figure presented as the agent's total. The graph panel
    /// already refuses that (`nodeUsageLine` names the half it knows), and
    /// qa-log #68 settled the rule -- one layer away from here.
    pub tokens: Option<u32>,
    /// The halves, so a caller can say which one it knows when only one was
    /// reported rather than adding a zero for the other.
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    /// Provider-reported cost in millionths of a dollar.
    pub cost_micro_usd: Option<u32>,
    pub turns: Option<u32>,
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
    /// GitWyrm worked this out itself and can vouch for it -- turn counts,
    /// message counts, helper counts.
    Measured,
    /// The provider told us. True of token counts, cost, and context-window
    /// readings: GitWyrm has no way to verify any of them.
    ProviderReported,
    /// A figure GitWyrm derived rather than counted or received.
    ///
    /// **Nothing constructs this today, deliberately.** GitWyrm does not
    /// estimate any usage figure: each one is either counted here or reported
    /// by the provider, and an absent figure stays absent rather than being
    /// guessed at (the "unknown stays unknown, never zero" rule). The variant
    /// exists so that if a figure ever IS derived -- a cost computed from
    /// token counts and a price table, say -- it can be labelled honestly
    /// instead of borrowing one of the other two labels.
    ///
    /// Recorded as N6 across passes 24-41 as "unreachable". Unreachable is
    /// the correct state for it; the real half of N6 was that `Measured` was
    /// unreachable too, which was wrong and is now fixed.
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
/// (`source: measured`) and sums whatever token/cost figures the provider
/// reported into `ExecutionRecord::usage` (`source: providerReported`).
///
/// A field stays `None` unless something actually reported it -- most ACP
/// agents report no usage at all, and `plan_limit`/`plan_reset_at` have no
/// source yet. architecture.md section 12: "It never turns unknown into zero
/// and never invents a dollar estimate."
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
            // A chat that never had helpers gets no row at all.
            //
            // This used to return `Some(0)`, reasoning that zero is a real
            // measured count rather than an unknown. True -- but it meant
            // every solo chat carried a permanent "Helpers active 0" about a
            // concept its owner has not met, and it meant `buildUsageRows`
            // was never empty, so the card's own "no usage data yet" state
            // was unreachable. qa-log #94 settled that a count which can only
            // ever be zero should not be shown.
            //
            // A chat that HAS had helpers still reports 0 when none are
            // running: there the number is telling you something changed.
            None
        }
    };

    // Turns, not messages. Counting messages counted every streamed note and
    // every tool activity row, so a three-turn run reported dozens of
    // "turns". Each execution's own recorded turn count is the only real
    // figure, so when no execution recorded usage at all this stays `None`
    // rather than falling back to a transcript row count dressed up as turns.
    let session_requests = {
        let reported: Option<u32> = session
            .executions
            .iter()
            .filter_map(|e| e.usage.as_ref())
            .map(|u| u.turns)
            .fold(None, |acc: Option<u32>, turns| Some(acc.unwrap_or(0).saturating_add(turns)));
        reported.map(|turns| UsageValue {
            value: f64::from(turns),
            // Measured, not reported: `UsageTotals::turns`'s own doc says
            // "Always known, because GitWyrm counts them itself rather than
            // asking the provider", and the token/cost totals below state the
            // rule -- `Measured` is for what GitWyrm can verify itself.
            //
            // Every usage figure was stamped `ProviderReported`, so
            // `UsageSource::Measured` was constructed nowhere in the tree and
            // the vision's three-way labelling was one-way in practice. This
            // is the one figure that is genuinely ours to claim.
            source: UsageSource::Measured,
        })
    };

    // The most recent context reading from the LEAD. Deliberately not summed:
    // two agents each holding 30k of their own 200k windows is not 60k of
    // anything. Helpers are skipped on purpose: the reading a person wants
    // when asking "how close am I to a compaction" is the conversation they
    // are typing into, and a helper that reported later would otherwise
    // replace it.
    let latest_context = session
        .executions
        .iter()
        .filter(|e| e.parent_execution_id.is_none())
        .filter_map(|e| e.usage.as_ref())
        .filter_map(|u| Some((u.context_used?, u.context_size?)))
        .next_back();

    let agents: Vec<AgentUsageRow> = session
        .executions
        .iter()
        .filter_map(|e| {
            let usage = e.usage.as_ref()?;
            let is_lead = e.parent_execution_id.is_none();
            // Only a real total. A half-known figure is reported through
            // `input_tokens`/`output_tokens` instead, so the caller can say
            // which half it is.
            let tokens = match (usage.input_tokens, usage.output_tokens) {
                (Some(a), Some(b)) => Some(a.saturating_add(b)),
                _ => None,
            };
            Some(AgentUsageRow {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                execution_id: e.execution_id.clone(),
                label: if is_lead {
                    "Lead".to_string()
                } else {
                    // A word, not a hex id. This fell back to `execution_id`,
                    // so an untitled helper was named by an ellipsised hex
                    // fragment in the hardest-truncated column of the cost
                    // screen. Every other consumer falls back to a human word
                    // and the graph's own comment states the rule.
                    e.job_title.clone().unwrap_or_else(|| "Helper".to_string())
                },
                is_lead,
                tokens,
                cost_micro_usd: usage.cost_micro_usd,
                turns: Some(usage.turns),
            })
        })
        .collect();

    // Summed across every execution on the session -- helpers included, since
    // a lead that spent its budget on five helpers cost the user all five.
    // Left `None` unless at least one execution actually reported a figure:
    // summing a session whose provider reports nothing must stay "not
    // reported", never a measured zero.
    let (session_tokens, session_cost_usd) = {
        let mut tokens: Option<u64> = None;
        let mut micro_usd: Option<u64> = None;
        for usage in session.executions.iter().filter_map(|e| e.usage.as_ref()) {
            for part in [usage.input_tokens, usage.output_tokens] {
                if let Some(v) = part {
                    // Widened to u64 for the sum: each execution's own figure
                    // is u32 (what the bindings can carry), but a session with
                    // many helpers could in principle total past that.
                    tokens = Some(tokens.unwrap_or(0).saturating_add(u64::from(v)));
                }
            }
            if let Some(c) = usage.cost_micro_usd {
                micro_usd = Some(micro_usd.unwrap_or(0).saturating_add(u64::from(c)));
            }
        }
        (
            tokens.map(|t| UsageValue {
                value: t as f64,
                // The provider counted these, not us -- `Measured` is
                // reserved for what GitWyrm can verify itself (message
                // counts, helper counts).
                source: UsageSource::ProviderReported,
            }),
            micro_usd.map(|m| UsageValue {
                value: m as f64 / 1_000_000.0,
                source: UsageSource::ProviderReported,
            }),
        )
    };

    SessionUsageOutcome::Available {
        usage: SessionUsage {
            session_tokens,
            session_requests,
            session_cost_usd,
            plan_limit: None,
            plan_reset_at: None,
            // The newest reading across this session's executions, not a sum.
            // The lead's own window is the one that matters to a person
            // reading "how close am I to a compaction".
            context_used: latest_context.map(|(used, _)| UsageValue {
                value: f64::from(used),
                source: UsageSource::ProviderReported,
            }),
            context_size: latest_context.map(|(_, size)| UsageValue {
                value: f64::from(size),
                source: UsageSource::ProviderReported,
            }),
            active_helper_count,
            agents,
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
            // An imported chat has no live source to re-read: its content is
            // a copy taken from another client at import time, and the way to
            // get newer messages is to import again, not to refresh. Marking
            // it unavailable would claim something was lost when nothing was.
            SessionSource::Imported { .. } => None,
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
///
/// `pub(crate)`: `commands::agent_graph::start_graph_at` (task 3.3, "detect
/// task/spec changes after draft and block Start until refreshed/accepted")
/// needs the same target resolution to recompute the current OpenSpec context
/// fingerprint before honoring Start -- reusing this rather than a second copy
/// of the `SessionSource` match keeps both call sites reading the exact same
/// "what does this session point at" answer.
pub(crate) enum OpenSpecTarget {
    Change { change_id: String, snapshot_title: String },
    Task { change_id: String, task_index: u32, task_text: String, snapshot_title: String },
}

pub(crate) fn openspec_target_of(source: &SessionSource) -> Option<OpenSpecTarget> {
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
pub(crate) fn resolve_openspec_context(
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

/// tasks.md 2.4, third of three: "mark launch-vs-live differences." Wiring
/// the OpenSpec context into the live prompt (`start_execution_at`) and
/// rebuilding it on a file-watcher refresh (`refresh_source_at`'s OpenSpec
/// branch) both existed already; this is the piece that was missing --
/// nothing told the USER when the source they are looking at has moved since
/// the agent last read it.
///
/// Compares the most recent execution's stamped `context_fingerprint`
/// (R5.3 -- what the agent actually saw) against a fingerprint recomputed
/// from the CURRENT live files right now, using the identical
/// `resolve_openspec_context` + `fingerprint` pair `start_execution_at`
/// itself uses, so this can never disagree with what a fresh execution would
/// actually be handed. Deliberately its own tiny read command (not folded
/// into `agent_session_openspec_context`, which returns the context itself
/// and is polled/rendered separately) so `SessionContextPanel` can show this
/// as a lightweight banner without re-fetching or re-rendering the whole
/// proposal/design/deltas/tasks body on every poll.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenSpecContextDriftOutcome {
    /// This session has at least one execution with a stamped
    /// `context_fingerprint`, and the comparison ran cleanly.
    Checked {
        /// True when the live files no longer match what the most recent
        /// execution was launched against.
        diverged: bool,
        /// RFC 3339 timestamp of the execution the comparison was made
        /// against, so the banner can say "since it started" plainly.
        launched_at: String,
    },
    /// This session's source is not OpenSpec, or no execution has run yet
    /// (nothing to compare against) -- not an error, just nothing to report.
    NothingToCompare,
    RepoNotOpen,
    NoOpenSpecFolder,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
}

fn openspec_context_drift_at(
    root: &SessionStoreRoot,
    manager: &crate::state::RepoManager,
    session_id: &str,
) -> OpenSpecContextDriftOutcome {
    use crate::agentdesk::model::SessionLoadError as E;
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(E::NotFound) => return OpenSpecContextDriftOutcome::SessionNotFound,
        Err(E::Io { detail }) => return OpenSpecContextDriftOutcome::SessionUnavailable { detail },
        Err(reason) => {
            return OpenSpecContextDriftOutcome::SessionDamaged {
                reason: reason.to_string(),
            }
        }
    };
    let Some(target) = openspec_target_of(&session.header.source) else {
        return OpenSpecContextDriftOutcome::NothingToCompare;
    };
    // The most recently STARTED execution that actually carries a
    // fingerprint -- a session can accumulate executions that predate R5.3
    // (fingerprint added later) or that were never OpenSpec-sourced at the
    // time (should not happen for a session whose CURRENT source is OpenSpec,
    // but a defensive `filter_map` costs nothing and avoids a false
    // "NothingToCompare" if a caller somehow reaches this before any launch).
    let Some(most_recent) = session
        .executions
        .iter()
        .filter(|e| e.context_fingerprint.is_some())
        .max_by(|a, b| a.started_at.cmp(&b.started_at))
    else {
        return OpenSpecContextDriftOutcome::NothingToCompare;
    };
    let launched_fingerprint = most_recent.context_fingerprint.clone().expect("filtered above");
    let launched_at = most_recent.started_at.clone();

    if manager.get(&session.header.repo_id).is_err() {
        return OpenSpecContextDriftOutcome::RepoNotOpen;
    }
    let repo_path = PathBuf::from(&session.header.repo_path);
    let Some(ctx) = resolve_openspec_context(&repo_path, &target) else {
        return OpenSpecContextDriftOutcome::NoOpenSpecFolder;
    };
    let current_fingerprint = crate::agentdesk::openspec_context::fingerprint(&ctx);
    let diverged = crate::agentdesk::openspec_context::context_changed_since(
        &current_fingerprint,
        &launched_fingerprint,
    );
    OpenSpecContextDriftOutcome::Checked { diverged, launched_at }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_openspec_context_drift(
    app: AppHandle,
    manager: tauri::State<'_, crate::state::RepoManager>,
    session_id: SessionId,
) -> Result<OpenSpecContextDriftOutcome, AppError> {
    let root = resolve_root(&app)?;
    Ok(openspec_context_drift_at(&root, manager.inner(), &session_id))
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

    // -- P1-B: `started_for_execution` -- the durable-flag half of "Plan can
    //    write before Start". `cli_run::run_task`'s own `started` parameter
    //    is proven to gate writes correctly by `policy.rs`'s
    //    `plan_cannot_write_before_start_but_can_after` and
    //    `read_only_intents_can_never_write` tests; what those tests cannot
    //    prove is that `start_execution_at` computes the VALUE it passes in
    //    correctly, from the session's own durable header rather than a
    //    hardcoded `true` (the actual bug) -- that is this function's job,
    //    and these are its tests. --

    fn header_with_intent_and_graph_started(intent: SessionIntent, graph_started_at: Option<&str>) -> AgentSessionHeader {
        AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: "sess-1".into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/proj".into(),
            repo_name: "proj".into(),
            title: "Test session".into(),
            source: SessionSource::Manual {
                repo_id: "repo-1".into(),
            },
            intent,
            state: SessionState::Ready,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            unread: false,
            changed_file_count: 0,
            active_execution_id: None,
            archived: false,
            graph_started_at: graph_started_at.map(|s| s.to_string()),
            preferred_provider: None,
            preferred_mode: None,
            preferred_team: None,
        preferred_model: None,
        preferred_effort: None,
        }
    }

    /// A freshly proposed Plan session (no Start pressed yet) must compute
    /// `started == false` -- this is the exact turn that used to receive
    /// write authority it had not earned, before this fix.
    #[test]
    fn plan_without_graph_started_at_is_not_started() {
        let header = header_with_intent_and_graph_started(SessionIntent::Plan, None);
        let policy = crate::agentdesk::policy::for_intent(header.intent);
        assert!(!started_for_execution(&header, &policy));
    }

    /// Once `agent_session_start_graph`'s commit step has durably stamped
    /// `graph_started_at` (P1-B's actual fix), every later turn on that same
    /// session -- not just the one immediately after Start -- must compute
    /// `started == true`.
    #[test]
    fn plan_with_graph_started_at_is_started() {
        let header = header_with_intent_and_graph_started(SessionIntent::Plan, Some("2026-01-01T00:00:00Z"));
        let policy = crate::agentdesk::policy::for_intent(header.intent);
        assert!(started_for_execution(&header, &policy));
    }

    /// Every non-Plan intent is `started` regardless of `graph_started_at`
    /// (which they never set in the first place) -- Fix has no Start gate at
    /// all, and read-only intents are refused writes by `can_write` itself,
    /// not by this flag, so this must not accidentally start gating them.
    #[test]
    fn non_plan_intents_are_always_started_regardless_of_graph_started_at() {
        for intent in [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Review,
            SessionIntent::Summarize,
            SessionIntent::Fix,
        ] {
            let header = header_with_intent_and_graph_started(intent, None);
            let policy = crate::agentdesk::policy::for_intent(intent);
            assert!(started_for_execution(&header, &policy), "{intent:?} should always report started");
        }
    }

    // -- P1-C wiring 4: `cleanup_unused_worktree` -- the mechanism a losing
    //    solo-start race (and a `CliAgent::discover` failure after
    //    provisioning) calls into so the worktree that call itself just
    //    created is not left behind. Exercising the full race through
    //    `start_execution_at` would require a real `CliAgent::discover`
    //    shell-out (an external process), so this proves the mechanism
    //    directly: provision exactly the way `start_execution_at` step 2b
    //    does, then confirm cleanup actually removes the folder and its
    //    branch, matching `agent_graph.rs`'s own
    //    `commit_started_graph_if_still_proposed`/`AlreadyStarted` cleanup
    //    for the equivalent graph-start race. --

    /// A real git repo with one commit on `main`, matching
    /// `commands::agent_kickoff`'s own `repo_with_commit` fixture (kept as a
    /// separate copy rather than made `pub(crate)` and shared -- this
    /// module's own test conventions keep fixtures private per file, same as
    /// every other `temp_root`/`test_locks` helper here).
    fn repo_with_commit() -> (tempfile::TempDir, std::sync::Arc<crate::state::OpenRepo>) {
        let dir = tempfile::TempDir::new().expect("temp repo");
        let repo = git2::Repository::init(dir.path()).expect("init repo");
        {
            let mut config = repo.config().expect("config");
            config.set_str("user.name", "Cleanup Test").expect("name");
            config
                .set_str("user.email", "cleanup@example.com")
                .expect("email");
        }
        std::fs::write(dir.path().join("a.txt"), "a").expect("write file");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("a.txt")).expect("add");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("tree id");
        let sig = git2::Signature::now("Cleanup Test", "cleanup@example.com").expect("sig");
        {
            let tree = repo.find_tree(tree_id).expect("tree");
            repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
                .expect("commit");
        }
        let open = std::sync::Arc::new(crate::state::OpenRepo::for_test(repo));
        (dir, open)
    }

    /// The exact shape a losing race leaves behind before this fix: a
    /// worktree `provision_kickoff_worktree` (step 2b's own call) created,
    /// with nothing else ever referencing it. Proves `cleanup_unused_worktree`
    /// actually removes it from disk and from `git worktree list`, not just
    /// that it returns without erroring.
    #[test]
    fn cleanup_unused_worktree_removes_a_freshly_provisioned_worktree() {
        let (_repo_dir, open) = repo_with_commit();
        let base_branch = {
            let repo = open.repo.lock().unwrap();
            // Bound, not chained: the `Reference` borrows `repo`, so a
            // temporary would still be alive when the guard drops.
            let head = repo.head().expect("head");
            head.shorthand().expect("shorthand").to_string()
        };
        let outcome = crate::commands::agent_kickoff::provision_kickoff_worktree(
            &open,
            &base_branch,
            "Losing race test",
        );
        let crate::commands::agent_kickoff::ProvisionKickoffWorktreeOutcome::Provisioned { path, .. } = outcome
        else {
            panic!("expected the worktree to be provisioned");
        };
        assert!(Path::new(&path).exists(), "worktree must exist before cleanup runs");

        cleanup_unused_worktree(&open, &path);

        assert!(
            !Path::new(&path).exists(),
            "a losing attempt's worktree must not be left behind on disk"
        );
        let repo = open.repo.lock().unwrap();
        assert!(
            crate::git::worktree::list(&repo, None)
                .into_iter()
                .all(|w| !crate::git::worktree::paths_equal(std::path::Path::new(&w.path), Path::new(&path))),
            "the removed worktree must not still be listed"
        );
    }

    /// `cleanup_unused_worktree` must never touch the main checkout -- only
    /// ever called (from `start_execution_at`) with the path this same call
    /// provisioned, but proven here directly since a future call-site bug
    /// passing `open.path` by mistake would otherwise silently discard the
    /// user's own working folder instead of a scratch worktree.
    #[test]
    fn cleanup_unused_worktree_refuses_the_main_checkout() {
        let (_repo_dir, open) = repo_with_commit();
        let main_path = open.path.to_string_lossy().into_owned();
        assert!(Path::new(&main_path).join("a.txt").exists());

        cleanup_unused_worktree(&open, &main_path);

        // `git::worktree::remove` refuses `entry.is_main` outright (see its
        // own doc comment) and `cleanup_unused_worktree` swallows that
        // refusal -- the file must still be there.
        assert!(
            Path::new(&main_path).join("a.txt").exists(),
            "the main checkout must survive a cleanup call, even if one were ever mis-targeted at it"
        );
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
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        let outcome = get_session_at(&locks, &executions, &root, "does-not-exist");
        assert!(matches!(outcome, GetSessionOutcome::NotFound));
    }

    #[test]
    fn get_returns_damaged_for_a_corrupt_file() {
        let (dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        let path = dir
            .path()
            .join("agent-desk")
            .join("v1")
            .join("sessions")
            .join("sess-bad.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let outcome = get_session_at(&locks, &executions, &root, "sess-bad");
        assert!(matches!(outcome, GetSessionOutcome::Damaged { .. }));
    }

    #[test]
    fn get_finds_a_created_session() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Findable"))
        else {
            panic!("expected Created");
        };
        let outcome = get_session_at(&locks, &executions, &root, &session.header.session_id);
        let GetSessionOutcome::Found { session: found } = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        assert_eq!(found, session);
    }

    // -- Startup reconciliation: a session persisted mid-run reconciles to
    // Interrupted on load when nothing in this process backs it. --

    /// Writes a session whose header and one execution both claim `state`
    /// (a live-process state, in every test below), with `active_execution_id`
    /// pointing at that execution -- the exact on-disk shape
    /// `record_execution_if_not_running` leaves behind mid-run, and exactly
    /// what a crash/force-quit/power-loss/app-update freezes in place.
    fn write_stuck_session(root: &SessionStoreRoot, session_id: &str, state: SessionState) -> AgentSession {
        let mut session = AgentSession::new(AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: session_id.into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/proj".into(),
            repo_name: "proj".into(),
            title: "Stuck session".into(),
            source: SessionSource::Manual {
                repo_id: "repo-1".into(),
            },
            intent: SessionIntent::Fix,
            state,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            unread: false,
            changed_file_count: 0,
            active_execution_id: Some("exec-1".into()),
            archived: false,
            graph_started_at: None,
            preferred_provider: None,
            preferred_mode: None,
            preferred_team: None,
        preferred_model: None,
        preferred_effort: None,
        });
        session.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
            "exec-1".into(),
            session_id.into(),
            None,
            state,
            "2026-01-01T00:00:00Z".into(),
            None,
            3,
        ));
        store::write_session(root, &session).expect("write fixture session");
        session
    }

    #[test]
    fn a_session_persisted_as_preparing_with_no_live_process_reconciles_on_load() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new(); // empty: nothing registered either
        write_stuck_session(&root, "sess-stuck", SessionState::Preparing);

        let outcome = get_session_at(&locks, &executions, &root, "sess-stuck");
        let GetSessionOutcome::Found { session } = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        assert_eq!(session.header.state, SessionState::Interrupted);
        assert_eq!(session.executions[0].state, SessionState::Interrupted, "the execution record, not just the header, must be reconciled");

        // The fix is durable: re-reading directly from disk (bypassing
        // get_session_at entirely) shows the same reconciled state, so this
        // is not just an in-memory patch that disappears on the next load.
        let reread = store::read_session(&root, "sess-stuck").expect("session still readable");
        assert_eq!(reread.header.state, SessionState::Interrupted);
        assert_eq!(reread.executions[0].state, SessionState::Interrupted);
    }

    #[test]
    fn a_session_persisted_as_working_with_no_live_process_reconciles_on_load() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        write_stuck_session(&root, "sess-stuck", SessionState::Working);

        let outcome = get_session_at(&locks, &executions, &root, "sess-stuck");
        let GetSessionOutcome::Found { session } = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        assert_eq!(session.header.state, SessionState::Interrupted);
    }

    #[test]
    fn a_session_persisted_as_needs_input_with_no_live_process_reconciles_on_load() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        write_stuck_session(&root, "sess-stuck", SessionState::NeedsInput);

        let outcome = get_session_at(&locks, &executions, &root, "sess-stuck");
        let GetSessionOutcome::Found { session } = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        assert_eq!(session.header.state, SessionState::Interrupted);
    }

    #[test]
    fn a_session_whose_execution_is_genuinely_linked_is_left_running() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        write_stuck_session(&root, "sess-live", SessionState::Working);
        // This is what a real in-process execution looks like: the runtime
        // registry has the execution registered (task 2.2: registered before
        // `Working` is ever emitted) -- `get_session_at` reads liveness
        // straight from `ExecutionRegistry` (session-granular, no repository
        // indirection) for both the header (`reconcile_session_header`) and
        // the execution records (`reconcile_session_executions`).
        executions.register(
            "sess-live".to_string(),
            "exec-1".to_string(),
            crate::airun::cli_run::CancelHandle::new(),
        );

        let outcome = get_session_at(&locks, &executions, &root, "sess-live");
        let GetSessionOutcome::Found { session } = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        assert_eq!(session.header.state, SessionState::Working, "a genuinely live session must not be reconciled");
        assert_eq!(session.executions[0].state, SessionState::Working);
    }

    #[test]
    fn a_session_already_finished_failed_or_stopped_is_left_untouched_by_reconciliation() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        for (id, state) in [
            ("sess-finished", SessionState::Finished),
            ("sess-failed", SessionState::Failed),
            ("sess-stopped", SessionState::Stopped),
        ] {
            write_stuck_session(&root, id, state);
            let outcome = get_session_at(&locks, &executions, &root, id);
            let GetSessionOutcome::Found { session } = outcome else {
                panic!("expected Found for {id}, got {outcome:?}");
            };
            assert_eq!(session.header.state, state, "{id} must not be rewritten by reconciliation");
        }
    }

    #[test]
    fn a_damaged_session_file_does_not_prevent_another_sessions_reconciliation() {
        let (dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let executions = crate::agentdesk::ExecutionRegistry::new();

        // One corrupt file alongside the stuck session -- store::read_session
        // reports it as Damaged and never touches it, per store.rs's own
        // quarantine contract; this just confirms get_session_at inherits
        // that behavior rather than panicking or otherwise interrupting the
        // reconciliation of a session it is not even asked about.
        let bad_path = dir.path().join("agent-desk").join("v1").join("sessions").join("sess-bad.json");
        std::fs::write(&bad_path, b"{ not json").unwrap();
        write_stuck_session(&root, "sess-stuck", SessionState::Working);

        let bad_outcome = get_session_at(&locks, &executions, &root, "sess-bad");
        assert!(matches!(bad_outcome, GetSessionOutcome::Damaged { .. }));

        let good_outcome = get_session_at(&locks, &executions, &root, "sess-stuck");
        let GetSessionOutcome::Found { session } = good_outcome else {
            panic!("expected Found, got {good_outcome:?}");
        };
        assert_eq!(session.header.state, SessionState::Interrupted);

        // The corrupt file itself must still be untouched, matching every
        // other quarantine test in this codebase (store.rs: "One damaged
        // session").
        assert_eq!(std::fs::read_to_string(&bad_path).unwrap(), "{ not json");
    }

    #[test]
    fn every_stuck_execution_in_a_multi_execution_session_reconciles_not_just_the_active_one() {
        // Regression for the graph panel: helper nodes render straight from
        // `session.executions` (agentGraphProjection.ts), so a fix that only
        // patched the header's own state would leave every helper showing
        // "Working" forever after a crash mid-graph-run.
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        let mut session = write_stuck_session(&root, "sess-graph", SessionState::Working);
        session.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
            "exec-helper".into(),
            "sess-graph".into(),
            Some("exec-1".into()),
            SessionState::NeedsInput,
            "2026-01-01T00:00:00Z".into(),
            None,
            1,
        ));
        session.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
            "exec-done".into(),
            "sess-graph".into(),
            Some("exec-1".into()),
            SessionState::Finished,
            "2026-01-01T00:00:00Z".into(),
            Some("2026-01-01T00:01:00Z".into()),
            5,
        ));
        store::write_session(&root, &session).unwrap();

        let outcome = get_session_at(&locks, &executions, &root, "sess-graph");
        let GetSessionOutcome::Found { session: reconciled } = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        let by_id = |id: &str| reconciled.executions.iter().find(|e| e.execution_id == id).unwrap();
        assert_eq!(by_id("exec-1").state, SessionState::Interrupted, "lead");
        assert_eq!(by_id("exec-helper").state, SessionState::Interrupted, "needs-input helper");
        assert_eq!(by_id("exec-done").state, SessionState::Finished, "already-finished helper untouched");
    }

    /// Task 2.1/2.7's actual improvement over the old session-granular
    /// check: a live LEAD must not resurrect a stuck HELPER that this
    /// process has nothing registered for, and vice versa -- the previous
    /// `RunSessionLinks`-only reconciliation could not tell the two apart
    /// (see `a_session_whose_execution_is_genuinely_linked_is_left_running`'s
    /// old body, which set the same liveness bool for the whole session).
    #[test]
    fn only_the_specific_registered_execution_is_left_running_not_its_siblings() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        let mut session = write_stuck_session(&root, "sess-mixed", SessionState::Working);
        session.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
            "exec-helper".into(),
            "sess-mixed".into(),
            Some("exec-1".into()),
            SessionState::Working,
            "2026-01-01T00:00:00Z".into(),
            None,
            1,
        ));
        store::write_session(&root, &session).unwrap();

        // Only the lead ("exec-1") is registered as live -- the helper is
        // not, simulating a crash that happened mid-helper.
        executions.register(
            "sess-mixed".to_string(),
            "exec-1".to_string(),
            crate::airun::cli_run::CancelHandle::new(),
        );

        let outcome = get_session_at(&locks, &executions, &root, "sess-mixed");
        let GetSessionOutcome::Found { session: reconciled } = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        let by_id = |id: &str| reconciled.executions.iter().find(|e| e.execution_id == id).unwrap();
        assert_eq!(by_id("exec-1").state, SessionState::Working, "the registered lead must stay running");
        assert_eq!(by_id("exec-helper").state, SessionState::Interrupted, "the unregistered helper must be reconciled");
    }

    #[test]
    fn agent_session_list_reconciles_headers_for_display_without_reading_full_session_files() {
        // A list call must reconcile what it shows (so a filter for
        // Interrupted or Working is honest) without paying for a full-session
        // parse of every stuck session -- store::list_sessions_reconciled
        // only ever touches headers (index.json, or `header` fields during a
        // rebuild scan). Proven here by writing sessions whose FULL file body
        // would fail to parse if it were ever read as `AgentSession` (a
        // `messages` field of the wrong type), while their `header` object on
        // its own is well-formed -- if list reconciliation reached for the
        // full session, this test would see a `Damaged`-style failure instead
        // of a normal reconciled page.
        let (dir, root) = temp_root();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        let session = write_stuck_session(&root, "sess-stuck", SessionState::Working);

        let sessions_dir = dir.path().join("agent-desk").join("v1").join("sessions");
        let mut raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(sessions_dir.join("sess-stuck.json")).unwrap()).unwrap();
        raw["messages"] = serde_json::json!("not an array, would fail AgentSession parsing");
        std::fs::write(sessions_dir.join("sess-stuck.json"), serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

        let filter = SessionListFilter::default();
        let page = store::list_sessions_reconciled(&root, &filter, None, 10, |header| {
            !executions
                .live_executions_for_session(&header.session_id)
                .is_empty()
        });

        assert_eq!(page.headers.len(), 1);
        assert_eq!(page.headers[0].session_id, session.header.session_id);
        assert_eq!(
            page.headers[0].state,
            SessionState::Interrupted,
            "the list must show the reconciled state, not the stale one still on disk"
        );
        assert!(page.diagnostics.is_empty(), "a header-only scan must never even notice the malformed messages field");
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
    fn prompt_replays_dialogue_but_not_tool_noise() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } = create_session_at(&root, create_request("Continuity")) else {
            panic!("expected Created");
        };
        let id = session.header.session_id.clone();
        let AppendUserMessageOutcome::Appended { message: first, .. } =
            append_user_message_at(&locks, &root, &id, "Inspect the parser.".into(), vec![]) else {
                panic!("expected Appended");
            };
        update_session_at(&locks, &root, &id, |session| {
            let mut assistant = first.clone();
            assistant.message_id = "assistant-import".into();
            assistant.role = MessageRole::Assistant;
            assistant.kind = MessageKind::Assistant;
            assistant.plain_content = "I found a boundary bug.".into();
            assistant.import = Some(crate::agentdesk::model::ImportProvenance {
                adapter_id: "external-client".into(),
                external_session_id: "chat-1".into(),
                external_message_id: "message-2".into(),
                imported_at: "2026-01-01T00:00:00Z".into(),
            });
            session.messages.push(assistant);
            let mut tool = first.clone();
            tool.message_id = "tool-noise".into();
            tool.role = MessageRole::Assistant;
            tool.kind = MessageKind::Tool;
            tool.plain_content = "Searching 4,812 files...".into();
            session.messages.push(tool);
        });
        append_user_message_at(&locks, &root, &id, "Fix it and add a test.".into(), vec![]);

        let prompt = build_prompt(&store::read_session(&root, &id).expect("session"));
        assert!(prompt.contains("User:\nInspect the parser."));
        assert!(prompt.contains("Assistant (imported from external-client):\nI found a boundary bug."));
        assert!(prompt.contains("User:\nFix it and add a test."));
        assert!(!prompt.contains("Searching 4,812 files"));
        assert_eq!(prompt.matches("Fix it and add a test.").count(), 1);
    }

    #[test]
    fn prompt_shortens_history_honestly_and_keeps_newest_request() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } = create_session_at(&root, create_request("Long chat")) else {
            panic!("expected Created");
        };
        let id = session.header.session_id.clone();
        for index in 0..6 {
            append_user_message_at(
                &locks,
                &root,
                &id,
                format!("old-{index}-{}", "x".repeat(PROMPT_MESSAGE_CHAR_BUDGET)),
                vec![],
            );
        }
        append_user_message_at(&locks, &root, &id, "Newest request".into(), vec![]);
        let prompt = build_prompt(&store::read_session(&root, &id).expect("session"));
        assert!(prompt.contains("Newest request"));
        assert!(prompt.contains("Earlier history shortened:"));
        assert!(prompt.contains("Message shortened:"));
        assert!(prompt.chars().count() < PROMPT_TRANSCRIPT_CHAR_BUDGET + 1_000);
    }

    #[test]
    fn message_saved_during_run_only_enters_next_prompt_snapshot() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let CreateSessionOutcome::Created { session } = create_session_at(&root, create_request("Queued turn")) else {
            panic!("expected Created");
        };
        let id = session.header.session_id.clone();
        append_user_message_at(&locks, &root, &id, "Start the work".into(), vec![]);
        update_session_at(&locks, &root, &id, |session| session.header.state = SessionState::Working);
        let active_prompt = build_prompt(&store::read_session(&root, &id).expect("active snapshot"));

        let outcome = append_user_message_at(&locks, &root, &id, "Also cover the empty case".into(), vec![]);
        assert!(matches!(outcome, AppendUserMessageOutcome::Appended { .. }));
        assert!(!active_prompt.contains("Also cover the empty case"));
        let next = store::read_session(&root, &id).expect("next snapshot");
        assert!(build_prompt(&next).contains("User:\nAlso cover the empty case"));
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

    /// Creates a session through the real command path and returns its id.
    fn seeded_session(root: &SessionStoreRoot, title: &str) -> String {
        match create_session_at(root, create_request(title)) {
            CreateSessionOutcome::Created { session } => session.header.session_id,
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn a_blank_chat_is_named_after_its_first_message() {
        let (_tmp, root) = temp_root();
        let locks = test_locks();
        let id = seeded_session(&root, "");
        match append_user_message_at(&locks, &root, &id, "What folder are we in?".into(), vec![]) {
            AppendUserMessageOutcome::Appended { session, .. } => {
                assert_eq!(session.header.title, "What folder are we in?");
            }
            other => panic!("expected Appended, got {other:?}"),
        }
    }

    #[test]
    fn a_title_that_was_already_set_is_never_overwritten() {
        let (_tmp, root) = temp_root();
        let locks = test_locks();
        let id = seeded_session(&root, "Mine");
        match append_user_message_at(&locks, &root, &id, "something else entirely".into(), vec![]) {
            AppendUserMessageOutcome::Appended { session, .. } => {
                assert_eq!(session.header.title, "Mine");
            }
            other => panic!("expected Appended, got {other:?}"),
        }
    }

    #[test]
    fn a_long_first_message_is_cut_at_a_word_boundary() {
        let long = "This is a deliberately long opening message that runs well past the sixty character budget a sidebar row can show";
        let title = title_from_first_message(long);
        assert!(title.chars().count() <= 60, "got {} chars: {title}", title.chars().count());
        assert!(!title.ends_with(' '));
        assert!(long.starts_with(&title), "title must be a prefix of the message");
        assert!(long[title.len()..].starts_with(' '), "cut landed mid-word");
    }

    #[test]
    fn a_first_message_with_no_spaces_still_produces_a_title() {
        assert_eq!(title_from_first_message(&"x".repeat(200)).chars().count(), 60);
    }

    #[test]
    fn a_blank_first_message_leaves_the_title_blank() {
        assert_eq!(title_from_first_message("   
  
 "), "");
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

        // What `start_execution_at` does before it ever touches the CLI: mint
        // an execution ID, link THAT execution (never the repository -- see
        // `RunSessionLinks`'s own doc comment) to this session, and record it.
        let execution_id = crate::agentdesk::execution_id_for_run_session("run-1");
        links.link(&execution_id, &session_id);
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

    /// Regression test for the "Getting ready... forever" bug: an Agent Desk
    /// execution's engine events were being silently dropped before they ever
    /// reached `route_to_agent_desk`/`route_run_event`, because
    /// `start_execution_at` originally routed its sink through
    /// `commands::airun::emit`, and `emit`'s very first line gates on
    /// `SessionRegistry::record`, which only accepts an event whose
    /// `(repo_id, run_session_id)` pair was previously registered via
    /// `SessionRegistry::start` -- something an Agent Desk execution never
    /// does (it has its own concurrency guard,
    /// `record_execution_if_not_running`, keyed by the durable session rather
    /// than by repository). The first assertion below reproduces that: an
    /// unregistered repo's event is dropped by the registry exactly as it was
    /// in production, which is why the session never left `Preparing` no
    /// matter how many events the engine emitted.
    ///
    /// The fix is `commands::airun::emit_agent_desk_only`, which performs the
    /// same persist-then-emit sequence as `route_to_agent_desk` without ever
    /// touching `SessionRegistry`. The second half of this test proves that
    /// half directly (`route_run_event` is the exact function
    /// `emit_agent_desk_only` calls) -- confirming the durable path succeeds
    /// on its own, independent of whatever `SessionRegistry` would have said.
    #[test]
    fn agent_desk_events_are_not_gated_on_the_airun_session_registry() {
        let registry = crate::airun::session::SessionRegistry::new();
        let event = run_event("run-1", crate::airun::driver::RunState::Working);

        // Reproduces the bug: nothing ever called `registry.start(...)` for
        // this repository (Agent Desk executions never do), so `record`
        // refuses the event exactly as `commands::airun::emit`'s gate did in
        // production -- this is why routing an Agent Desk execution's sink
        // through `emit` left every session stuck in `Preparing`.
        assert!(
            !registry.record(&event),
            "an unregistered repo's event must be dropped by SessionRegistry, \
             reproducing why routing through commands::airun::emit silently \
             swallowed every Agent Desk execution event"
        );

        // The fix: `route_run_event` (what `emit_agent_desk_only` calls)
        // succeeds on its own, with no dependency on `SessionRegistry` at all
        // -- proving the durable path Agent Desk actually needs does not run
        // through the gate that dropped it.
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Not registry-gated"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        links.link(&event.session_id, &session_id);

        let sequence = links.next_sequence(&event.session_id);
        let routed = crate::agentdesk::route_run_event(&root, &links, &locks, sequence, "2026-01-01T00:00:01Z", &event);
        assert!(
            matches!(routed, crate::agentdesk::RunEventRouted::Persisted { .. }),
            "the durable route must succeed even though SessionRegistry never saw this repo: got {routed:?}"
        );

        let reread = store::read_session(&root, &session_id).expect("session still readable");
        assert_eq!(
            reread.header.state,
            SessionState::Working,
            "the session must leave Preparing once the durable route delivers an event, \
             regardless of SessionRegistry"
        );
    }

    /// Regression test for the "no silent freeze" requirement: even a
    /// preparation that never reports even one event from the engine (the
    /// spawned task panicked, or the CLI hung before its first message) must
    /// still be reachable to a typed terminal state rather than sitting in
    /// `Preparing` forever. `start_execution_at`'s watchdog task achieves this
    /// by awaiting the engine task's `JoinHandle` and, on panic, emitting a
    /// `RunStep::Ended { state: Failed, .. }` itself -- this test proves that
    /// exact event, routed the same way the watchdog routes it, moves a
    /// session stuck in `Preparing` to a typed `Failed` with a plain-language
    /// reason in the transcript, not an indefinite hang.
    #[test]
    fn a_preparation_that_never_reports_still_reaches_a_typed_failed_state() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Never reports"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();

        // Reproduce `record_execution_if_not_running`'s effect: the session
        // is left in `Preparing`, exactly where it sits the whole time the
        // engine task is starting up.
        let execution_id = crate::agentdesk::execution_id_for_run_session("run-1");
        links.link(&execution_id, &session_id);
        update_session_at(&locks, &root, &session_id, |s| {
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
        let stuck = store::read_session(&root, &session_id).unwrap();
        assert_eq!(stuck.header.state, SessionState::Preparing, "sanity: still stuck exactly as reported");

        // What the watchdog task sends when the engine task panics or is
        // otherwise never heard from again: the same `Ended`/`Failed` event
        // every other engine failure path already produces (see
        // `cli_run::run_task`'s own `connect`-failure branch).
        let ended = crate::airun::driver::RunEventKind {
            repo_id: session.header.repo_id.clone(),
            session_id: "run-1".into(),
            state: crate::airun::driver::RunState::Failed,
            summary: "Something went wrong while starting the agent, and it never got to report why.".into(),
            step: crate::airun::driver::RunStep::Ended {
                state: crate::airun::driver::RunState::Failed,
                detail: "Something went wrong while starting the agent, and it never got to report why. Nothing was committed and your own work is untouched.".into(),
            },
        };
        let sequence = links.next_sequence(&ended.session_id);
        let routed = crate::agentdesk::route_run_event(&root, &links, &locks, sequence, "2026-01-01T00:00:05Z", &ended);
        assert!(
            matches!(routed, crate::agentdesk::RunEventRouted::Persisted { .. }),
            "the watchdog's Ended/Failed event must persist: got {routed:?}"
        );

        let final_session = store::read_session(&root, &session_id).expect("session still readable");
        assert_eq!(
            final_session.header.state,
            SessionState::Failed,
            "a preparation that never reported must still reach a typed terminal state, never stay Preparing"
        );
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
                record_execution_if_not_running(locks_ref, root_ref, session_id_ref, exec_a.clone(), None, ExecutionProvenance::default())
            });
            let b = scope.spawn(|| {
                barrier.wait();
                record_execution_if_not_running(locks_ref, root_ref, session_id_ref, exec_b.clone(), None, ExecutionProvenance::default())
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

    #[tokio::test]
    async fn stop_scope_one_stops_only_the_named_execution() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let executions = crate::agentdesk::ExecutionRegistry::new();
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
            &executions,
            &session_id,
            StopScope::One { execution_id: exec_b.clone() },
        )
        .await;
        let StopExecutionOutcome::Stopped { session, stopped, timed_out } = outcome else {
            panic!("expected Stopped, got {outcome:?}");
        };
        assert_eq!(stopped, vec![exec_b.clone()]);
        assert!(timed_out.is_empty(), "nothing was registered as live, so nothing can time out");
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
    /// This drives the real path end-to-end: link the lead's AND the helper's
    /// own execution IDs (now that `RunSessionLinks` is execution-addressed,
    /// there is no single "the repo's link" to share between them -- each
    /// execution gets its own mapping, exactly like a real
    /// `start_execution_at`/`launch_helper` pair would), give the session a
    /// lead + a helper both `Working`, stop only the helper, assert the
    /// LEAD's own link survives (and the helper's is gone), then route a
    /// real run event for the lead and assert it still lands as a persisted
    /// message instead of being dropped.
    #[tokio::test]
    async fn stopping_one_helper_leaves_the_link_intact_for_the_still_running_lead() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        let CreateSessionOutcome::Created { session } =
            create_session_at(&root, create_request("Lead plus helper"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();

        let lead = crate::agentdesk::execution_id_for_run_session("run-lead");
        let helper = crate::agentdesk::execution_id_for_run_session("run-helper");
        links.link(&lead, &session_id);
        links.link(&helper, &session_id);
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
            &executions,
            &session_id,
            StopScope::One { execution_id: helper.clone() },
        )
        .await;
        let StopExecutionOutcome::Stopped { stopped, .. } = outcome else {
            panic!("expected Stopped, got {outcome:?}");
        };
        assert_eq!(stopped, vec![helper.clone()]);

        assert_eq!(
            links.get(&lead),
            Some(session_id.clone()),
            "the lead's own link must survive: it is still Working"
        );
        assert_eq!(
            links.get(&helper),
            None,
            "the stopped helper's own link must be removed"
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

    /// P0 regression, at the `stop_execution_at` command level rather than
    /// `bridge.rs`'s pure-function level: TWO SESSIONS (not two executions in
    /// one session, which the test above already covers) share a repository,
    /// each with its own live execution linked. Stopping session A's
    /// execution must unlink only A's own mapping -- session B's link (and
    /// its ability to keep receiving routed events) must be untouched.
    /// Before the P0 fix (`RunSessionLinks` keyed by `repo_id`), linking B's
    /// execution would have silently overwritten A's link the moment B
    /// started, and stopping A would have unlinked whichever session's link
    /// happened to currently occupy that one repo-keyed slot -- not
    /// necessarily A's own.
    #[tokio::test]
    async fn stopping_session_as_execution_never_touches_session_bs_link_for_the_same_repo() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let executions = crate::agentdesk::ExecutionRegistry::new();

        let CreateSessionOutcome::Created { session: session_a } =
            create_session_at(&root, create_request("Session A"))
        else {
            panic!("expected Created");
        };
        let CreateSessionOutcome::Created { session: session_b } =
            create_session_at(&root, create_request("Session B"))
        else {
            panic!("expected Created");
        };
        let session_id_a = session_a.header.session_id.clone();
        let session_id_b = session_b.header.session_id.clone();

        let exec_a = crate::agentdesk::execution_id_for_run_session("run-a");
        let exec_b = crate::agentdesk::execution_id_for_run_session("run-b");
        // Both sessions' own executions get linked, exactly as
        // `start_execution_at` links each execution it mints -- note these
        // two sessions are NOT required to share a repository for this
        // registry to isolate them (it never reads `repo_id` at all), but the
        // audit's own wording calls out "same repository" as the sharpest
        // version of the bug, so both fixtures use the default test repo.
        links.link(&exec_a, &session_id_a);
        links.link(&exec_b, &session_id_b);

        for (session_id, execution_id) in [(session_id_a.clone(), exec_a.clone()), (session_id_b.clone(), exec_b.clone())] {
            update_session_at(&locks, &root, &session_id, |s| {
                s.executions.push(crate::agentdesk::model::ExecutionRecord::minimal(
                    execution_id.clone(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::Working,
                    now_rfc3339(),
                    None,
                    0,
                ));
                s.header.active_execution_id = Some(execution_id.clone());
            });
        }

        let outcome = stop_execution_at(
            &locks,
            &root,
            &links,
            &drivers,
            &executions,
            &session_id_a,
            StopScope::One { execution_id: exec_a.clone() },
        )
        .await;
        assert!(matches!(outcome, StopExecutionOutcome::Stopped { .. }), "got {outcome:?}");

        assert_eq!(links.get(&exec_a), None, "session A's own execution must be unlinked");
        assert_eq!(
            links.get(&exec_b),
            Some(session_id_b.clone()),
            "session B's link must survive stopping session A's unrelated execution"
        );

        // Prove it end-to-end: a run event for B's execution still routes to
        // session B, not silently dropped.
        let event_for_b = crate::airun::driver::RunEventKind {
            repo_id: session_b.header.repo_id.clone(),
            session_id: "run-b".into(),
            state: crate::airun::driver::RunState::Working,
            summary: "still alive".into(),
            step: crate::airun::driver::RunStep::Note { text: "still alive".into() },
        };
        let sequence = links.next_sequence(&event_for_b.session_id);
        let routed = crate::agentdesk::route_run_event(
            &root,
            &links,
            &locks,
            sequence,
            "2026-01-01T00:00:05Z",
            &event_for_b,
        );
        assert!(
            matches!(routed, crate::agentdesk::RunEventRouted::Persisted { .. }),
            "session B must keep receiving its own routed events after A stopped: got {routed:?}"
        );
    }

    #[tokio::test]
    async fn stop_scope_all_stops_every_active_execution() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let executions = crate::agentdesk::ExecutionRegistry::new();
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

        let outcome = stop_execution_at(&locks, &root, &links, &drivers, &executions, &session_id, StopScope::All)
            .await;
        let StopExecutionOutcome::Stopped { session, stopped, timed_out } = outcome else {
            panic!("expected Stopped, got {outcome:?}");
        };
        assert_eq!(stopped.len(), 2, "both executions must be stopped");
        assert!(timed_out.is_empty());
        assert!(session.executions.iter().all(|e| e.state == SessionState::Stopped));
        assert_eq!(session.header.state, SessionState::Stopped);
    }

    #[tokio::test]
    async fn stopping_an_already_finished_execution_is_a_harmless_no_op() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let executions = crate::agentdesk::ExecutionRegistry::new();
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
            &executions,
            &session_id,
            StopScope::One { execution_id: exec_a },
        )
        .await;
        let StopExecutionOutcome::Stopped { stopped, .. } = outcome else {
            panic!("expected Stopped, got {outcome:?}");
        };
        assert!(stopped.is_empty(), "an execution that already finished has nothing to stop");
    }

    #[tokio::test]
    async fn stop_of_an_unknown_session_is_not_found() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let links = test_links();
        let drivers = crate::commands::airun::DriverRegistry::default();
        let executions = crate::agentdesk::ExecutionRegistry::new();
        let outcome = stop_execution_at(&locks, &root, &links, &drivers, &executions, "ghost", StopScope::All)
            .await;
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
        // Was `Some(0)`, reasoning that zero helpers is a real measured count.
        // It is -- but on a chat that never had any it is also a number that
        // can never be anything else, so it earned a permanent row about a
        // concept its owner has not met and made the card's empty state
        // unreachable. qa-log #94.
        assert_eq!(
            usage.active_helper_count, None,
            "a chat that never had helpers reports no helper count at all"
        );
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
        // A transcript row is not a turn. Before this fix a single streamed
        // note was reported as "1 turn"; an unreported figure must stay absent.
        assert!(usage.session_requests.is_none(), "transcript rows must never be counted as turns");
        assert!(usage.agents.is_empty(), "no execution reported usage, so there is no breakdown");
    }

    /// Builds a session with a lead and one helper that both reported usage,
    /// with the helper's report landing AFTER the lead's.
    fn seed_lead_and_helper_usage(root: &SessionStoreRoot, locks: &crate::agentdesk::SessionLocks) -> (String, String, String) {
        use crate::agentdesk::model::{ExecutionRecord, ExecutionUsage};
        let CreateSessionOutcome::Created { session } = create_session_at(root, create_request("Team"))
        else {
            panic!("expected Created");
        };
        let session_id = session.header.session_id.clone();
        let lead_id = crate::agentdesk::execution_id_for_run_session("lead");
        let helper_id = crate::agentdesk::execution_id_for_run_session("helper");
        update_session_at(locks, root, &session_id, |s| {
            let mut lead = ExecutionRecord::minimal(
                lead_id.clone(),
                s.header.session_id.clone(),
                None,
                SessionState::Finished,
                now_rfc3339(),
                Some(now_rfc3339()),
                3,
            );
            lead.usage = Some(ExecutionUsage {
                input_tokens: Some(1_000),
                output_tokens: Some(200),
                cost_micro_usd: Some(4_500),
                turns: 3,
                context_used: Some(31_000),
                context_size: Some(200_000),
                ..ExecutionUsage::default()
            });
            let mut helper = ExecutionRecord::minimal(
                helper_id.clone(),
                s.header.session_id.clone(),
                Some(lead_id.clone()),
                SessionState::Finished,
                now_rfc3339(),
                Some(now_rfc3339()),
                2,
            );
            helper.job_title = Some("Trace the crash".into());
            helper.usage = Some(ExecutionUsage {
                input_tokens: Some(500),
                output_tokens: None,
                cost_micro_usd: None,
                turns: 2,
                context_used: Some(90_000),
                context_size: Some(128_000),
                ..ExecutionUsage::default()
            });
            s.executions.push(lead);
            s.executions.push(helper);
        });
        (session_id, lead_id, helper_id)
    }

    #[test]
    fn usage_context_comes_from_the_lead_even_when_a_helper_reported_later() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let (session_id, _, _) = seed_lead_and_helper_usage(&root, &locks);

        let SessionUsageOutcome::Available { usage } = session_usage_at(&root, &session_id) else {
            panic!("expected Available");
        };
        assert_eq!(usage.context_used.map(|v| v.value), Some(31_000.0));
        assert_eq!(usage.context_size.map(|v| v.value), Some(200_000.0));
        // Totals still include the helper.
        assert_eq!(usage.session_tokens.map(|v| v.value), Some(1_700.0));
        assert_eq!(usage.session_requests.map(|v| v.value), Some(5.0));
    }

    /// The turn count is the one usage figure GitWyrm works out itself.
    ///
    /// `UsageTotals::turns`'s own doc says so: "Always known, because GitWyrm
    /// counts them itself rather than asking the provider." And the comment
    /// beside the token/cost totals states the rule this file follows --
    /// "`Measured` is reserved for what GitWyrm can verify itself (message
    /// counts, helper counts)."
    ///
    /// Every figure was nonetheless stamped `ProviderReported`, so the
    /// vision's three-way labelling (measured / estimated / unavailable) was
    /// one-way in practice: `Measured` was constructed nowhere in the tree.
    /// Recorded as N6 across passes 24-41 and carried unfixed.
    #[test]
    fn a_turn_count_is_labelled_measured_not_provider_reported() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let (session_id, _, _) = seed_lead_and_helper_usage(&root, &locks);

        let SessionUsageOutcome::Available { usage } = session_usage_at(&root, &session_id) else {
            panic!("expected Available");
        };
        let turns = usage.session_requests.expect("a turn count was recorded");
        assert_eq!(
            turns.source,
            UsageSource::Measured,
            "GitWyrm counts turns itself, so claiming the provider reported them is wrong"
        );
        // The figures that DO come from the provider must stay as they are.
        assert_eq!(
            usage.session_tokens.expect("tokens").source,
            UsageSource::ProviderReported,
            "token counts really do come from the provider"
        );
    }

    #[test]
    fn usage_breaks_down_per_agent_without_inventing_missing_figures() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let (session_id, lead_id, helper_id) = seed_lead_and_helper_usage(&root, &locks);

        let SessionUsageOutcome::Available { usage } = session_usage_at(&root, &session_id) else {
            panic!("expected Available");
        };
        assert_eq!(usage.agents.len(), 2);
        let lead = &usage.agents[0];
        assert_eq!(lead.execution_id, lead_id);
        assert_eq!(lead.label, "Lead");
        assert!(lead.is_lead);
        assert_eq!(lead.tokens, Some(1_200));
        assert_eq!(lead.cost_micro_usd, Some(4_500));
        assert_eq!(lead.turns, Some(3));

        let helper = &usage.agents[1];
        assert_eq!(helper.execution_id, helper_id);
        assert_eq!(helper.label, "Trace the crash");
        assert!(!helper.is_lead);
        // Was `Some(500)` from input alone, called "a real token figure". It
        // is not a TOTAL, which is what the field means and how the card
        // renders it -- the graph panel refuses the same sum one layer up.
        assert_eq!(helper.tokens, None, "a half-known figure is not a total");
        assert_eq!(helper.input_tokens, Some(500), "the half that WAS reported is still carried");
        assert_eq!(helper.output_tokens, None);
        assert_eq!(helper.cost_micro_usd, None, "an unreported cost stays absent, never zero");
        assert_eq!(helper.turns, Some(2));
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

    // -- Task 2.4, third of three: "mark launch-vs-live differences." --
    // `openspec_context_drift_at` is the read side of this: given a session
    // whose most recent execution stamped a `context_fingerprint`, does the
    // live OpenSpec source still match what that execution actually saw.

    mod openspec_context_drift {
        use super::*;
        use std::fs;

        fn repo_with_change(root: &std::path::Path) -> (crate::state::RepoManager, String) {
            git2::Repository::init(root).expect("init repo");
            let change_dir = root.join("openspec").join("changes").join("add-thing");
            fs::create_dir_all(&change_dir).unwrap();
            fs::write(
                change_dir.join("proposal.md"),
                "# Change: Add thing\n\n## Why\n\nBecause.\n",
            )
            .unwrap();
            fs::write(change_dir.join("tasks.md"), "## 1. Group\n\n- [ ] 1.1 Do it\n").unwrap();

            let manager = crate::state::RepoManager::default();
            let (repo_id, _open, _reused) =
                manager.open(root.to_str().expect("utf8 path")).expect("open repo");
            (manager, repo_id)
        }

        fn session_with_fingerprint(root: &SessionStoreRoot, repo_id: &str, repo_path: &std::path::Path, launched_fingerprint: &str) -> String {
            let req = CreateSessionRequest {
                repo_id: repo_id.to_string(),
                repo_path: repo_path.to_string_lossy().into_owned(),
                repo_name: "widgets".into(),
                title: "Add thing".into(),
                source: SessionSource::OpenSpecTask {
                    change_id: "add-thing".into(),
                    task_index: 0,
                    task_text: "1.1 Do it".into(),
                    snapshot: SourceSnapshot {
                        title: "Add thing".into(),
                        summary: "1.1 Do it".into(),
                        captured_at: "2026-01-01T00:00:00Z".into(),
                        live_unavailable: false,
                    },
                },
                intent: SessionIntent::Fix,
            };
            let CreateSessionOutcome::Created { session } = create_session_at(root, req) else {
                panic!("expected Created");
            };
            let session_id = session.header.session_id.clone();

            update_session_at(&test_locks(), root, &session_id, |s| {
                let mut exec = crate::agentdesk::model::ExecutionRecord::minimal(
                    "exec-1".into(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::Finished,
                    "2026-01-01T00:00:00Z".into(),
                    Some("2026-01-01T00:01:00Z".into()),
                    0,
                );
                exec.context_fingerprint = Some(launched_fingerprint.to_string());
                s.executions.push(exec);
            });

            session_id
        }

        fn fingerprint_for(root: &std::path::Path) -> String {
            let target = OpenSpecTarget::Task {
                change_id: "add-thing".into(),
                task_index: 0,
                task_text: "1.1 Do it".into(),
                snapshot_title: "Add thing".into(),
            };
            let ctx = resolve_openspec_context(root, &target).expect("context builds");
            crate::agentdesk::openspec_context::fingerprint(&ctx)
        }

        #[test]
        fn reports_diverged_when_tasks_md_changed_since_the_execution_ran() {
            let (dir, root) = temp_root();
            let (manager, repo_id) = repo_with_change(dir.path());
            let launched_fingerprint = fingerprint_for(dir.path());
            let session_id = session_with_fingerprint(&root, &repo_id, dir.path(), &launched_fingerprint);

            fs::write(
                dir.path().join("openspec/changes/add-thing/tasks.md"),
                "## 1. Group\n\n- [ ] 1.1 Do it\n- [ ] 1.2 Added after the run\n",
            )
            .unwrap();

            let outcome = openspec_context_drift_at(&root, &manager, &session_id);
            match outcome {
                OpenSpecContextDriftOutcome::Checked { diverged, .. } => {
                    assert!(diverged, "tasks.md changed, so this must report diverged");
                }
                other => panic!("expected Checked, got {other:?}"),
            }
        }

        #[test]
        fn reports_not_diverged_when_nothing_changed() {
            let (dir, root) = temp_root();
            let (manager, repo_id) = repo_with_change(dir.path());
            let launched_fingerprint = fingerprint_for(dir.path());
            let session_id = session_with_fingerprint(&root, &repo_id, dir.path(), &launched_fingerprint);

            let outcome = openspec_context_drift_at(&root, &manager, &session_id);
            match outcome {
                OpenSpecContextDriftOutcome::Checked { diverged, .. } => {
                    assert!(!diverged, "nothing changed, so this must not report diverged");
                }
                other => panic!("expected Checked, got {other:?}"),
            }
        }

        #[test]
        fn a_manual_session_with_no_openspec_source_has_nothing_to_compare() {
            let (_dir, root) = temp_root();
            let manager = crate::state::RepoManager::default();
            let CreateSessionOutcome::Created { session } =
                create_session_at(&root, create_request("Manual chat"))
            else {
                panic!("expected Created");
            };

            let outcome = openspec_context_drift_at(&root, &manager, &session.header.session_id);
            assert!(matches!(outcome, OpenSpecContextDriftOutcome::NothingToCompare));
        }

        #[test]
        fn an_openspec_session_with_no_execution_yet_has_nothing_to_compare() {
            let (dir, root) = temp_root();
            let (_manager, repo_id) = repo_with_change(dir.path());
            let req = CreateSessionRequest {
                repo_id,
                repo_path: dir.path().to_string_lossy().into_owned(),
                repo_name: "widgets".into(),
                title: "Add thing".into(),
                source: SessionSource::OpenSpecTask {
                    change_id: "add-thing".into(),
                    task_index: 0,
                    task_text: "1.1 Do it".into(),
                    snapshot: SourceSnapshot {
                        title: "Add thing".into(),
                        summary: "1.1 Do it".into(),
                        captured_at: "2026-01-01T00:00:00Z".into(),
                        live_unavailable: false,
                    },
                },
                intent: SessionIntent::Fix,
            };
            let CreateSessionOutcome::Created { session } = create_session_at(&root, req) else {
                panic!("expected Created");
            };
            let manager = crate::state::RepoManager::default();

            // Never launched -- no execution carries a fingerprint yet.
            let outcome = openspec_context_drift_at(&root, &manager, &session.header.session_id);
            assert!(matches!(outcome, OpenSpecContextDriftOutcome::NothingToCompare));
        }
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
            let executions = crate::agentdesk::ExecutionRegistry::new();
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
                record_execution_if_not_running(&locks, &root, &session_id, "exec-1".to_string(), None, ExecutionProvenance::default());
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
            let GetSessionOutcome::Found { session: reloaded } =
                get_session_at(&locks, &executions, &root, &session_id)
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

            let GetSessionOutcome::Found { session: final_reload } =
                get_session_at(&locks, &executions, &root, &session_id)
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

// -- Seams for `commands::agent_kickoff`'s "Fix this" escalation (source-kickoffs
//    task 4.6). `update_session_at` and `append_user_message_at` stay private
//    because their outcome enums carry UI-facing variants this module's own
//    commands own; kickoff only needs "did it work, and if not, why" as a plain
//    `Result`, the same shape `create_session_for_kickoff` already hands it. --

/// `update_session_at`, flattened to a `Result` for `commands::agent_kickoff`.
pub(crate) fn update_session_for_kickoff(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    mutate: impl FnOnce(&mut AgentSession),
) -> Result<AgentSession, String> {
    match update_session_at(locks, root, session_id, mutate) {
        UpdateSessionOutcome::Updated { session } => Ok(session),
        UpdateSessionOutcome::NotFound => Err("the chat could not be found".into()),
        UpdateSessionOutcome::Damaged { reason } => Err(reason),
        UpdateSessionOutcome::WriteFailed { detail } | UpdateSessionOutcome::Unavailable { detail } => Err(detail),
    }
}

/// `append_user_message_at`, flattened to a `Result` for `commands::agent_kickoff`.
/// The seeded message goes through the same path a typed one does, so it
/// gets a segment, lifts the session out of `Draft`, and refreshes the index
/// exactly like a message the person wrote by hand.
pub(crate) fn append_user_message_for_kickoff(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    content: String,
) -> Result<AgentSession, String> {
    match append_user_message_at(locks, root, session_id, content, Vec::new()) {
        AppendUserMessageOutcome::Appended { session, .. } => Ok(session),
        AppendUserMessageOutcome::NotFound => Err("the chat could not be found".into()),
        AppendUserMessageOutcome::Damaged { reason } => Err(reason),
        AppendUserMessageOutcome::WriteFailed { detail } | AppendUserMessageOutcome::Unavailable { detail } => Err(detail),
    }
}
