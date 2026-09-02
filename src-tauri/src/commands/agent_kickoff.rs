//! The one shared kickoff request every source surface (issue, pull request,
//! OpenSpec change/task, ...) uses to start or focus an Agent Desk session.
//!
//! `openspec/changes/agent-desk-source-kickoffs`, architecture.md section 8:
//! "All UI actions call one frontend request shape." This module is that
//! shape's backend half, plus the duplicate-session lookup (task 1.4) that
//! makes re-clicking Fix on the same issue focus the existing session rather
//! than fork a second one (design.md "Deduplication").
//!
//! Deliberately a separate file from `commands::agent_desk`, which already
//! carries the six foundation commands (create/list/get/rename/archive/
//! mark-read) plus start/stop-execution/usage/refresh-source at over 2500
//! lines. Kickoff is layered strictly on top of that module's
//! `CreateSessionRequest` and `start_execution_at` -- it does not reimplement
//! session creation or execution start, it only adds "build the right
//! `CreateSessionRequest` from a typed source input" and "check for an
//! existing session first."

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

use crate::agentdesk::model::{
    AgentSession, AgentSessionHeader, SessionIntent, SessionSource, SessionState,
};
use crate::agentdesk::policy::{self, ExecutionMode, ExecutionTeam};
use crate::agentdesk::store::{self, SessionStoreRoot};
use crate::commands::spec_desk::AGENT_DESK_LABEL;
use crate::error::AppError;

use super::agent_desk::{CreateSessionRequest, StartExecutionOutcome};

/// Emitted at the Agent Desk window right after `agent_session_start`
/// resolves, naming the exact session it should show -- R3.3: "Send a
/// targeted select-session event after the session exists; do not depend on
/// another window's query invalidation."
///
/// A sibling of `commands::spec_desk::SELECT_DESK_TARGET_EVENT`, not a
/// replacement: that event answers "which repository/change" for the
/// window's very first paint (URL-seeded) and any later repo switch. This
/// one answers "which session, right now" for the far more common case
/// where the Desk is already open on the right repository and kickoff just
/// needs to land the click on the exact session it created or focused,
/// without waiting on `agentSessionsAll` to invalidate, refetch, and have
/// `AgentDeskView`'s "land on the most recent session" effect happen to
/// guess correctly -- that effect only fires when NO pane has a session
/// yet, so it does nothing at all once any chat has ever been opened.
pub const SELECT_SESSION_EVENT: &str = "agent-desk://select-session";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectSessionTarget {
    pub session_id: String,
}

/// Best-effort: a Desk window that is not open yet has no listener, and
/// `useStartAgentSession` already awaits `openSpecDesk` (which creates or
/// focuses the window) before this fires, so the common race -- event
/// arriving before the window has a subscriber -- is already covered by that
/// ordering. A window that genuinely does not exist yet just does not
/// receive this; kickoff does not fail because of it.
fn emit_select_session(app: &AppHandle, session_id: &str) {
    let _ = app.emit_to(
        AGENT_DESK_LABEL,
        SELECT_SESSION_EVENT,
        &SelectSessionTarget {
            session_id: session_id.to_string(),
        },
    );
}

/// Everything a source surface (issue row, PR row, OpenSpec task, ...) needs
/// to say "start an Agent Desk session for this" -- architecture.md section
/// 8's `StartAgentSessionRequest`, given a Rust/Specta home.
///
/// `mode`/`team`/`provider_override` are optional: when omitted, the
/// intent's policy default (`agentdesk::policy::for_intent`) is used, so a
/// caller that just wants "Fix with AI" does not have to know Fix defaults
/// to Auto + Lead.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartAgentSessionRequest {
    pub repo_id: String,
    pub repo_path: String,
    pub repo_name: String,
    pub source: SessionSourceInput,
    pub intent: SessionIntent,
    pub mode: Option<ExecutionMode>,
    pub team: Option<ExecutionTeam>,
    pub provider_override: Option<String>,
}

/// What a source surface actually knows at click time, before any network
/// round-trip -- architecture.md section 8: "Create from known row data,
/// then enrich in the Desk." This is intentionally NOT the same type as
/// [`SessionSource`]: that type carries a [`crate::agentdesk::model::SourceSnapshot`]
/// with a `captured_at` timestamp and a `live_unavailable` flag that only the
/// backend should stamp, so a frontend cannot construct a source that lies
/// about when it was captured or claims to already know the live source is
/// gone. `into_source_and_title` is where a `SessionSourceInput` becomes a
/// real [`SessionSource`], stamping the snapshot itself.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionSourceInput {
    #[serde(rename_all = "camelCase")]
    Manual,
    #[serde(rename_all = "camelCase")]
    Issue {
        host_id: String,
        owner: String,
        repo: String,
        number: u32,
        url: String,
        title: String,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    PullRequest {
        host_id: String,
        owner: String,
        repo: String,
        number: u32,
        url: String,
        head: String,
        base: String,
        title: String,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    OpenSpecChange {
        change_id: String,
        title: String,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    OpenSpecTask {
        change_id: String,
        task_index: u32,
        task_text: String,
        title: String,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    Commit {
        oid: String,
        title: String,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    Diff {
        scope: String,
        paths: Vec<String>,
        title: String,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    WorkingChanges {
        paths: Vec<String>,
        title: String,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    CheckFailure {
        provider: String,
        check_id: String,
        url: Option<String>,
        title: String,
        summary: String,
    },
}

/// Now, RFC 3339 UTC -- mirrors `commands::agent_desk::now_rfc3339`. Kept as
/// its own copy rather than making that function `pub(crate)` and importing
/// it: this module's stamping of `captured_at` is a distinct responsibility
/// (a request-shape decision, task 1.1) from that module's stamping of
/// `created_at`/`updated_at`, and duplicating four lines is cheaper than
/// coupling the two modules' visibility on a helper this small.
fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

impl SessionSourceInput {
    /// Builds the real [`SessionSource`] (stamping a fresh
    /// [`crate::agentdesk::model::SourceSnapshot`]) plus the title a new
    /// session's header should use.
    fn into_source_and_title(self, repo_id: &str) -> (SessionSource, String) {
        use crate::agentdesk::model::SourceSnapshot;
        let captured_at = now_rfc3339();
        let snapshot = |title: &str, summary: String| SourceSnapshot {
            title: title.to_string(),
            summary,
            captured_at: captured_at.clone(),
            live_unavailable: false,
        };

        match self {
            SessionSourceInput::Manual => (
                SessionSource::Manual {
                    repo_id: repo_id.to_string(),
                },
                String::new(),
            ),
            SessionSourceInput::Issue {
                host_id,
                owner,
                repo,
                number,
                url,
                title,
                summary,
            } => {
                let header_title = title.clone();
                (
                    SessionSource::Issue {
                        host_id,
                        owner,
                        repo,
                        number,
                        url,
                        snapshot: snapshot(&title, summary),
                    },
                    header_title,
                )
            }
            SessionSourceInput::PullRequest {
                host_id,
                owner,
                repo,
                number,
                url,
                head,
                base,
                title,
                summary,
            } => {
                let header_title = title.clone();
                (
                    SessionSource::PullRequest {
                        host_id,
                        owner,
                        repo,
                        number,
                        url,
                        head,
                        base,
                        snapshot: snapshot(&title, summary),
                    },
                    header_title,
                )
            }
            SessionSourceInput::OpenSpecChange {
                change_id,
                title,
                summary,
            } => {
                let header_title = title.clone();
                (
                    SessionSource::OpenSpecChange {
                        change_id,
                        snapshot: snapshot(&title, summary),
                    },
                    header_title,
                )
            }
            SessionSourceInput::OpenSpecTask {
                change_id,
                task_index,
                task_text,
                title,
                summary,
            } => {
                let header_title = title.clone();
                (
                    SessionSource::OpenSpecTask {
                        change_id,
                        task_index,
                        task_text,
                        snapshot: snapshot(&title, summary),
                    },
                    header_title,
                )
            }
            SessionSourceInput::Commit { oid, title, summary } => {
                let header_title = title.clone();
                (
                    SessionSource::Commit {
                        oid,
                        snapshot: snapshot(&title, summary),
                    },
                    header_title,
                )
            }
            SessionSourceInput::Diff {
                scope,
                paths,
                title,
                summary,
            } => {
                let header_title = title.clone();
                (
                    SessionSource::Diff {
                        scope,
                        paths,
                        snapshot: snapshot(&title, summary),
                    },
                    header_title,
                )
            }
            SessionSourceInput::WorkingChanges {
                paths,
                title,
                summary,
            } => {
                let header_title = title.clone();
                (
                    SessionSource::WorkingChanges {
                        paths,
                        snapshot: snapshot(&title, summary),
                    },
                    header_title,
                )
            }
            SessionSourceInput::CheckFailure {
                provider,
                check_id,
                url,
                title,
                summary,
            } => {
                let header_title = title.clone();
                (
                    SessionSource::CheckFailure {
                        provider,
                        check_id,
                        url,
                        snapshot: snapshot(&title, summary),
                    },
                    header_title,
                )
            }
        }
    }
}

/// What kickoff found or did. Every branch is something the frontend's
/// `useStartAgentSession` (task 2.1) can react to without inventing its own
/// state machine -- `FocusedExisting` in particular is what makes double-Fix
/// (spec `Duplicate active work`) show the existing session instead of a
/// silently-forked second one.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartAgentSessionOutcome {
    /// A brand-new session was created for this source/intent, and this
    /// command already attempted to run its first turn -- P1-A ("source
    /// clicks do not all do what they say"): EVERY explicit source action
    /// starts immediately, not just the ones whose intent happens to allow
    /// writes. `start` is that attempt's own outcome: `Some(Started {..})`
    /// once execution is genuinely running (Ask/Explain/Review/Summarize
    /// included -- those intents run and can respond, they simply cannot
    /// reach a write tool, per `agentdesk::policy::IntentPolicy::can_write`
    /// -- authority is a capability check inside the run, never a gate on
    /// whether the run happens at all). A non-`Started` variant inside
    /// `Some` (e.g. `SourceMissing`, `AdapterUnsupported`) means the session
    /// exists and is visible, but its first turn could not begin; the
    /// caller should surface that reason (`agentDeskResult::explainAutoStartOutcome`
    /// already knows how) rather than silently leaving the session sitting
    /// in `Draft`. `start` is only ever `None` for a request whose intent's
    /// own `mode`/`team`/`provider_override` could not even be resolved
    /// before the session was created -- today that never happens (session
    /// creation has no policy dependency), so this exists for forward
    /// compatibility rather than a real path in this build.
    Created {
        session: AgentSession,
        start: Option<StartExecutionOutcome>,
    },
    /// An active (non-finished/failed/stopped) session already exists for
    /// this exact repo/source-identity/intent -- design.md: "An active
    /// session with the same repo/source/intent is focused and explained."
    /// No new session was created and no execution is (re)started here: the
    /// existing session may already be running, or may be exactly the
    /// `Draft`/`Ready` session a near-simultaneous first click already
    /// triggered a start for -- starting a second execution on top of it
    /// would either be refused as `AlreadyRunning` or, worse, race the
    /// first attempt. The caller should select `session` and tell the user
    /// why.
    FocusedExisting { session: AgentSession },
    WriteFailed { detail: String },
}

/// Every session state that counts as "still active" for duplicate
/// detection -- the same set `start_execution_at` treats as "an execution is
/// already running" in `commands::agent_desk`, plus `Draft`/`Ready`: a
/// session that has not started its execution yet is still the same
/// in-flight request as far as "don't fork a second one for this click" is
/// concerned. Only the terminal states (`Finished`, `Failed`, `Stopped`) and
/// `MissingSource` are excluded -- design.md: "A finished session may be
/// reopened or a new session explicitly started."
fn is_active(state: SessionState) -> bool {
    matches!(
        state,
        SessionState::Draft
            | SessionState::Preparing
            | SessionState::Ready
            | SessionState::Working
            | SessionState::NeedsInput
    )
}

/// Task 1.4: finds an active session for this repo whose source has the same
/// [`SessionSource::identity_key`] and the same intent as `source`/`intent`.
///
/// A full scan of this repo's headers rather than an index lookup by key --
/// matching `commands::agent_desk`'s stance that session counts are "dozens
/// per day" (architecture.md section 2), so this is not a hot path worth a
/// second index.
pub fn find_active_session_for_source(
    root: &SessionStoreRoot,
    repo_id: &str,
    source: &SessionSource,
    intent: SessionIntent,
) -> Option<AgentSessionHeader> {
    let target_key = source.identity_key();
    let loaded = store::load_or_rebuild_index(root);
    loaded
        .headers
        .into_iter()
        .filter(|h| !h.archived)
        .filter(|h| h.repo_id == repo_id)
        .filter(|h| h.intent == intent)
        .filter(|h| is_active(h.state))
        .find(|h| h.source.identity_key() == target_key)
}

fn resolve_root(app: &AppHandle) -> Result<SessionStoreRoot, AppError> {
    SessionStoreRoot::resolve(app).map_err(|e| AppError::Other(e.to_string()))
}

/// P1-A: what mode/team a freshly created session's first execution should
/// actually run under -- the request's own explicit choice when it named
/// one, the intent's policy default otherwise. Pulled out as its own pure
/// function (not inlined into `agent_session_start`'s async body) so a test
/// can prove the override survives without needing Tauri state or a real
/// `CliAgent` -- this is the exact value that used to be silently discarded:
/// `StartAgentSessionRequest.mode`/`.team` were accepted into the request
/// and then never read by anything, because the old frontend auto-start call
/// always passed the intent's OWN policy default and a hardcoded `null`
/// override, regardless of what a "Fix with <mode>" caller had asked for.
fn resolve_kickoff_execution_params(
    intent: SessionIntent,
    mode: Option<ExecutionMode>,
    team: Option<ExecutionTeam>,
) -> (ExecutionMode, ExecutionTeam) {
    let intent_policy = policy::for_intent(intent);
    (
        mode.unwrap_or(intent_policy.default_mode),
        team.unwrap_or(intent_policy.default_team),
    )
}

/// The find-existing-or-create half of kickoff, kept separate from actually
/// starting an execution: creation is a fast, local, disk-only step
/// (`architecture.md` section 8's step (c)), and the caller
/// (`agent_session_start`) needs to know precisely which case happened --
/// `Created` (attempt to start a fresh execution, carrying this exact
/// request's `mode`/`team`/`provider_override`) vs `Focused` (never attempt
/// a start: an active session already owns this repo/source/intent, and it
/// may already be running or already mid-start from a near-simultaneous
/// click).
#[derive(Debug)]
enum FoundOrCreatedSession {
    Created { session: AgentSession },
    Focused { session: AgentSession },
    WriteFailed { detail: String },
}

fn find_or_create_agent_session_at(
    root: &SessionStoreRoot,
    request: StartAgentSessionRequest,
) -> FoundOrCreatedSession {
    let (source, title) = request.source.into_source_and_title(&request.repo_id);

    if let Some(existing) =
        find_active_session_for_source(root, &request.repo_id, &source, request.intent)
    {
        match store::read_session(root, &existing.session_id) {
            Ok(session) => return FoundOrCreatedSession::Focused { session },
            Err(_) => {
                // The header was in the index but the file itself could not
                // be read right now (a transient lock, most likely on
                // Windows) -- fall through and create a new session rather
                // than blocking kickoff on a read that may recover a moment
                // later. This is a availability trade-off, not a
                // correctness one: worst case is a duplicate session next to
                // a temporarily-unreadable one, which the user can merge or
                // ignore, versus kickoff refusing to do anything at all.
            }
        }
    }

    let create_request = CreateSessionRequest {
        repo_id: request.repo_id,
        repo_path: request.repo_path,
        repo_name: request.repo_name,
        title,
        source,
        intent: request.intent,
    };

    match super::agent_desk::create_session_for_kickoff(root, create_request) {
        Ok(session) => FoundOrCreatedSession::Created { session },
        Err(detail) => FoundOrCreatedSession::WriteFailed { detail },
    }
}

/// P1-A ("source clicks do not all do what they say" / "one-click source
/// execution"): the single kickoff entry point every source surface calls,
/// now responsible end to end for BOTH durably creating the session AND
/// starting its first turn -- not split across a backend create step and a
/// frontend-side "should I also start this" decision the way
/// `useStartAgentSession.autoStartIfWriteCapable` used to work.
///
/// That frontend gate was the actual bug: it read `IntentPolicy::canWrite`
/// and skipped starting altogether for Review/Summarize (and Ask/Explain),
/// so clicking "Review with AI" created a session that then just sat in
/// `Draft` forever -- the click LOOKED like it worked (a session appeared)
/// but nothing ever ran. The fix is not to grant those intents write
/// authority (they must stay hard-refused any write tool, enforced entirely
/// by `agentdesk::policy::check_tool_capability`, independent of this
/// function) -- it is to stop conflating "can this intent ever write" with
/// "should this session's first turn run at all." Every intent's first turn
/// runs; only some of them are ever allowed to write once running.
///
/// This also fixes "kickoff provider overrides are passed into session
/// creation, then automatic start uses no override": `request.mode`/`.team`/
/// `.provider_override` used to be accepted into `StartAgentSessionRequest`
/// and then silently dropped -- `CreateSessionRequest` (session creation)
/// has no such fields, and the OLD frontend auto-start call
/// (`commands.agentSessionStartExecution(id, policy.defaultMode,
/// policy.defaultTeam, null)`) always passed the INTENT'S policy default and
/// a hardcoded `null` override, never what the user actually picked at
/// kickoff. Here, the exact same request that named the override is what
/// starts the execution -- `request.mode.unwrap_or(intent default)`,
/// `request.team.unwrap_or(intent default)`, and `request.provider_override`
/// verbatim -- so a "Fix with <provider>" click's choice survives all the
/// way into the first `cli_run::run_task` call, not just into the session
/// header.
///
/// Deliberately does NOT start anything for `FocusedExisting`: that session
/// may already be running (starting a second execution on it would be
/// refused as `AlreadyRunning`, which is correct but pointless to attempt),
/// or may itself be mid-start from the click that created it -- either way,
/// this is not this call's session to start.
#[tauri::command]
#[specta::specta]
pub async fn agent_session_start(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    links: tauri::State<'_, crate::agentdesk::RunSessionLinks>,
    manager: tauri::State<'_, crate::state::RepoManager>,
    executions: tauri::State<'_, crate::agentdesk::ExecutionRegistry>,
    request: StartAgentSessionRequest,
) -> Result<StartAgentSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let root_for_find = root.clone();
    let request_for_find = request.clone();
    let found = tauri::async_runtime::spawn_blocking(move || {
        find_or_create_agent_session_at(&root_for_find, request_for_find)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

    let outcome = match found {
        FoundOrCreatedSession::Focused { session } => StartAgentSessionOutcome::FocusedExisting { session },
        FoundOrCreatedSession::WriteFailed { detail } => StartAgentSessionOutcome::WriteFailed { detail },
        FoundOrCreatedSession::Created { session } => {
            // Step (c)/(d) of architecture.md section 8, now performed in
            // the SAME command rather than left to a separate frontend
            // call: the session is durable the instant `Created` above
            // returned, so starting its execution here (still inside this
            // one `agent_session_start` invocation, still before the
            // frontend's `await` resolves for a fast path, but never
            // blocking the eventual return on a slow provider/worktree --
            // see below) is what makes "every explicit source action starts
            // immediately" literally true rather than a UI-side promise
            // that could drift from what the backend actually does.
            let session_id = session.header.session_id.clone();
            let (mode, team) = resolve_kickoff_execution_params(request.intent, request.mode, request.team);
            let provider_override = request.provider_override.clone();
            // Synchronous, not `spawn_blocking`: matches
            // `agent_session_start_execution`'s own call to this exact
            // function (see that command's doc comment on why -- the slow
            // work `start_execution_at` does is a shell-out inside
            // `CliAgent::discover`, which it already isolates into its own
            // `tauri::async_runtime::spawn`'d task before returning
            // `Started`; nothing about the caller needing a blocking thread
            // pool of its own).
            let locks_arc = locks.inner().clone();
            let start = super::agent_desk::start_execution_at(
                &app,
                &locks_arc,
                &root,
                links.inner(),
                manager.inner(),
                executions.inner(),
                &session_id,
                mode,
                team,
                provider_override,
            );
            StartAgentSessionOutcome::Created {
                session,
                start: Some(start),
            }
        }
    };

    // R3.3: tell the Desk window exactly which session to select, the moment
    // it exists -- whether it was freshly created or an existing one was
    // focused (double-Fix must land on the same session just as surely as a
    // first Fix does). Fires regardless of whether the start attempt above
    // succeeded: the session itself is real and visible either way, and the
    // user needs to land on it to see why a start failed just as much as to
    // watch one succeed.
    match &outcome {
        StartAgentSessionOutcome::Created { session, .. }
        | StartAgentSessionOutcome::FocusedExisting { session } => {
            emit_select_session(&app, &session.header.session_id);
        }
        StartAgentSessionOutcome::WriteFailed { .. } => {}
    }

    Ok(outcome)
}

/// The intent policy table (`agentdesk::policy::for_intent`), exposed to the
/// frontend so `useStartAgentSession` and the source menus can show correct
/// mode/team defaults and know up front whether an intent can ever write,
/// without hand-copying architecture.md section 9's table into TypeScript.
#[tauri::command]
#[specta::specta]
pub fn agent_intent_policy(intent: SessionIntent) -> policy::IntentPolicy {
    policy::for_intent(intent)
}

// -- Task 4.6: escalate a read-only review into a Fix chat --
//
// A Review/Explain/Summarize/Ask chat can never reach a write tool
// (`agentdesk::policy::IntentPolicy::can_write`), and that stays true here:
// escalation does not grant the review session anything. It opens a NEW
// session with `SessionIntent::Fix`, on the same repo and the same source,
// and seeds its first user message with the review's conclusion so the person
// does not have to re-explain what was found. Nothing is started -- the
// person reads the seeded message and presses Send themselves.

/// What escalating a review to a fix found or did.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EscalateToFixOutcome {
    /// A new Fix session exists, seeded and waiting for Send. Not started.
    Created { session: AgentSession },
    NotFound,
    /// The session's intent can already write (Fix/Plan), so there is nothing
    /// to escalate -- the caller should not have offered the button.
    NotAReview { intent: SessionIntent },
    /// The review never produced a conclusion to carry over: no assistant
    /// message with any text in it.
    NothingToFix { detail: String },
    /// The review could not be read, or the new session could not be written.
    Failed { detail: String },
}

/// Which intents may be escalated: exactly the read-only ones. Mirrors the
/// frontend's `canEscalateToFix` in `src/lib/agentDeskResult.ts`; the backend
/// check is the one that actually refuses, the frontend one only hides the
/// button.
pub fn can_escalate_to_fix(intent: SessionIntent) -> bool {
    matches!(
        intent,
        SessionIntent::Ask | SessionIntent::Explain | SessionIntent::Review | SessionIntent::Summarize
    )
}

/// The new Fix session's title, from the review's. A review titled from its
/// source ("Issue #12: login fails") becomes "Fix: Issue #12: login fails";
/// a review with no title at all gets a plain fallback rather than "Fix: ".
fn fix_title_from_review(review_title: &str) -> String {
    let trimmed = review_title.trim();
    if trimmed.is_empty() {
        return "Fix what the review found".into();
    }
    // Do not stack prefixes when a review was itself titled "Fix: ..." by hand.
    if trimmed.starts_with("Fix: ") {
        return trimmed.to_string();
    }
    format!("Fix: {trimmed}")
}

/// The seeded first message: one plain line saying where the text came from,
/// then the review's conclusion verbatim. The agent reads this as its task,
/// so the intro is an instruction, not just a label.
fn build_fix_seed(review_title: &str, conclusion: &str) -> String {
    let title = review_title.trim();
    let intro = if title.is_empty() {
        "This came from a review chat. Please fix what it found:".to_string()
    } else {
        format!("This came from the review chat \"{title}\". Please fix what it found:")
    };
    format!("{intro}\n\n{}", conclusion.trim())
}

/// The review's conclusion: its last assistant-role, assistant-kind message
/// with any text. Thought summaries and tool rows are skipped -- they are
/// how the agent got there, not what it concluded.
fn review_conclusion(session: &AgentSession) -> Option<&str> {
    use crate::agentdesk::model::{MessageKind, MessageRole};
    session
        .messages
        .iter()
        .rev()
        .filter(|m| m.role == MessageRole::Assistant && m.kind == MessageKind::Assistant)
        .map(|m| m.plain_content.trim())
        .find(|text| !text.is_empty())
}

/// The plain, testable half of `agent_session_escalate_to_fix`.
///
/// Three writes through `commands::agent_desk`'s own seams, in this order:
/// create (a fresh `Draft` header), copy the review's provider/mode/team
/// preferences onto it, then append the seed message (which lifts it to
/// `Ready`). The preference copy runs before the append so the composer
/// shows the right provider the moment the pane opens, with no flash of the
/// default.
pub(crate) fn escalate_review_to_fix_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    review_session_id: &str,
) -> EscalateToFixOutcome {
    use crate::agentdesk::model::SessionLoadError as E;

    let review = match store::read_session(root, review_session_id) {
        Ok(s) => s,
        Err(E::NotFound) => return EscalateToFixOutcome::NotFound,
        Err(e) => return EscalateToFixOutcome::Failed { detail: e.to_string() },
    };

    if !can_escalate_to_fix(review.header.intent) {
        return EscalateToFixOutcome::NotAReview {
            intent: review.header.intent,
        };
    }

    let conclusion = match review_conclusion(&review) {
        Some(text) => text.to_string(),
        None => {
            return EscalateToFixOutcome::NothingToFix {
                detail: "The review has not said anything yet. Wait for it to finish, then try again.".into(),
            }
        }
    };

    let seed = build_fix_seed(&review.header.title, &conclusion);
    let create_request = CreateSessionRequest {
        repo_id: review.header.repo_id.clone(),
        repo_path: review.header.repo_path.clone(),
        repo_name: review.header.repo_name.clone(),
        title: fix_title_from_review(&review.header.title),
        source: review.header.source.clone(),
        intent: SessionIntent::Fix,
    };

    let created = match super::agent_desk::create_session_for_kickoff(root, create_request) {
        Ok(session) => session,
        Err(detail) => return EscalateToFixOutcome::Failed { detail },
    };
    let new_id = created.header.session_id.clone();

    let preferred_provider = review.header.preferred_provider.clone();
    let preferred_mode = review.header.preferred_mode.clone();
    let preferred_team = review.header.preferred_team.clone();
    if let Err(detail) = super::agent_desk::update_session_for_kickoff(locks, root, &new_id, |session| {
        session.header.preferred_provider = preferred_provider;
        session.header.preferred_mode = preferred_mode;
        session.header.preferred_team = preferred_team;
    }) {
        return EscalateToFixOutcome::Failed { detail };
    }

    match super::agent_desk::append_user_message_for_kickoff(locks, root, &new_id, seed) {
        Ok(session) => EscalateToFixOutcome::Created { session },
        Err(detail) => EscalateToFixOutcome::Failed { detail },
    }
}

/// "Fix this" on a finished review: opens a seeded Fix chat, does not start
/// it. See `escalate_review_to_fix_at`. Deliberately does not emit
/// `SELECT_SESSION_EVENT`: the button lives inside the Desk window, and the
/// pane that showed the review selects the new session itself.
#[tauri::command]
#[specta::specta]
pub async fn agent_session_escalate_to_fix(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: String,
) -> Result<EscalateToFixOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || escalate_review_to_fix_at(&locks, &root, &session_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

// -- Task 5: isolation for Fix, refused for everything else --
//
// architecture.md section 9: Fix's worktree policy is `Always` -- a worktree
// is provisioned before the engine receives edit capability, every time.
// Review/Summarize/Ask/Explain never call this at all (their `WorktreePolicy`
// is `Never`); Plan calls it only once the user explicitly starts it, at
// which point it behaves like Fix. This module does not itself decide WHEN
// to provision -- that is the execution-start caller's job, gated by
// `agentdesk::policy::check_tool_capability(.., ToolCapability::Worktree)` --
// it only provides the "make one, or fail outright" primitive so that
// decision has something honest to call.

/// What provisioning a Fix worktree found. Mirrors
/// `commands::airun::provision_run_worktree`'s contract exactly (same
/// `worktree::add` + `mark_as_run_worktree` calls, same
/// `gitwyrm-task/<slug>` branch naming) rather than reimplementing it, per
/// architecture.md section 1: "`src-tauri/src/git/worktree.rs` remains
/// execution isolation."
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProvisionKickoffWorktreeOutcome {
    /// `path` is the absolute folder the Fix execution must run in;
    /// `branch` is the new branch it was created on.
    Provisioned { path: String, branch: String },
    /// Isolation could not be created. Task 5.2: "If provisioning fails, do
    /// not fall back to the user's checkout" -- the caller must treat this
    /// as a hard stop for the Fix execution, never as permission to run
    /// against `open.path` instead.
    Failed { detail: String },
}

/// Provisions an isolated worktree for a Fix execution, on a fresh branch
/// named after the session's title, starting from `base_branch`.
///
/// Deliberately takes an already-resolved `crate::state::OpenRepo` (not a
/// `repo_id` it looks up itself) so this stays a plain, synchronous,
/// directly testable function -- the async/Tauri-state plumbing belongs to
/// whatever execution-start call site invokes this, matching how
/// `provision_run_worktree` in `commands::airun` is itself a plain function
/// called from inside an async command rather than being a command.
pub fn provision_kickoff_worktree(
    open: &std::sync::Arc<crate::state::OpenRepo>,
    base_branch: &str,
    session_title: &str,
) -> ProvisionKickoffWorktreeOutcome {
    use crate::git::worktree;

    let (main_path, run_branch, path) = {
        let repo = open.repo.lock().unwrap();
        let main = match worktree::main_workdir(&repo) {
            Some(m) => m,
            None => {
                return ProvisionKickoffWorktreeOutcome::Failed {
                    detail: "this project has no working folder".into(),
                }
            }
        };

        // Named after the session's own title (its source's snapshot title,
        // set at kickoff) rather than a generic "fix" so a leftover folder
        // says what it was for -- same reasoning as
        // `provision_run_worktree`'s task-based naming.
        let slug = worktree::branch_slug(&session_title.chars().take(40).collect::<String>());
        let mut candidate = format!("gitwyrm-fix/{slug}");
        let mut n = 2;
        while repo
            .find_branch(&candidate, git2::BranchType::Local)
            .is_ok()
        {
            candidate = format!("gitwyrm-fix/{slug}-{n}");
            n += 1;
            if n > 100 {
                break;
            }
        }
        let path = worktree::suggest_path(&main, &candidate);
        (main.to_string_lossy().into_owned(), candidate, path)
    };

    if let Err(e) = worktree::add(&main_path, &path, &run_branch, true, Some(base_branch)) {
        // Task 5.2: refused outright, no fallback to `open.path`. The
        // caller must not attempt to run the Fix execution against the
        // user's own checkout after seeing this.
        return ProvisionKickoffWorktreeOutcome::Failed {
            detail: format!(
                "Fix needs its own folder to work in safely, and one could not be made: {e}"
            ),
        };
    }

    // Mark it so the worktree list can label it and discard can tell a
    // Fix-created folder from one the user made themselves. Best-effort,
    // matching `provision_run_worktree`: a marker failure does not undo the
    // worktree that was already successfully created.
    {
        let repo = open.repo.lock().unwrap();
        if let Some(name) = worktree::list(&repo, None)
            .into_iter()
            .find(|w| worktree::paths_equal(std::path::Path::new(&w.path), std::path::Path::new(&path)))
            .map(|w| w.name)
        {
            let _ = worktree::mark_as_run_worktree(&repo, &name);
        }
    }

    ProvisionKickoffWorktreeOutcome::Provisioned {
        path,
        branch: run_branch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::SessionState;
    use std::path::Path;
    use tempfile::TempDir;

    fn temp_root() -> (TempDir, SessionStoreRoot) {
        let dir = TempDir::new().expect("create temp dir");
        let root = SessionStoreRoot::at(dir.path().join("agent-desk").join("v1"))
            .expect("init store root");
        (dir, root)
    }

    /// A real git repo with one commit on `main`, wrapped as the
    /// `crate::state::OpenRepo` shape `provision_kickoff_worktree` takes --
    /// matching the fixture pattern `git::worktree`'s own tests use.
    fn repo_with_commit() -> (TempDir, std::sync::Arc<crate::state::OpenRepo>, String) {
        let dir = TempDir::new().expect("temp repo");
        let repo = git2::Repository::init(dir.path()).expect("init repo");
        {
            let mut config = repo.config().expect("config");
            config.set_str("user.name", "Kickoff Test").expect("name");
            config
                .set_str("user.email", "kickoff@example.com")
                .expect("email");
        }
        std::fs::write(dir.path().join("a.txt"), "a").expect("write file");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("a.txt")).expect("add");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("tree id");
        let sig = git2::Signature::now("Kickoff Test", "kickoff@example.com").expect("sig");
        {
            // Scoped so `tree`'s borrow of `repo` ends before `repo` moves
            // into `OpenRepo::for_test` below.
            let tree = repo.find_tree(tree_id).expect("tree");
            repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
                .expect("commit");
        }
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();

        let open = std::sync::Arc::new(crate::state::OpenRepo::for_test(repo));
        (dir, open, branch)
    }

    fn issue_input(number: u32) -> SessionSourceInput {
        SessionSourceInput::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number,
            url: format!("https://example.test/issues/{number}"),
            title: format!("Issue #{number}"),
            summary: "Body text".into(),
        }
    }

    fn request(intent: SessionIntent, number: u32) -> StartAgentSessionRequest {
        StartAgentSessionRequest {
            repo_id: "repo-1".into(),
            repo_path: "C:/code/widgets".into(),
            repo_name: "widgets".into(),
            source: issue_input(number),
            intent,
            mode: None,
            team: None,
            provider_override: None,
        }
    }

    // R3.3: pins the event name the frontend listener in `AgentDeskView.tsx`
    // matches against by hand (there is no shared codegen for event names,
    // only for command/type shapes) -- mirrors `spec_desk.rs`'s own pin test
    // for `SELECT_DESK_TARGET_EVENT`.
    #[test]
    fn select_session_event_name_matches_the_frontend_listener() {
        assert_eq!(SELECT_SESSION_EVENT, "agent-desk://select-session");
    }

    // -- P1-A: `resolve_kickoff_execution_params` -- proves a kickoff's own
    //    explicit mode/team choice reaches the first run rather than being
    //    silently replaced by the intent's policy default, which is exactly
    //    what the old frontend-only auto-start path did (it always called
    //    `agentSessionStartExecution(id, policy.defaultMode,
    //    policy.defaultTeam, null)`, discarding whatever the kickoff request
    //    itself had named). --

    /// A caller that names neither `mode` nor `team` gets the intent's own
    /// policy defaults -- the ordinary "Fix" (no "...with" override) case.
    #[test]
    fn no_override_falls_back_to_the_intents_policy_defaults() {
        let (mode, team) = resolve_kickoff_execution_params(SessionIntent::Fix, None, None);
        let policy = policy::for_intent(SessionIntent::Fix);
        assert_eq!(mode, policy.default_mode);
        assert_eq!(team, policy.default_team);
    }

    /// An explicit mode override reaches the resolved params even when it
    /// disagrees with the intent's own default -- this is the value that
    /// used to be accepted into `StartAgentSessionRequest` and then dropped.
    #[test]
    fn explicit_mode_override_is_not_replaced_by_the_intent_default() {
        // Fix's own default is `Auto` -- naming `Ask` here proves the
        // request's choice wins, not the table's.
        let (mode, _team) = resolve_kickoff_execution_params(SessionIntent::Fix, Some(ExecutionMode::Ask), None);
        assert_eq!(mode, ExecutionMode::Ask);
    }

    /// Same proof for `team`: Fix defaults to `Lead`, so naming `Solo`
    /// explicitly must survive.
    #[test]
    fn explicit_team_override_is_not_replaced_by_the_intent_default() {
        let (_mode, team) = resolve_kickoff_execution_params(SessionIntent::Fix, None, Some(ExecutionTeam::Solo));
        assert_eq!(team, ExecutionTeam::Solo);
    }

    /// Both named at once, on an intent whose defaults differ from both --
    /// proves the two overrides are independent and both survive together,
    /// not just whichever one a narrower test happened to check.
    #[test]
    fn both_overrides_together_survive_independently() {
        let (mode, team) =
            resolve_kickoff_execution_params(SessionIntent::Plan, Some(ExecutionMode::Auto), Some(ExecutionTeam::Solo));
        assert_eq!(mode, ExecutionMode::Auto);
        assert_eq!(team, ExecutionTeam::Solo);
    }

    #[test]
    fn first_kickoff_creates_a_session() {
        let (_dir, root) = temp_root();
        let outcome = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        match outcome {
            FoundOrCreatedSession::Created { session } => {
                assert_eq!(session.header.intent, SessionIntent::Fix);
                assert_eq!(session.header.repo_id, "repo-1");
                assert_eq!(session.header.title, "Issue #1");
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn re_clicking_the_same_intent_on_the_same_issue_focuses_the_existing_session() {
        let (_dir, root) = temp_root();
        let first = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let FoundOrCreatedSession::Created { session: first_session } = first else {
            panic!("expected first kickoff to create a session");
        };

        let second = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        match second {
            FoundOrCreatedSession::Focused { session } => {
                assert_eq!(
                    session.header.session_id, first_session.header.session_id,
                    "the second Fix click must focus the first session, not fork a new one"
                );
            }
            other => panic!("expected FocusedExisting, got {other:?}"),
        }
    }

    #[test]
    fn different_intents_on_the_same_issue_get_separate_sessions() {
        let (_dir, root) = temp_root();
        let fix = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let explain = find_or_create_agent_session_at(&root, request(SessionIntent::Explain, 1));

        let (FoundOrCreatedSession::Created { session: fix_session }, FoundOrCreatedSession::Created { session: explain_session }) =
            (fix, explain)
        else {
            panic!("both Fix and Explain should create their own session the first time");
        };
        assert_ne!(fix_session.header.session_id, explain_session.header.session_id);
    }

    /// P1-A ("source clicks do not all do what they say"): the create half
    /// of kickoff must succeed identically for Review and Summarize -- the
    /// two read-only intents the audit named by name -- as it does for Fix.
    /// This is the necessary precondition for `agent_session_start`'s fix:
    /// nothing about `find_or_create_agent_session_at` branches on
    /// `IntentPolicy.canWrite`, so there is nothing here that could gate
    /// creation OR (by the same absence of a canWrite check in
    /// `agent_session_start`'s own body -- see that command's doc comment)
    /// the start attempt that follows it, the way the old frontend-only
    /// `autoStartIfWriteCapable` used to.
    #[test]
    fn review_and_summarize_create_a_session_exactly_like_fix_does() {
        let (_dir, root) = temp_root();
        for intent in [SessionIntent::Review, SessionIntent::Summarize, SessionIntent::Fix] {
            let outcome = find_or_create_agent_session_at(&root, request(intent, 100 + intent as u32));
            let FoundOrCreatedSession::Created { session } = outcome else {
                panic!("expected {intent:?} to create a session, got a different outcome");
            };
            assert_eq!(session.header.intent, intent);
            assert_eq!(session.header.state, SessionState::Draft);
        }
    }

    #[test]
    fn different_issues_get_separate_sessions() {
        let (_dir, root) = temp_root();
        let one = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let two = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 2));

        let (FoundOrCreatedSession::Created { session: s1 }, FoundOrCreatedSession::Created { session: s2 }) =
            (one, two)
        else {
            panic!("both issues should create their own session");
        };
        assert_ne!(s1.header.session_id, s2.header.session_id);
    }

    #[test]
    fn a_finished_session_does_not_block_starting_a_new_one() {
        let (_dir, root) = temp_root();
        let first = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let FoundOrCreatedSession::Created { mut session } = first else {
            panic!("expected Created");
        };
        session.header.state = SessionState::Finished;
        store::write_session(&root, &session).expect("write finished session");
        // Real callers that change a session's state always go through
        // `update_session_at`, which refreshes `index.json` after every
        // write -- `find_active_session_for_source` reads the index, not
        // individual session files, so a test that skips this step is
        // exercising a state the index can never actually be in.
        let (headers, _diagnostics) = store::rebuild_index_from_sessions(&root);
        store::write_index(&root, &headers).expect("refresh index");

        let second = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        match second {
            FoundOrCreatedSession::Created { session: new_session } => {
                assert_ne!(
                    new_session.header.session_id, session.header.session_id,
                    "a finished session must not be treated as still active"
                );
            }
            other => panic!("expected a new Created session, got {other:?}"),
        }
    }

    #[test]
    fn an_archived_active_session_does_not_block_a_new_one() {
        let (_dir, root) = temp_root();
        let first = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let FoundOrCreatedSession::Created { mut session } = first else {
            panic!("expected Created");
        };
        // Still "Working" (active), but archived -- archiving is a stronger
        // signal than state alone that the user is done with it.
        session.header.state = SessionState::Working;
        session.header.archived = true;
        store::write_session(&root, &session).expect("write archived session");
        // See the comment in `a_finished_session_does_not_block_starting_a_new_one`:
        // the index must be refreshed to match what a real mutation path does.
        let (headers, _diagnostics) = store::rebuild_index_from_sessions(&root);
        store::write_index(&root, &headers).expect("refresh index");

        let second = find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 1));
        assert!(matches!(second, FoundOrCreatedSession::Created { .. }));
    }

    #[test]
    fn duplicate_detection_is_scoped_to_the_repository() {
        let (_dir, root) = temp_root();
        let mut in_repo_a = request(SessionIntent::Fix, 1);
        in_repo_a.repo_id = "repo-a".into();
        let mut in_repo_b = request(SessionIntent::Fix, 1);
        in_repo_b.repo_id = "repo-b".into();

        let a = find_or_create_agent_session_at(&root, in_repo_a);
        let b = find_or_create_agent_session_at(&root, in_repo_b);
        let (FoundOrCreatedSession::Created { session: sa }, FoundOrCreatedSession::Created { session: sb }) =
            (a, b)
        else {
            panic!("same issue number in two different repos must both create sessions");
        };
        assert_ne!(sa.header.session_id, sb.header.session_id);
    }

    #[test]
    fn find_active_session_for_source_matches_identity_and_intent_and_repo() {
        let (_dir, root) = temp_root();
        let FoundOrCreatedSession::Created { session } =
            find_or_create_agent_session_at(&root, request(SessionIntent::Review, 9))
        else {
            panic!("expected Created");
        };
        let (source, _title) = issue_input(9).into_source_and_title("repo-1");
        let found = find_active_session_for_source(&root, "repo-1", &source, SessionIntent::Review);
        assert_eq!(found.map(|h| h.session_id), Some(session.header.session_id));
    }

    #[test]
    fn manual_source_never_collides_across_kickoffs() {
        // Manual sessions ("New chat") share the same identity key
        // (`manual:<repo_id>`) for a repo, which is deliberate for other
        // uses of `identity_key`, but kickoff must never be called with
        // `SessionSourceInput::Manual` for the Fix/Review/Summarize flows
        // this package adds -- documented here so a future caller does not
        // assume Manual participates in deduplication the way issue/PR
        // sources do.
        let (_dir, root) = temp_root();
        let mut req = request(SessionIntent::Ask, 1);
        req.source = SessionSourceInput::Manual;
        let first = find_or_create_agent_session_at(&root, req);
        assert!(matches!(first, FoundOrCreatedSession::Created { .. }));
    }

    // -- provision_kickoff_worktree (task 5.1/5.2) --

    #[test]
    fn provisions_an_isolated_worktree_on_a_fresh_branch() {
        let (_repo_dir, open, base_branch) = repo_with_commit();
        let outcome = provision_kickoff_worktree(&open, &base_branch, "Fix the login bug");
        match outcome {
            ProvisionKickoffWorktreeOutcome::Provisioned { path, branch } => {
                assert!(
                    std::path::Path::new(&path).exists(),
                    "provisioned worktree folder must actually exist on disk"
                );
                assert!(
                    branch.starts_with("gitwyrm-fix/"),
                    "branch {branch} should be named after the Fix worktree convention"
                );
            }
            ProvisionKickoffWorktreeOutcome::Failed { detail } => {
                panic!("expected a worktree to be provisioned, got Failed: {detail}")
            }
        }
    }

    #[test]
    fn provisioned_worktree_is_marked_as_a_run_worktree() {
        let (_repo_dir, open, base_branch) = repo_with_commit();
        let outcome = provision_kickoff_worktree(&open, &base_branch, "Fix the login bug");
        let ProvisionKickoffWorktreeOutcome::Provisioned { path, .. } = outcome else {
            panic!("expected Provisioned");
        };

        // Marked exactly the way `commands::airun::provision_run_worktree`
        // marks its own worktrees, so the existing worktree list/discard UI
        // treats a Fix worktree the same as a task-run one (task 5.1: "before
        // the Fix engine receives edit capability").
        let repo = open.repo.lock().unwrap();
        let marked = crate::git::worktree::list(&repo, None)
            .into_iter()
            .find(|w| crate::git::worktree::paths_equal(std::path::Path::new(&w.path), std::path::Path::new(&path)))
            .map(|w| w.is_run_worktree)
            .unwrap_or(false);
        assert!(marked, "the provisioned worktree must be marked as a run worktree");
    }

    #[test]
    fn a_second_provision_for_the_same_title_gets_a_distinct_branch() {
        // Two Fix kickoffs whose sessions share a title (or whose slugs
        // collide) must not collide on the same branch name -- mirrors
        // `provision_run_worktree`'s own de-duplication loop.
        let (_repo_dir, open, base_branch) = repo_with_commit();
        let first = provision_kickoff_worktree(&open, &base_branch, "Fix the login bug");
        let second = provision_kickoff_worktree(&open, &base_branch, "Fix the login bug");

        let (ProvisionKickoffWorktreeOutcome::Provisioned { branch: b1, .. }, ProvisionKickoffWorktreeOutcome::Provisioned { branch: b2, .. }) =
            (first, second)
        else {
            panic!("expected both provisions to succeed");
        };
        assert_ne!(b1, b2, "two Fix worktrees must not share a branch name");
    }

    #[test]
    fn provisioning_never_touches_the_users_own_checkout() {
        // Task 5.2's core proof: after provisioning, the main repository's
        // own working directory must be untouched -- the new file only
        // exists inside the isolated worktree.
        let (repo_dir, open, base_branch) = repo_with_commit();
        let outcome = provision_kickoff_worktree(&open, &base_branch, "Fix the login bug");
        let ProvisionKickoffWorktreeOutcome::Provisioned { path, .. } = outcome else {
            panic!("expected Provisioned");
        };

        std::fs::write(std::path::Path::new(&path).join("fix.txt"), "isolated change")
            .expect("write into the worktree");

        assert!(
            !repo_dir.path().join("fix.txt").exists(),
            "a write into the Fix worktree must never appear in the user's own checkout"
        );
    }

    // -- Task 4.6: escalate a review into a Fix chat --

    fn assistant_message(text: &str) -> crate::agentdesk::model::SessionMessage {
        use crate::agentdesk::model::{MessageKind, MessageRole, SessionMessage};
        SessionMessage {
            message_id: format!("m-{}", text.len()),
            segment_id: "seg-1".into(),
            role: MessageRole::Assistant,
            timestamp: "2026-01-01T00:00:00Z".into(),
            plain_content: text.into(),
            rendered_content: None,
            provider: None,
            model: None,
            kind: MessageKind::Assistant,
            execution_id: None,
            sequence: None,
            import: None,
            targets: Vec::new(),
        }
    }

    /// A finished Review session on issue #7 whose transcript ends with the
    /// given assistant conclusion, plus a provider preference to carry over.
    fn review_session_with_conclusion(root: &SessionStoreRoot, conclusion: Option<&str>) -> AgentSession {
        let FoundOrCreatedSession::Created { mut session } =
            find_or_create_agent_session_at(root, request(SessionIntent::Review, 7))
        else {
            panic!("expected the review to be created");
        };
        session.header.preferred_provider = Some("codex".into());
        session.header.preferred_mode = Some("ask".into());
        session.header.state = SessionState::Finished;
        if let Some(text) = conclusion {
            session.messages.push(assistant_message(text));
        }
        store::write_session(root, &session).expect("write review session");
        session
    }

    #[test]
    fn escalating_a_review_creates_a_seeded_fix_session_on_the_same_source() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let review = review_session_with_conclusion(&root, Some("The login form drops the password on retry."));

        let outcome = escalate_review_to_fix_at(&locks, &root, &review.header.session_id);
        let EscalateToFixOutcome::Created { session } = outcome else {
            panic!("expected Created, got {outcome:?}");
        };

        assert_ne!(session.header.session_id, review.header.session_id);
        assert_eq!(session.header.intent, SessionIntent::Fix);
        assert_eq!(session.header.repo_id, review.header.repo_id);
        assert_eq!(session.header.source, review.header.source, "the fix must point at the same issue");
        assert_eq!(session.header.title, "Fix: Issue #7");
        assert_eq!(session.header.preferred_provider.as_deref(), Some("codex"));
        assert_eq!(session.header.preferred_mode.as_deref(), Some("ask"));
        // Seeded but not started: Ready (one user message), no execution.
        assert_eq!(session.header.state, SessionState::Ready);
        assert!(session.executions.is_empty());
        assert_eq!(session.messages.len(), 1);
        let seed = &session.messages[0];
        assert_eq!(seed.role, crate::agentdesk::model::MessageRole::User);
        assert!(
            seed.plain_content.starts_with("This came from the review chat \"Issue #7\"."),
            "{}",
            seed.plain_content
        );
        assert!(
            seed.plain_content.ends_with("The login form drops the password on retry."),
            "{}",
            seed.plain_content
        );

        // The review itself is untouched: still Review, still read-only.
        let reread = store::read_session(&root, &review.header.session_id).expect("review still readable");
        assert_eq!(reread.header.intent, SessionIntent::Review);
        assert!(reread
            .messages
            .iter()
            .all(|m| m.role != crate::agentdesk::model::MessageRole::User));
    }

    #[test]
    fn escalation_uses_the_last_conclusion_not_the_first() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let mut review = review_session_with_conclusion(&root, Some("First pass: looks fine."));
        review.messages.push(assistant_message("Second pass: the retry path is broken."));
        store::write_session(&root, &review).expect("write");

        let EscalateToFixOutcome::Created { session } =
            escalate_review_to_fix_at(&locks, &root, &review.header.session_id)
        else {
            panic!("expected Created");
        };
        assert!(session.messages[0]
            .plain_content
            .ends_with("Second pass: the retry path is broken."));
    }

    #[test]
    fn escalation_refuses_a_fix_session() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let FoundOrCreatedSession::Created { mut session } =
            find_or_create_agent_session_at(&root, request(SessionIntent::Fix, 7))
        else {
            panic!("expected Created");
        };
        session.messages.push(assistant_message("Done, I fixed it."));
        store::write_session(&root, &session).expect("write");

        let outcome = escalate_review_to_fix_at(&locks, &root, &session.header.session_id);
        assert!(
            matches!(
                outcome,
                EscalateToFixOutcome::NotAReview {
                    intent: SessionIntent::Fix
                }
            ),
            "got {outcome:?}"
        );
    }

    #[test]
    fn escalation_refuses_a_review_with_no_assistant_message() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let review = review_session_with_conclusion(&root, None);

        let outcome = escalate_review_to_fix_at(&locks, &root, &review.header.session_id);
        assert!(matches!(outcome, EscalateToFixOutcome::NothingToFix { .. }), "got {outcome:?}");
        // Nothing was created for it.
        let loaded = store::load_or_rebuild_index(&root);
        assert_eq!(loaded.headers.len(), 1, "no Fix session may be left behind after a refusal");
    }

    #[test]
    fn escalation_skips_thought_and_tool_rows_when_finding_the_conclusion() {
        use crate::agentdesk::model::MessageKind;
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let mut review = review_session_with_conclusion(&root, Some("Real conclusion."));
        let mut thought = assistant_message("thinking about it");
        thought.kind = MessageKind::ThoughtSummary;
        review.messages.push(thought);
        store::write_session(&root, &review).expect("write");

        let EscalateToFixOutcome::Created { session } =
            escalate_review_to_fix_at(&locks, &root, &review.header.session_id)
        else {
            panic!("expected Created");
        };
        assert!(session.messages[0].plain_content.ends_with("Real conclusion."));
    }

    #[test]
    fn escalation_reports_not_found_for_an_unknown_session() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        assert!(matches!(
            escalate_review_to_fix_at(&locks, &root, "no-such-session"),
            EscalateToFixOutcome::NotFound
        ));
    }

    #[test]
    fn fix_title_handles_blank_and_already_prefixed_titles() {
        assert_eq!(fix_title_from_review("Issue #3"), "Fix: Issue #3");
        assert_eq!(fix_title_from_review("   "), "Fix what the review found");
        assert_eq!(fix_title_from_review("Fix: Issue #3"), "Fix: Issue #3");
    }

    #[test]
    fn every_read_only_intent_may_escalate_and_no_writing_intent_may() {
        for intent in [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Review,
            SessionIntent::Summarize,
        ] {
            assert!(can_escalate_to_fix(intent), "{intent:?}");
        }
        for intent in [SessionIntent::Fix, SessionIntent::Plan] {
            assert!(!can_escalate_to_fix(intent), "{intent:?}");
        }
    }
}
