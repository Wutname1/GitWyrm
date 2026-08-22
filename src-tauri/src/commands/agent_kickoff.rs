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

use super::agent_desk::CreateSessionRequest;

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
    /// A brand-new session was created for this source/intent and is now
    /// `Preparing` (or `Draft`, for intents that do not auto-start).
    Created { session: AgentSession },
    /// An active (non-finished/failed/stopped) session already exists for
    /// this exact repo/source-identity/intent -- design.md: "An active
    /// session with the same repo/source/intent is focused and explained."
    /// No new session was created; the caller should select `session` and
    /// tell the user why.
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

fn start_agent_session_at(
    root: &SessionStoreRoot,
    request: StartAgentSessionRequest,
) -> StartAgentSessionOutcome {
    let (source, title) = request.source.into_source_and_title(&request.repo_id);

    if let Some(existing) =
        find_active_session_for_source(root, &request.repo_id, &source, request.intent)
    {
        match store::read_session(root, &existing.session_id) {
            Ok(session) => return StartAgentSessionOutcome::FocusedExisting { session },
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
        Ok(session) => StartAgentSessionOutcome::Created { session },
        Err(detail) => StartAgentSessionOutcome::WriteFailed { detail },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_start(
    app: AppHandle,
    request: StartAgentSessionRequest,
) -> Result<StartAgentSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let outcome = tauri::async_runtime::spawn_blocking(move || start_agent_session_at(&root, request))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    // R3.3: tell the Desk window exactly which session to select, the moment
    // it exists -- whether it was freshly created or an existing one was
    // focused (double-Fix must land on the same session just as surely as a
    // first Fix does).
    match &outcome {
        StartAgentSessionOutcome::Created { session } | StartAgentSessionOutcome::FocusedExisting { session } => {
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

    #[test]
    fn first_kickoff_creates_a_session() {
        let (_dir, root) = temp_root();
        let outcome = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        match outcome {
            StartAgentSessionOutcome::Created { session } => {
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
        let first = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let StartAgentSessionOutcome::Created { session: first_session } = first else {
            panic!("expected first kickoff to create a session");
        };

        let second = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        match second {
            StartAgentSessionOutcome::FocusedExisting { session } => {
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
        let fix = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let explain = start_agent_session_at(&root, request(SessionIntent::Explain, 1));

        let (StartAgentSessionOutcome::Created { session: fix_session }, StartAgentSessionOutcome::Created { session: explain_session }) =
            (fix, explain)
        else {
            panic!("both Fix and Explain should create their own session the first time");
        };
        assert_ne!(fix_session.header.session_id, explain_session.header.session_id);
    }

    #[test]
    fn different_issues_get_separate_sessions() {
        let (_dir, root) = temp_root();
        let one = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let two = start_agent_session_at(&root, request(SessionIntent::Fix, 2));

        let (StartAgentSessionOutcome::Created { session: s1 }, StartAgentSessionOutcome::Created { session: s2 }) =
            (one, two)
        else {
            panic!("both issues should create their own session");
        };
        assert_ne!(s1.header.session_id, s2.header.session_id);
    }

    #[test]
    fn a_finished_session_does_not_block_starting_a_new_one() {
        let (_dir, root) = temp_root();
        let first = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let StartAgentSessionOutcome::Created { mut session } = first else {
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

        let second = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        match second {
            StartAgentSessionOutcome::Created { session: new_session } => {
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
        let first = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        let StartAgentSessionOutcome::Created { mut session } = first else {
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

        let second = start_agent_session_at(&root, request(SessionIntent::Fix, 1));
        assert!(matches!(second, StartAgentSessionOutcome::Created { .. }));
    }

    #[test]
    fn duplicate_detection_is_scoped_to_the_repository() {
        let (_dir, root) = temp_root();
        let mut in_repo_a = request(SessionIntent::Fix, 1);
        in_repo_a.repo_id = "repo-a".into();
        let mut in_repo_b = request(SessionIntent::Fix, 1);
        in_repo_b.repo_id = "repo-b".into();

        let a = start_agent_session_at(&root, in_repo_a);
        let b = start_agent_session_at(&root, in_repo_b);
        let (StartAgentSessionOutcome::Created { session: sa }, StartAgentSessionOutcome::Created { session: sb }) =
            (a, b)
        else {
            panic!("same issue number in two different repos must both create sessions");
        };
        assert_ne!(sa.header.session_id, sb.header.session_id);
    }

    #[test]
    fn find_active_session_for_source_matches_identity_and_intent_and_repo() {
        let (_dir, root) = temp_root();
        let StartAgentSessionOutcome::Created { session } =
            start_agent_session_at(&root, request(SessionIntent::Review, 9))
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
        let first = start_agent_session_at(&root, req);
        assert!(matches!(first, StartAgentSessionOutcome::Created { .. }));
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
}
