//! Commands for lead-and-helper agent graphs (openspec/changes/agent-desk-agent-graphs).
//!
//! Extends `commands::agent_desk`'s single-execution session commands with a
//! graph of one lead plus up to three helpers: proposing a graph (Plan mode),
//! starting it (which mints helper execution IDs and provisions a mandatory
//! worktree per helper), projecting it for the UI, resolving an integration
//! conflict, and answering an approval keyed to the helper that asked.
//!
//! Deliberately a *sibling* module to `agent_desk`, not an edit to it: the
//! existing single-execution commands
//! (`agent_session_start_execution`/`agent_session_stop_execution`) are
//! reused as-is for starting the lead and for stopping any one node or all
//! of them (`StopScope::One`/`StopScope::All` already cover "stop this
//! helper" and "stop everything"), so nothing here duplicates that surface.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::agentdesk::graph::{
    self, GraphNodeView, GraphValidationError, HelperRole, IntegrationConflict, ProposedGraph,
    ProposedHelperJob,
};
use crate::agentdesk::model::{
    AgentSession, ExecutionId, ExecutionRecord, SessionId, SessionLoadError, SessionState,
};
use crate::agentdesk::store::{self, SessionStoreRoot};
use crate::error::AppError;
use crate::git::worktree;
use crate::state::RepoManager;

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn resolve_root(app: &AppHandle) -> Result<SessionStoreRoot, AppError> {
    SessionStoreRoot::resolve(app).map_err(|e| AppError::Other(e.to_string()))
}

/// Mirrors `commands::agent_desk`'s own private `UpdateSessionOutcome` --
/// duplicated rather than imported because that type is not `pub` and this
/// module is deliberately not editing `agent_desk.rs` (see module doc).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateOutcome {
    Updated { session: AgentSession },
    NotFound,
    Damaged { reason: String },
    WriteFailed { detail: String },
    Unavailable { detail: String },
}

fn update_session_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    mutate: impl FnOnce(&mut AgentSession),
) -> UpdateOutcome {
    locks.with_session_lock(session_id, || {
        let mut session = match store::read_session(root, session_id) {
            Ok(s) => s,
            Err(SessionLoadError::NotFound) => return UpdateOutcome::NotFound,
            Err(SessionLoadError::Io { detail }) => return UpdateOutcome::Unavailable { detail },
            Err(reason) => {
                return UpdateOutcome::Damaged {
                    reason: reason.to_string(),
                }
            }
        };
        mutate(&mut session);
        session.header.updated_at = now_rfc3339();
        match store::write_session(root, &session) {
            Ok(()) => UpdateOutcome::Updated { session },
            Err(e) => UpdateOutcome::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

// ---------------------------------------------------------------------------
// Propose (Plan mode)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProposeGraphOutcome {
    /// The proposal validated and was persisted on a fresh lead execution in
    /// `NeedsInput` (AwaitingStart) -- nothing has started running yet
    /// (tasks.md 2.1, 2.2).
    AwaitingStart {
        session: AgentSession,
        execution_id: ExecutionId,
    },
    Invalid { reason: GraphValidationError },
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
    /// This session already has an execution running -- a second lead cannot
    /// be proposed on top of one already in flight.
    AlreadyRunning { execution_id: ExecutionId },
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_propose_graph(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    graph: ProposedGraph,
) -> Result<ProposeGraphOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    Ok(propose_graph_at(&locks_arc, &root, &session_id, graph))
}

fn propose_graph_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    proposed: ProposedGraph,
) -> ProposeGraphOutcome {
    if let Err(reason) = graph::validate_graph(&proposed) {
        return ProposeGraphOutcome::Invalid { reason };
    }

    let execution_id = new_id();
    let outcome = update_session_at(locks, root, session_id, |s| {
        s.header.state = SessionState::NeedsInput;
        let now = now_rfc3339();
        let mut record = ExecutionRecord::minimal(
            execution_id.clone(),
            s.header.session_id.clone(),
            None,
            SessionState::NeedsInput,
            now,
            None,
            0,
        );
        record.proposed_graph = Some(proposed.clone());
        s.executions.push(record);
        s.header.active_execution_id = Some(execution_id.clone());
    });

    match outcome {
        UpdateOutcome::Updated { session } => {
            if let Some(active) = &session.header.active_execution_id {
                if active != &execution_id {
                    // Someone else's execution won the race for "active" --
                    // extremely unlikely under the session lock, but report
                    // it honestly rather than pretending this one is live.
                    return ProposeGraphOutcome::AlreadyRunning {
                        execution_id: active.clone(),
                    };
                }
            }
            ProposeGraphOutcome::AwaitingStart {
                session,
                execution_id,
            }
        }
        UpdateOutcome::NotFound => ProposeGraphOutcome::NotFound,
        UpdateOutcome::Damaged { reason } => ProposeGraphOutcome::Damaged { reason },
        UpdateOutcome::WriteFailed { detail } => ProposeGraphOutcome::WriteFailed { detail },
        UpdateOutcome::Unavailable { detail } => ProposeGraphOutcome::Unavailable { detail },
    }
}

// ---------------------------------------------------------------------------
// Start (mints helper execution IDs, provisions worktrees, schedules)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartGraphOutcome {
    /// The lead's `NeedsInput` proposal became `Working`, and every
    /// dependency-ready helper (up to the 3-way cap) was minted an execution
    /// ID, a branch, and a mandatory worktree, then marked `Ready` so the
    /// scheduler will pick them up (tasks.md 2.3, 2.4, 3.1, 3.3, 4).
    Started {
        session: AgentSession,
        lead_execution_id: ExecutionId,
        started_helpers: Vec<ExecutionId>,
    },
    /// No `NeedsInput` proposal was found on this session -- Start was
    /// called with nothing to start.
    NoProposal,
    /// The proposal that was drafted no longer validates (e.g. a concurrent
    /// edit corrupted it) -- re-checked here, not trusted from draft time
    /// (tasks.md 2.3 "revalidate current source/policy/worktree capacity").
    Invalid { reason: GraphValidationError },
    /// A helper's worktree could not be created. No helper is left
    /// half-started: this is checked for every helper before any of them is
    /// marked `Ready`.
    WorktreeFailed { node_id: String, detail: String },
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
    SourceMissing { detail: String },
    /// A concurrent `agent_session_start_graph` call already consumed this
    /// lead's proposal (double click, frontend retry) between this call's
    /// unlocked read and its locked write. The worktrees THIS call
    /// provisioned were cleaned up before returning, so nothing is leaked --
    /// re-check `agent_session_graph_view` for whatever the winning call
    /// actually started.
    AlreadyStarted,
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_start_graph(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    links: tauri::State<'_, crate::agentdesk::RunSessionLinks>,
    executions: tauri::State<'_, crate::agentdesk::ExecutionRegistry>,
    manager: tauri::State<'_, RepoManager>,
    session_id: SessionId,
) -> Result<StartGraphOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    let manager_owned = manager.inner();
    let outcome = start_graph_at(&locks_arc, &root, manager_owned, &session_id);

    // R6.3: "Launch each dependency-ready helper through the same execution
    // registry as solo runs." `start_graph_at` only PERSISTS the ready
    // helpers as `Ready` records with a provisioned worktree -- nothing
    // about writing that record starts a process. This is the actual launch
    // step, mirroring `commands::agent_desk::start_execution_at`'s own tail
    // (discover the transport, register in `ExecutionRegistry` before the
    // first `Working` event, spawn `cli_run::run_task`) once per helper this
    // call is responsible for starting.
    if let StartGraphOutcome::Started { ref session, ref started_helpers, .. } = outcome {
        for helper_execution_id in started_helpers {
            launch_helper(
                &app,
                &locks_arc,
                &root,
                links.inner(),
                executions.inner(),
                session,
                helper_execution_id,
            );
        }
    }

    Ok(outcome)
}

fn start_graph_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    manager: &RepoManager,
    session_id: &str,
) -> StartGraphOutcome {
    // Step 1: read-only pass to find the lead's proposal and re-validate it
    // against the CURRENT policy/graph shape (tasks.md 2.3) -- nothing is
    // written yet.
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(SessionLoadError::NotFound) => return StartGraphOutcome::NotFound,
        Err(SessionLoadError::Io { detail }) => return StartGraphOutcome::Unavailable { detail },
        Err(reason) => {
            return StartGraphOutcome::Damaged {
                reason: reason.to_string(),
            }
        }
    };

    let Some(lead) = session
        .executions
        .iter()
        .find(|e| e.parent_execution_id.is_none() && e.proposed_graph.is_some())
    else {
        return StartGraphOutcome::NoProposal;
    };
    let lead_execution_id = lead.execution_id.clone();
    let proposed = lead.proposed_graph.clone().expect("checked above");

    if let Err(reason) = graph::validate_graph(&proposed) {
        return StartGraphOutcome::Invalid { reason };
    }

    // Step 2: the repository must actually be open here, and worktrees are
    // provisioned against its main working folder (tasks.md 2.3, 4).
    let open = match manager.get(&session.header.repo_id) {
        Ok(o) => o,
        Err(e) => {
            return StartGraphOutcome::SourceMissing {
                detail: e.to_string(),
            }
        }
    };
    let main_workdir = {
        let repo = open.repo.lock().unwrap_or_else(|e| e.into_inner());
        worktree::main_workdir(&repo)
    };
    let Some(main_workdir) = main_workdir else {
        return StartGraphOutcome::SourceMissing {
            detail: "this project has no working folder".into(),
        };
    };
    let main_workdir_str = main_workdir.to_string_lossy().into_owned();

    // Step 3: figure out which helpers are dependency-ready right now (no
    // `depends_on` at all, since nothing has an execution id yet at
    // proposal time -- every helper with zero dependencies is eligible),
    // capped at MAX_CONCURRENT_HELPERS. Helpers with dependencies are
    // created in `Draft` and picked up by the scheduler once their
    // dependency's execution finishes (tasks.md 3.3).
    let ready_first: Vec<&graph::ProposedHelperJob> = proposed
        .helpers
        .iter()
        .filter(|h| h.depends_on.is_empty())
        .take(graph::MAX_CONCURRENT_HELPERS)
        .collect();

    // Provision a worktree for every helper that writes (role Builder, or
    // any role with a non-empty allowed_paths) BEFORE any session write, so
    // a mid-way failure leaves nothing half-started (tasks.md 4:
    // "MANDATORY worktree per helper" -- every helper gets one, including
    // read-only ones, so its inspection never touches the user's own
    // checkout either).
    struct Provisioned {
        node_id: String,
        execution_id: ExecutionId,
        branch: String,
        path: String,
        base_oid: Option<String>,
    }
    // R6.7/R3.5: the commit every helper's worktree started from, recorded
    // ONCE here (not re-derived per helper) so all helpers in the same
    // batch share one honest merge base even if HEAD moves during
    // provisioning -- the same reasoning `start_execution_at` already
    // applies to a solo Fix execution's own `base_oid`.
    let base_oid: Option<String> = git2::Repository::open(&main_workdir_str)
        .ok()
        .and_then(|repo| repo.head().ok().and_then(|h| h.target()))
        .map(|oid| oid.to_string());
    let mut provisioned: Vec<Provisioned> = Vec::new();
    for job in &ready_first {
        let execution_id = crate::agentdesk::execution_id_for_run_session(&new_id());
        let branch = format!("agent-desk/{}/{}", short(&lead_execution_id), job.node_id);
        let path = worktree::suggest_path(&main_workdir, &branch);
        if let Err(e) = worktree::add(&main_workdir_str, &path, &branch, true, Some("HEAD")) {
            return StartGraphOutcome::WorktreeFailed {
                node_id: job.node_id.clone(),
                detail: e.to_string(),
            };
        }
        if let Ok(marked_repo) = git2::Repository::open(&main_workdir_str) {
            let _ = worktree::mark_as_run_worktree(&marked_repo, &worktree_admin_name(&path));
        }
        provisioned.push(Provisioned {
            node_id: job.node_id.clone(),
            execution_id,
            branch,
            path,
            base_oid: base_oid.clone(),
        });
    }

    // Step 4: re-check the lead STILL has a proposal to consume (no other
    // concurrent `start_graph_at` call already won this race) and, if so,
    // write every provisioned helper as `Ready` -- all inside ONE lock
    // acquisition via `commit_started_graph_if_still_proposed` (mirrors
    // `record_execution_if_not_running` in `commands::agent_desk`).
    // Worktree provisioning above stays unlocked and slow; only this final
    // step is locked.
    let started_helpers: Vec<ExecutionId> =
        provisioned.iter().map(|p| p.execution_id.clone()).collect();
    let provisioned_for_commit: Vec<ProvisionedHelper> = provisioned
        .iter()
        .map(|p| ProvisionedHelper {
            node_id: p.node_id.clone(),
            execution_id: p.execution_id.clone(),
            branch: p.branch.clone(),
            path: p.path.clone(),
            base_oid: p.base_oid.clone(),
        })
        .collect();
    let commit_outcome = commit_started_graph_if_still_proposed(
        locks,
        root,
        session_id,
        &lead_execution_id,
        &proposed,
        &provisioned_for_commit,
    );

    match commit_outcome {
        CommitGraphOutcome::Started { session } => StartGraphOutcome::Started {
            session,
            lead_execution_id,
            started_helpers,
        },
        CommitGraphOutcome::AlreadyStarted => {
            // The re-check inside the lock found this lead's proposal
            // already gone -- another concurrent call won the race and
            // consumed it. The worktrees THIS call just provisioned are not
            // referenced by any record that will ever be written, so they
            // must be cleaned up here or they leak on disk (the defect this
            // fix closes). Best-effort: this call already lost the race, so
            // a cleanup failure is swallowed rather than surfaced as this
            // call's own error -- the winning call's result is what the
            // user sees.
            for p in &provisioned {
                if let Ok(repo) = git2::Repository::open(&main_workdir_str) {
                    let _ = worktree::remove(
                        &repo,
                        &main_workdir_str,
                        &p.path,
                        worktree::DirtyChoice::Discard,
                    );
                }
            }
            StartGraphOutcome::AlreadyStarted
        }
        CommitGraphOutcome::NotFound => StartGraphOutcome::NotFound,
        CommitGraphOutcome::Damaged { reason } => StartGraphOutcome::Damaged { reason },
        CommitGraphOutcome::WriteFailed { detail } => StartGraphOutcome::WriteFailed { detail },
        CommitGraphOutcome::Unavailable { detail } => StartGraphOutcome::Unavailable { detail },
    }
}

/// A worktree `start_graph_at` already provisioned on disk (unlocked,
/// before this function's single locked re-check-and-write), reduced to
/// what the write step needs. Kept separate from `start_graph_at`'s private
/// `Provisioned` so this function has no dependency on that local type and
/// can be called directly from tests without going through the full
/// (worktree-provisioning, therefore slow and hard to race deterministically
/// in a test) `start_graph_at`.
struct ProvisionedHelper {
    node_id: String,
    execution_id: ExecutionId,
    branch: String,
    path: String,
    base_oid: Option<String>,
}

/// Internal-only outcome of `commit_started_graph_if_still_proposed`. Not a
/// public/`Type` enum like `StartGraphOutcome` -- purely lets that single
/// `with_session_lock` closure report "someone else already started this
/// graph" alongside the ordinary read/write failure modes, mirroring
/// `commands::agent_desk::RecordOutcome`.
enum CommitGraphOutcome {
    Started { session: AgentSession },
    AlreadyStarted,
    NotFound,
    Damaged { reason: String },
    WriteFailed { detail: String },
    Unavailable { detail: String },
}

/// Atomically re-checks "does the lead still have a proposal to consume"
/// and, if so, writes every provisioned helper as `Ready` (and any
/// dependency-gated helper as `Draft`) -- both inside ONE
/// `with_session_lock` acquisition, so no other concurrent caller of this
/// function for the same session can observe or act on an in-between state.
/// This is what makes concurrent `start_graph_at` calls for one proposal
/// safe: the slow, unlocked worktree provisioning happens before this call,
/// but only this call's locked re-check decides whether the write actually
/// happens.
fn commit_started_graph_if_still_proposed(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    lead_execution_id: &str,
    proposed: &ProposedGraph,
    provisioned: &[ProvisionedHelper],
) -> CommitGraphOutcome {
    locks.with_session_lock(session_id, || {
        let mut s = match store::read_session(root, session_id) {
            Ok(s) => s,
            Err(SessionLoadError::NotFound) => return CommitGraphOutcome::NotFound,
            Err(SessionLoadError::Io { detail }) => {
                return CommitGraphOutcome::Unavailable { detail }
            }
            Err(reason) => {
                return CommitGraphOutcome::Damaged {
                    reason: reason.to_string(),
                }
            }
        };

        let still_proposed = s
            .executions
            .iter()
            .any(|e| e.execution_id == lead_execution_id && e.proposed_graph.is_some());
        if !still_proposed {
            return CommitGraphOutcome::AlreadyStarted;
        }

        for exec in s.executions.iter_mut() {
            if exec.execution_id == lead_execution_id {
                exec.state = SessionState::Working;
                exec.proposed_graph = None;
            }
        }
        for job in &proposed.helpers {
            let now = now_rfc3339();
            let is_ready_now = provisioned.iter().find(|p| p.node_id == job.node_id);
            let (execution_id, state, worktree_path, branch, base_oid) = match is_ready_now {
                Some(p) => (
                    p.execution_id.clone(),
                    SessionState::Ready,
                    Some(p.path.clone()),
                    Some(p.branch.clone()),
                    p.base_oid.clone(),
                ),
                None => (
                    crate::agentdesk::execution_id_for_run_session(&new_id()),
                    SessionState::Draft,
                    None,
                    None,
                    None,
                ),
            };
            let mut record = ExecutionRecord::minimal(
                execution_id,
                s.header.session_id.clone(),
                Some(lead_execution_id.to_string()),
                state,
                now,
                None,
                0,
            );
            record.job_title = Some(job.title.clone());
            record.job_description = Some(job.description.clone());
            record.helper_role = Some(role_label(job.role));
            record.allowed_paths = job.allowed_paths.clone();
            record.worktree_path = worktree_path;
            record.branch = branch;
            // R6.7: the merge base `integrate_helper_result` diffs this
            // helper's worktree against once it finishes.
            record.base_oid = base_oid;
            // depends_on is stored by node_id at proposal time but the
            // schedulable field wants execution ids -- resolved by looking
            // up already-pushed records with a matching job_title is
            // fragile, so instead we resolve via node_id -> execution_id
            // using a side map built alongside this loop.
            s.executions.push(record);
        }
        // Second pass: now that every helper has a durable execution_id,
        // rewrite `depends_on` from node_id references to execution ids.
        let node_to_exec: std::collections::HashMap<String, ExecutionId> = proposed
            .helpers
            .iter()
            .zip(
                s.executions
                    .iter()
                    .rev()
                    .take(proposed.helpers.len())
                    .rev()
                    .map(|e| e.execution_id.clone()),
            )
            .map(|(job, exec_id)| (job.node_id.clone(), exec_id))
            .collect();
        let helper_count = proposed.helpers.len();
        let total = s.executions.len();
        for (job, exec) in proposed
            .helpers
            .iter()
            .zip(s.executions[total - helper_count..].iter_mut())
        {
            exec.depends_on = job
                .depends_on
                .iter()
                .filter_map(|dep_node_id| node_to_exec.get(dep_node_id).cloned())
                .collect();
        }
        s.header.active_execution_id = Some(lead_execution_id.to_string());
        s.header.state = SessionState::Working;
        s.header.updated_at = now_rfc3339();

        match store::write_session(root, &s) {
            Ok(()) => CommitGraphOutcome::Started { session: s },
            Err(e) => CommitGraphOutcome::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

fn role_label(role: graph::HelperRole) -> String {
    match role {
        graph::HelperRole::Researcher => "researcher".into(),
        graph::HelperRole::Builder => "builder".into(),
        graph::HelperRole::Verifier => "verifier".into(),
    }
}

fn short(execution_id: &str) -> String {
    execution_id.chars().take(8).collect()
}

/// The worktree's admin folder name (what `mark_as_run_worktree` needs) is
/// the target folder's own last path component -- matching how
/// `commands::worktree::add_worktree` derives it implicitly via git itself
/// naming the admin dir after the folder.
fn worktree_admin_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

// ---------------------------------------------------------------------------
// Launching a helper (R6.3, R6.4, R6.6)
// ---------------------------------------------------------------------------

/// Launches ONE already-`Ready` helper execution: resolves its policy,
/// discovers the provider transport rooted at its own worktree, registers
/// its cancellation handle in the SAME [`crate::agentdesk::ExecutionRegistry`]
/// solo runs use, then spawns `cli_run::run_task`.
///
/// Deliberately mirrors `commands::agent_desk::start_execution_at`'s tail
/// (register in the registry BEFORE the first `Working` event, spawn on the
/// async runtime, a watchdog task that turns a panic into a typed `Failed`
/// event) rather than importing/refactoring that function directly -- the
/// two entry points differ in where they get their prompt (a helper's own
/// `job_description`, not a session's source snapshot), their working
/// directory (the helper's own worktree, never the lead's), and their
/// policy (`ExecutionPolicy::resolve_for_helper`, not `SessionIntent`), and
/// forcing a single shared function to cover both would have made either
/// caller's argument list describe cases that do not apply to it.
///
/// Failure is reported into the transcript via the ordinary `Ended`/`Failed`
/// run-step path (same as any other execution failure) rather than as a
/// separate typed outcome on `agent_session_start_graph` -- once `Started`
/// has been returned, the graph is genuinely running, and a helper that
/// fails to launch is exactly the same kind of failure as one that launches
/// and then fails immediately: visible in that node's own row in the Graph
/// panel, never silently dropped.
fn launch_helper(
    app: &AppHandle,
    locks: &std::sync::Arc<crate::agentdesk::SessionLocks>,
    root: &SessionStoreRoot,
    links: &crate::agentdesk::RunSessionLinks,
    executions: &crate::agentdesk::ExecutionRegistry,
    session: &AgentSession,
    helper_execution_id: &str,
) {
    let Some(helper) = session
        .executions
        .iter()
        .find(|e| e.execution_id == helper_execution_id)
    else {
        // Should not happen: the caller just wrote this record. Nothing to
        // launch against, and nothing safe to report it against either.
        log::error!("agent desk graph: launch_helper called for an unknown execution {helper_execution_id}");
        return;
    };
    let Some(worktree_path) = helper.worktree_path.clone() else {
        record_helper_launch_failure(
            locks,
            root,
            &session.header.session_id,
            helper_execution_id,
            "This helper has no workspace to run in.",
        );
        return;
    };
    let repo_id = session.header.repo_id.clone();
    let session_id = session.header.session_id.clone();
    let helper_execution_id = helper_execution_id.to_string();
    let job_title = helper.job_title.clone().unwrap_or_else(|| "Helper".to_string());
    let job_description = helper.job_description.clone().unwrap_or_default();
    let allowed_paths = helper.allowed_paths.clone();
    // Builder always writes; a Researcher/Verifier writes only if it was
    // explicitly given an allowance -- the same rule `graph::validate_graph`
    // already enforces at proposal time (`MissingAllowedPaths`), so a
    // helper that reached `Ready` at all is guaranteed to satisfy it.
    let can_write = helper.helper_role.as_deref() == Some("builder") || !allowed_paths.is_empty();
    let policy = crate::agentdesk::policy::ExecutionPolicy::resolve_for_helper(can_write, allowed_paths);

    // The repo->session link is almost certainly already set by the lead's
    // own `start_execution_at` (or by an earlier helper launch in this same
    // batch) -- `RunSessionLinks::link` is an idempotent overwrite of the
    // same value either way, so calling it again here is harmless and keeps
    // this function correct even if it is ever called before the lead links
    // anything (a lead that finished before its helpers, then later helper
    // dependents starting after a re-schedule).
    links.link(&repo_id, &session_id);

    let agent = match crate::ai::agent::cli_agent::CliAgent::discover(std::path::PathBuf::from(&worktree_path)) {
        Ok(a) => a,
        Err(e) => {
            record_helper_launch_failure(
                locks,
                root,
                &session_id,
                &helper_execution_id,
                &crate::ai::agent::select::plain_explanation(&e),
            );
            return;
        }
    };

    let (answer_tx, answer_rx) = std::sync::mpsc::channel::<crate::airun::driver::GateAnswer>();
    crate::commands::airun::gate_answers()
        .lock()
        .unwrap()
        .insert((session_id.clone(), helper_execution_id.clone()), answer_tx);

    // R6.3: registered BEFORE the spawned task can emit a single `Working`
    // event -- identical guarantee to `start_execution_at`'s own comment on
    // this exact ordering.
    let cancel_handle = crate::airun::cli_run::CancelHandle::new();
    executions.register(session_id.clone(), helper_execution_id.clone(), cancel_handle.clone());

    let prompt = format!(
        "{}\n\n{}",
        job_title,
        if job_description.is_empty() { "No further detail was given." } else { &job_description }
    );

    let app_for_task = app.clone();
    let repo_for_task = repo_id.clone();
    let session_id_for_task = session_id.clone();
    let helper_execution_id_for_task = helper_execution_id.clone();
    let executions_for_task = executions.clone();
    let root_for_task = root.clone();
    let locks_for_task = locks.clone();
    let join_handle = tauri::async_runtime::spawn(async move {
        let sink: crate::airun::engine::Sink = {
            let app = app_for_task.clone();
            let repo = repo_for_task.clone();
            let sid = helper_execution_id_for_task.clone();
            std::sync::Arc::new(move |state, step| {
                crate::commands::airun::emit_agent_desk_only(&app, &repo, &sid, state, step);
            })
        };

        crate::airun::cli_run::run_task(
            &agent,
            &format!("{}\n\nThe task:\n{}", crate::ai::agent::run::SYSTEM_PROMPT, prompt),
            sink,
            answer_rx,
            policy,
            true,
            cancel_handle,
        )
        .await;

        crate::commands::airun::gate_answers()
            .lock()
            .unwrap()
            .remove(&(session_id_for_task.clone(), helper_execution_id_for_task.clone()));
        executions_for_task.complete(&session_id_for_task, &helper_execution_id_for_task);

        // R6.7: once this helper is done (however it ended), re-run the
        // scheduler against the session's current executions and launch
        // whatever became dependency-ready as a result, then integrate this
        // helper's own result if it finished cleanly. Both steps read the
        // freshly-persisted state under the session lock rather than the
        // stale `session` this closure captured, since the bridge may have
        // written several more events to this execution's record since this
        // task started.
        advance_graph_after_helper_completion(
            &app_for_task,
            &locks_for_task,
            &root_for_task,
            &session_id_for_task,
            &helper_execution_id_for_task,
        );
    });

    let app_for_watchdog = app.clone();
    let repo_for_watchdog = repo_id;
    let session_id_for_watchdog = session_id.clone();
    let helper_execution_id_for_watchdog = helper_execution_id.clone();
    let executions_for_watchdog = executions.clone();
    let locks_for_watchdog = locks.clone();
    let root_for_watchdog = root.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(join_error) = join_handle.await {
            log::error!(
                "agent desk helper {} for session {} panicked: {join_error}",
                helper_execution_id_for_watchdog,
                session_id_for_watchdog
            );
            crate::commands::airun::emit_agent_desk_only(
                &app_for_watchdog,
                &repo_for_watchdog,
                &helper_execution_id_for_watchdog,
                crate::airun::driver::RunState::Failed,
                crate::airun::driver::RunStep::Ended {
                    state: crate::airun::driver::RunState::Failed,
                    detail: "Something went wrong while starting this helper, and it never got to report why. Nothing was committed and its own workspace is untouched.".into(),
                },
            );
            crate::commands::airun::gate_answers()
                .lock()
                .unwrap()
                .remove(&(session_id_for_watchdog.clone(), helper_execution_id_for_watchdog.clone()));
            executions_for_watchdog.complete(&session_id_for_watchdog, &helper_execution_id_for_watchdog);
            // A panicked helper still leaves the graph responsible for
            // re-scheduling and eventual completion -- without this, a
            // dependent helper waiting on the panicked one would never be
            // launched, and the graph would look permanently stuck.
            advance_graph_after_helper_completion(
                &app_for_watchdog,
                &locks_for_watchdog,
                &root_for_watchdog,
                &session_id_for_watchdog,
                &helper_execution_id_for_watchdog,
            );
        }
    });
}

/// Records a helper's launch failure directly (no engine ever started, so
/// there is no `run_task`/sink to report through) -- moves the execution to
/// `Failed` with a plain-language detail, visible in that node's own row.
fn record_helper_launch_failure(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: &str,
    detail: &str,
) {
    let _ = update_session_at(locks, root, session_id, |s| {
        if let Some(exec) = s.executions.iter_mut().find(|e| e.execution_id == execution_id) {
            exec.state = SessionState::Failed;
            exec.ended_at = Some(now_rfc3339());
            exec.output_summary = Some(detail.to_string());
        }
    });
}

/// R6.7: after one helper finishes (in whatever order it happened to
/// finish), re-run the scheduler and launch every newly dependency-ready
/// helper, then integrate this helper's own result if it finished cleanly.
///
/// Called from every helper's own completion path -- helpers finish in an
/// arbitrary order relative to each other, so "completion order" (R6.7's own
/// wording) is realized by each completion independently triggering the next
/// step, rather than by a single central loop that would have to guess when
/// a batch is "done."
fn advance_graph_after_helper_completion(
    app: &AppHandle,
    locks: &std::sync::Arc<crate::agentdesk::SessionLocks>,
    root: &SessionStoreRoot,
    session_id: &str,
    finished_execution_id: &str,
) {
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(_) => return,
    };

    integrate_helper_result(locks, root, session_id, finished_execution_id, &session);

    // Re-read after integration may have changed this node's state.
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(_) => return,
    };
    let decision = graph::schedule(&session.executions);
    if decision.ready.is_empty() {
        maybe_finish_graph(locks, root, session_id, &session);
        return;
    }

    let links = match app.try_state::<crate::agentdesk::RunSessionLinks>() {
        Some(l) => l,
        None => return,
    };
    let executions = match app.try_state::<crate::agentdesk::ExecutionRegistry>() {
        Some(e) => e,
        None => return,
    };

    for ready_execution_id in &decision.ready {
        launch_helper(
            app,
            locks,
            root,
            links.inner(),
            executions.inner(),
            &session,
            ready_execution_id,
        );
    }
}

/// Repo-relative paths this worktree's HEAD changed relative to `base_oid`,
/// via `git2`'s tree diff (never a shell-out) -- the same
/// `worktree_path`/`base_oid` pair `ExecutionRecord` already persists for
/// this exact purpose (R6.7's doc comment). A helper that made no commit at
/// all (a run that only left uncommitted worktree edits, or one that never
/// wrote anything) is not covered by this diff -- comparing committed trees
/// is what stays correct across a worktree that may have since been
/// discarded, which uncommitted-file scanning would not survive.
fn changed_files_since(worktree_path: &str, base_oid: &str) -> Result<Vec<String>, git2::Error> {
    let repo = git2::Repository::open(worktree_path)?;
    let base = git2::Oid::from_str(base_oid)?;
    let base_tree = repo.find_commit(base)?.tree()?;
    let head_tree = repo.head()?.peel_to_tree()?;
    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;
    let mut paths = Vec::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path().and_then(|p| p.to_str()) {
            paths.push(path.to_string());
        }
    }
    Ok(paths)
}

/// The full text of `path` as it stood in the tree at `base_oid`, or an
/// empty string for a file that did not exist there yet (a file the helper
/// created from scratch) -- matching `graph::detect_conflict`'s own
/// "helper never touched this file" reasoning: a blob that is genuinely
/// absent at the base is a legitimate `base_text`, not a read failure.
fn read_file_at_revision(worktree_path: &str, base_oid: &str, path: &str) -> Option<String> {
    let repo = git2::Repository::open(worktree_path).ok()?;
    let base = git2::Oid::from_str(base_oid).ok()?;
    let tree = repo.find_commit(base).ok()?.tree().ok()?;
    let entry = match tree.get_path(std::path::Path::new(path)) {
        Ok(e) => e,
        Err(_) => return Some(String::new()),
    };
    let blob = repo.find_blob(entry.id()).ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// R6.7's conflict-detection half: when a helper finishes cleanly, compare
/// its own worktree's changed files against the lead's checkout (the
/// integration target). A file changed by BOTH the helper and something
/// already integrated (another finished sibling, or the lead's own edits) is
/// reported as a typed [`IntegrationConflict`] via
/// `agent_session_record_conflict`'s own write path -- both texts preserved,
/// neither committed (tasks.md 5.3/5.4, `graph::detect_conflict`).
///
/// A helper that did not finish cleanly (`Stopped`/`Failed`/`Interrupted`)
/// has nothing to integrate -- its worktree stands as evidence of what it
/// was doing, but nothing from it is folded into the lead's tree.
fn integrate_helper_result(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    helper_execution_id: &str,
    session: &AgentSession,
) {
    let Some(helper) = session
        .executions
        .iter()
        .find(|e| e.execution_id == helper_execution_id)
    else {
        return;
    };
    if helper.state != SessionState::Finished {
        return;
    }
    let (Some(worktree_path), Some(base_oid)) = (&helper.worktree_path, &helper.base_oid) else {
        // No base revision recorded (a helper launched before this field was
        // wired, or one whose worktree failed to provision cleanly) -- there
        // is no merge base to diff against, so this helper's changes are
        // left for the lead/user to review manually via the worktree path
        // already shown in the inspector, rather than guessing at a base.
        return;
    };
    let lead_path = &session.header.repo_path;

    let changed = match changed_files_since(worktree_path, base_oid) {
        Ok(files) => files,
        Err(_) => return,
    };

    for path in changed {
        let base_text = read_file_at_revision(worktree_path, base_oid, &path).unwrap_or_default();
        let helper_text = std::fs::read_to_string(std::path::Path::new(worktree_path).join(&path)).unwrap_or_default();
        let integrated_text = std::fs::read_to_string(std::path::Path::new(lead_path).join(&path)).unwrap_or_default();

        let sibling = session
            .executions
            .iter()
            .find(|e| e.parent_execution_id.is_some() && e.execution_id != helper_execution_id && e.state == SessionState::Finished);
        let conflicting_with = sibling
            .map(|s| s.execution_id.clone())
            .unwrap_or_else(|| "lead".to_string());

        if let graph::IntegrationState::Conflicted { conflict } =
            graph::detect_conflict(&path, &conflicting_with, &base_text, &helper_text, &integrated_text)
        {
            let _ = update_session_at(locks, root, session_id, |s| {
                if let Some(exec) = s.executions.iter_mut().find(|e| e.execution_id == helper_execution_id) {
                    exec.conflict = Some(conflict.clone());
                    exec.state = SessionState::NeedsInput;
                }
            });
            // Only the first conflicted file for this helper is recorded per
            // pass -- `agent_session_resolve_conflict` clears it and a later
            // re-run of this function (triggered by the next graph event)
            // will surface the next one, so nothing is lost, only shown one
            // at a time.
            return;
        }
    }
}

/// R6.9: once every node in the graph has reached a terminal state
/// (`Finished`/`Stopped`/`Failed`/`Interrupted` -- nothing left `Ready`,
/// `Draft`, `Preparing`, `Working`, or blocked-with-a-conflict
/// `NeedsInput`), mark the LEAD's own record with a combined summary so the
/// Graph panel and the session header both read as genuinely complete
/// rather than silently going quiet once the last helper's process exits.
///
/// This is a minimal, honest substitute for a full "lead re-reads every
/// helper's output and writes a real review message" pass -- that requires
/// handing the lead's own live conversation a fresh turn with every helper's
/// `output_summary` as context, which is a second engine invocation this
/// change does not yet wire (see this task cluster's report for what stays
/// unwired). What this function DOES guarantee is that the graph is never
/// left looking like it is still working once nothing is.
fn maybe_finish_graph(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    session: &AgentSession,
) {
    let Some(lead) = session.executions.iter().find(|e| e.parent_execution_id.is_none()) else {
        return;
    };
    if lead.state != SessionState::Working {
        return;
    }
    let helpers: Vec<&ExecutionRecord> = session
        .executions
        .iter()
        .filter(|e| e.parent_execution_id.is_some())
        .collect();
    if helpers.is_empty() {
        return;
    }
    let all_terminal = helpers.iter().all(|h| {
        matches!(
            h.state,
            SessionState::Finished | SessionState::Stopped | SessionState::Failed | SessionState::Interrupted
        )
    });
    if !all_terminal {
        return;
    }
    let finished = helpers.iter().filter(|h| h.state == SessionState::Finished).count();
    let _ = update_session_at(locks, root, session_id, |s| {
        if let Some(lead) = s.executions.iter_mut().find(|e| e.parent_execution_id.is_none()) {
            lead.state = SessionState::Finished;
            lead.ended_at = Some(now_rfc3339());
            lead.output_summary = Some(format!("{finished} of {} helpers finished.", helpers.len()));
        }
        s.header.state = SessionState::Finished;
    });
}

// ---------------------------------------------------------------------------
// Use solo instead
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UseSoloOutcome {
    /// The proposal was discarded; the session is back to a plain
    /// `Ready` session with no graph, so the ordinary
    /// `agent_session_start_execution` (solo) path can be used.
    Cleared { session: AgentSession },
    NoProposal,
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_use_solo_instead(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
) -> Result<UseSoloOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();

    let session = match store::read_session(&root, &session_id) {
        Ok(s) => s,
        Err(SessionLoadError::NotFound) => return Ok(UseSoloOutcome::NotFound),
        Err(SessionLoadError::Io { detail }) => {
            return Ok(UseSoloOutcome::Unavailable { detail })
        }
        Err(reason) => {
            return Ok(UseSoloOutcome::Damaged {
                reason: reason.to_string(),
            })
        }
    };
    let had_proposal = session
        .executions
        .iter()
        .any(|e| e.proposed_graph.is_some());
    if !had_proposal {
        return Ok(UseSoloOutcome::NoProposal);
    }

    let outcome = update_session_at(&locks_arc, &root, &session_id, |s| {
        s.executions.retain(|e| e.proposed_graph.is_none());
        s.header.active_execution_id = None;
        s.header.state = SessionState::Ready;
    });
    Ok(match outcome {
        UpdateOutcome::Updated { session } => UseSoloOutcome::Cleared { session },
        UpdateOutcome::NotFound => UseSoloOutcome::NotFound,
        UpdateOutcome::Damaged { reason } => UseSoloOutcome::Damaged { reason },
        UpdateOutcome::WriteFailed { detail } => UseSoloOutcome::WriteFailed { detail },
        UpdateOutcome::Unavailable { detail } => UseSoloOutcome::Unavailable { detail },
    })
}

// ---------------------------------------------------------------------------
// Graph projection for the UI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphViewOutcome {
    Found { nodes: Vec<GraphNodeView> },
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_graph_view(
    app: AppHandle,
    session_id: SessionId,
) -> Result<GraphViewOutcome, AppError> {
    let root = resolve_root(&app)?;
    Ok(match store::read_session(&root, &session_id) {
        Ok(s) => GraphViewOutcome::Found {
            nodes: graph::project_graph(&s.executions),
        },
        Err(SessionLoadError::NotFound) => GraphViewOutcome::NotFound,
        Err(SessionLoadError::Io { detail }) => GraphViewOutcome::Unavailable { detail },
        Err(reason) => GraphViewOutcome::Damaged {
            reason: reason.to_string(),
        },
    })
}

// ---------------------------------------------------------------------------
// Conflict resolution (tasks.md 5.3, 5.4)
// ---------------------------------------------------------------------------

/// Which side of a conflict the user picked, or that they supplied their own
/// merged text -- never inferred, always an explicit user action so neither
/// copy is silently preferred.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConflictResolution {
    KeepHelper,
    KeepIntegrated,
    UseMerged { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResolveConflictOutcome {
    /// The conflict was cleared on this node; only THIS node's integration
    /// resumes (tasks.md 5.4) -- no other node's state is touched.
    Resolved {
        session: AgentSession,
        resolved_text: String,
    },
    NoConflict,
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
}

#[tauri::command]
#[specta::specta]
pub async fn agent_session_resolve_conflict(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    execution_id: ExecutionId,
    resolution: ConflictResolution,
) -> Result<ResolveConflictOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    Ok(resolve_conflict_at(
        &locks_arc,
        &root,
        &session_id,
        &execution_id,
        resolution,
    ))
}

fn resolve_conflict_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: &str,
    resolution: ConflictResolution,
) -> ResolveConflictOutcome {
    let mut resolved_text = String::new();
    let mut found = false;
    let outcome = update_session_at(locks, root, session_id, |s| {
        if let Some(exec) = s
            .executions
            .iter_mut()
            .find(|e| e.execution_id == execution_id)
        {
            if let Some(conflict) = exec.conflict.take() {
                found = true;
                resolved_text = match &resolution {
                    ConflictResolution::KeepHelper => conflict.helper_text.clone(),
                    ConflictResolution::KeepIntegrated => conflict.integrated_text.clone(),
                    ConflictResolution::UseMerged { text } => text.clone(),
                };
                // This node's own integration resumes -- its state moves
                // out of the conflicted marker back toward Finished, which
                // is what lets `schedule()`/`project_graph()` treat it as
                // done again. Sibling nodes were never touched by this
                // write (this closure only mutates the one matching
                // `execution_id`), so their own pending integrations are
                // unaffected (tasks.md 5.4).
                exec.state = SessionState::Finished;
                exec.output_summary = Some("Resolved: conflict cleared".into());
            }
        }
    });

    if !found {
        return ResolveConflictOutcome::NoConflict;
    }

    match outcome {
        UpdateOutcome::Updated { session } => ResolveConflictOutcome::Resolved {
            session,
            resolved_text,
        },
        UpdateOutcome::NotFound => ResolveConflictOutcome::NotFound,
        UpdateOutcome::Damaged { reason } => ResolveConflictOutcome::Damaged { reason },
        UpdateOutcome::WriteFailed { detail } => ResolveConflictOutcome::WriteFailed { detail },
        UpdateOutcome::Unavailable { detail } => ResolveConflictOutcome::Unavailable { detail },
    }
}

/// Records a conflict on a helper's execution -- called by the integration
/// step (tasks.md 5.1/5.2/5.3), exposed as a command for tests and for a
/// future integration-runner caller to invoke without duplicating the
/// read-modify-write.
#[tauri::command]
#[specta::specta]
pub async fn agent_session_record_conflict(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
    execution_id: ExecutionId,
    conflict: IntegrationConflict,
) -> Result<ResolveConflictOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    let outcome = update_session_at(&locks_arc, &root, &session_id, |s| {
        if let Some(exec) = s
            .executions
            .iter_mut()
            .find(|e| e.execution_id == execution_id)
        {
            exec.conflict = Some(conflict.clone());
            // The node's OWN integration pauses; siblings are untouched
            // (tasks.md 5.3 "does not stop unrelated helpers").
            exec.state = SessionState::NeedsInput;
        }
    });
    Ok(match outcome {
        UpdateOutcome::Updated { session } => ResolveConflictOutcome::Resolved {
            session,
            resolved_text: String::new(),
        },
        UpdateOutcome::NotFound => ResolveConflictOutcome::NotFound,
        UpdateOutcome::Damaged { reason } => ResolveConflictOutcome::Damaged { reason },
        UpdateOutcome::WriteFailed { detail } => ResolveConflictOutcome::WriteFailed { detail },
        UpdateOutcome::Unavailable { detail } => ResolveConflictOutcome::Unavailable { detail },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::graph::{CompletionCondition, JobBudget};
    use crate::agentdesk::model::{
        AgentSessionHeader, SessionIntent, SessionSource, CURRENT_SCHEMA_VERSION,
    };

    fn temp_root() -> (tempfile::TempDir, SessionStoreRoot) {
        let dir = tempfile::tempdir().unwrap();
        let root = SessionStoreRoot::at(dir.path().to_path_buf()).unwrap();
        (dir, root)
    }

    fn seed_session(root: &SessionStoreRoot, session_id: &str) {
        let header = AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/widgets".into(),
            repo_name: "widgets".into(),
            title: "Fix the bug".into(),
            source: SessionSource::Manual {
                repo_id: "repo-1".into(),
            },
            intent: SessionIntent::Fix,
            state: SessionState::Ready,
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
            unread: false,
            changed_file_count: 0,
            active_execution_id: None,
            archived: false,
        };
        let session = AgentSession::new(header);
        store::write_session(root, &session).unwrap();
    }

    fn sample_graph() -> ProposedGraph {
        ProposedGraph {
            lead_summary: "Split the fix into research + build".into(),
            helpers: vec![
                ProposedHelperJob {
                    node_id: "research".into(),
                    title: "Trace the crash".into(),
                    description: "Find where the null deref happens".into(),
                    role: HelperRole::Researcher,
                    allowed_paths: vec![],
                    depends_on: vec![],
                    budget: JobBudget::default(),
                    completion: CompletionCondition::ReportsResult,
                },
                ProposedHelperJob {
                    node_id: "build".into(),
                    title: "Add the guard".into(),
                    description: "Add the null check".into(),
                    role: HelperRole::Builder,
                    allowed_paths: vec!["src/**".into()],
                    depends_on: vec!["research".into()],
                    budget: JobBudget::default(),
                    completion: CompletionCondition::ReportsResult,
                },
            ],
            proposed_at: now_rfc3339(),
        }
    }

    #[test]
    fn propose_graph_persists_as_awaiting_start_and_does_not_run_anything() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");

        let outcome = propose_graph_at(&locks, &root, "sess-1", sample_graph());
        match outcome {
            ProposeGraphOutcome::AwaitingStart { session, execution_id } => {
                let lead = session
                    .executions
                    .iter()
                    .find(|e| e.execution_id == execution_id)
                    .unwrap();
                assert_eq!(lead.state, SessionState::NeedsInput);
                assert!(lead.proposed_graph.is_some());
                assert_eq!(session.header.state, SessionState::NeedsInput);
            }
            other => panic!("expected AwaitingStart, got {other:?}"),
        }
    }

    #[test]
    fn an_invalid_proposal_is_rejected_before_anything_is_persisted() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");

        let mut bad = sample_graph();
        bad.helpers.push(ProposedHelperJob {
            node_id: "extra1".into(),
            title: "x".into(),
            description: String::new(),
            role: HelperRole::Verifier,
            allowed_paths: vec![],
            depends_on: vec![],
            budget: JobBudget::default(),
            completion: CompletionCondition::ReportsResult,
        });
        bad.helpers.push(ProposedHelperJob {
            node_id: "extra2".into(),
            title: "y".into(),
            description: String::new(),
            role: HelperRole::Verifier,
            allowed_paths: vec![],
            depends_on: vec![],
            budget: JobBudget::default(),
            completion: CompletionCondition::ReportsResult,
        });

        let outcome = propose_graph_at(&locks, &root, "sess-1", bad);
        assert!(matches!(outcome, ProposeGraphOutcome::Invalid { .. }));

        // Nothing was persisted -- rereading shows no executions at all.
        let session = store::read_session(&root, "sess-1").unwrap();
        assert!(session.executions.is_empty());
    }

    #[test]
    fn use_solo_clears_a_proposal_and_returns_to_ready() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        propose_graph_at(&locks, &root, "sess-1", sample_graph());

        let session = store::read_session(&root, "sess-1").unwrap();
        assert!(!session.executions.is_empty());

        let had_proposal = session.executions.iter().any(|e| e.proposed_graph.is_some());
        assert!(had_proposal);

        let outcome = update_session_at(&locks, &root, "sess-1", |s| {
            s.executions.retain(|e| e.proposed_graph.is_none());
            s.header.active_execution_id = None;
            s.header.state = SessionState::Ready;
        });
        match outcome {
            UpdateOutcome::Updated { session } => {
                assert!(session.executions.is_empty());
                assert_eq!(session.header.state, SessionState::Ready);
            }
            other => panic!("expected Updated, got {other:?}"),
        }
    }

    #[test]
    fn record_conflict_then_resolve_keep_helper_preserves_both_texts_until_resolved() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");

        update_session_at(&locks, &root, "sess-1", |s| {
            s.executions.push(ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Working,
                now_rfc3339(),
                None,
                0,
            ));
        });

        let conflict = IntegrationConflict {
            path: "src/greet.rs".into(),
            conflicting_with: "helper-b".into(),
            base_text: "base".into(),
            helper_text: "helper-a-version".into(),
            integrated_text: "helper-b-version".into(),
        };

        let outcome = update_session_at(&locks, &root, "sess-1", |s| {
            if let Some(exec) = s.executions.iter_mut().find(|e| e.execution_id == "helper-a") {
                exec.conflict = Some(conflict.clone());
                exec.state = SessionState::NeedsInput;
            }
        });
        assert!(matches!(outcome, UpdateOutcome::Updated { .. }));

        // Both copies survive on disk until resolved.
        let session = store::read_session(&root, "sess-1").unwrap();
        let helper_a = session
            .executions
            .iter()
            .find(|e| e.execution_id == "helper-a")
            .unwrap();
        assert_eq!(helper_a.state, SessionState::NeedsInput);
        let stored_conflict = helper_a.conflict.as_ref().unwrap();
        assert_eq!(stored_conflict.helper_text, "helper-a-version");
        assert_eq!(stored_conflict.integrated_text, "helper-b-version");

        let resolve_outcome = resolve_conflict_at(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            ConflictResolution::KeepHelper,
        );
        match resolve_outcome {
            ResolveConflictOutcome::Resolved { session, resolved_text } => {
                assert_eq!(resolved_text, "helper-a-version");
                let helper_a = session
                    .executions
                    .iter()
                    .find(|e| e.execution_id == "helper-a")
                    .unwrap();
                assert!(helper_a.conflict.is_none());
                assert_eq!(helper_a.state, SessionState::Finished);
            }
            other => panic!("expected Resolved, got {other:?}"),
        }
    }

    #[test]
    fn resolving_one_nodes_conflict_never_touches_a_sibling_node() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");

        update_session_at(&locks, &root, "sess-1", |s| {
            let mut a = ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::NeedsInput,
                now_rfc3339(),
                None,
                0,
            );
            a.conflict = Some(IntegrationConflict {
                path: "a.rs".into(),
                conflicting_with: "helper-b".into(),
                base_text: "base".into(),
                helper_text: "a-text".into(),
                integrated_text: "b-text".into(),
            });
            s.executions.push(a);
            // Sibling helper-c is mid-flight and unrelated to this conflict.
            s.executions.push(ExecutionRecord::minimal(
                "helper-c".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Working,
                now_rfc3339(),
                None,
                0,
            ));
        });

        let outcome = resolve_conflict_at(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            ConflictResolution::KeepIntegrated,
        );
        match outcome {
            ResolveConflictOutcome::Resolved { session, .. } => {
                let helper_c = session
                    .executions
                    .iter()
                    .find(|e| e.execution_id == "helper-c")
                    .unwrap();
                // Untouched: still Working, exactly as before the resolve.
                assert_eq!(helper_c.state, SessionState::Working);
            }
            other => panic!("expected Resolved, got {other:?}"),
        }
    }

    #[test]
    fn resolving_a_node_with_no_conflict_is_reported_not_silently_accepted() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            s.executions.push(ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Working,
                now_rfc3339(),
                None,
                0,
            ));
        });

        let outcome = resolve_conflict_at(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            ConflictResolution::KeepHelper,
        );
        assert!(matches!(outcome, ResolveConflictOutcome::NoConflict));
    }

    #[test]
    fn graph_projection_reflects_persisted_executions_with_no_separate_truth() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            let mut lead = ExecutionRecord::minimal(
                "lead".into(),
                s.header.session_id.clone(),
                None,
                SessionState::Working,
                now_rfc3339(),
                None,
                0,
            );
            lead.job_title = None;
            s.executions.push(lead);
            let mut helper = ExecutionRecord::minimal(
                "h1".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Ready,
                now_rfc3339(),
                None,
                0,
            );
            helper.job_title = Some("Trace the crash".into());
            helper.helper_role = Some("researcher".into());
            s.executions.push(helper);
        });

        let session = store::read_session(&root, "sess-1").unwrap();
        let views = graph::project_graph(&session.executions);
        assert_eq!(views.len(), 2);
        let lead_view = views.iter().find(|v| v.execution_id == "lead").unwrap();
        assert!(lead_view.is_lead);
        assert_eq!(lead_view.title, "Lead agent");
        let helper_view = views.iter().find(|v| v.execution_id == "h1").unwrap();
        assert!(!helper_view.is_lead);
        assert_eq!(helper_view.title, "Trace the crash");
    }

    /// Regression test for the concurrent-Start defect: two callers racing
    /// `start_graph_at`'s final locked write step for the SAME lead
    /// proposal must not both win. Before the fix, that write happened
    /// unconditionally once `start_graph_at` reached it -- there was no
    /// re-check that the lead still had a proposal to consume, so if two
    /// calls both got that far (both having captured the same proposal
    /// during their earlier UNLOCKED reads and unlocked worktree
    /// provisioning), both would push a full set of helper `ExecutionRecord`s
    /// on top of each other.
    ///
    /// `commit_started_graph_if_still_proposed` is the atomic replacement:
    /// re-check and write inside one `with_session_lock` acquisition. This
    /// test drives it directly with two real threads and a barrier so they
    /// contend for the same lock -- the only way to make the pre-fix race
    /// reproduce (two sequential calls cannot expose it: the second would
    /// simply see the proposal already gone from `start_graph_at`'s own
    /// earlier, correct, step-1 check, same as
    /// `agent_desk::concurrent_start_attempts_yield_exactly_one_started_and_one_already_running`
    /// documents for the sibling single-execution path). Bypassing the full
    /// `start_graph_at` also avoids real `git worktree add` collisions
    /// between the two threads (both would otherwise race for the same
    /// branch name), which is orthogonal to the defect this test targets.
    #[test]
    fn two_concurrent_starts_on_one_proposal_produce_exactly_one_set_of_helpers() {
        let (_dir, root) = temp_root();
        let locks = std::sync::Arc::new(crate::agentdesk::SessionLocks::new());
        seed_session(&root, "sess-1");

        let ProposeGraphOutcome::AwaitingStart { execution_id: lead_execution_id, .. } =
            propose_graph_at(&locks, &root, "sess-1", sample_graph())
        else {
            panic!("expected AwaitingStart");
        };

        let proposed = sample_graph();
        // Both callers provisioned real worktrees for the same helper
        // (`research`, the only dependency-free job) under DIFFERENT paths
        // -- exactly what two racing `start_graph_at` calls would each have
        // done unlocked, before either reaches this shared write step.
        let provisioned_a = vec![ProvisionedHelper {
            node_id: "research".into(),
            execution_id: "helper-from-a".into(),
            branch: "agent-desk/lead/research-a".into(),
            path: "C:/fake/worktree-a".into(),
            base_oid: Some("deadbeef".into()),
        }];
        let provisioned_b = vec![ProvisionedHelper {
            node_id: "research".into(),
            execution_id: "helper-from-b".into(),
            branch: "agent-desk/lead/research-b".into(),
            path: "C:/fake/worktree-b".into(),
            base_oid: Some("deadbeef".into()),
        }];

        let barrier = std::sync::Barrier::new(2);
        let root_ref = &root;
        let locks_ref = &locks;
        let lead_ref = lead_execution_id.as_str();
        let proposed_ref = &proposed;

        let (outcome_a, outcome_b) = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                commit_started_graph_if_still_proposed(
                    locks_ref,
                    root_ref,
                    "sess-1",
                    lead_ref,
                    proposed_ref,
                    &provisioned_a,
                )
            });
            let b = scope.spawn(|| {
                barrier.wait();
                commit_started_graph_if_still_proposed(
                    locks_ref,
                    root_ref,
                    "sess-1",
                    lead_ref,
                    proposed_ref,
                    &provisioned_b,
                )
            });
            (a.join().unwrap(), b.join().unwrap())
        });

        let outcomes = [&outcome_a, &outcome_b];
        let started = outcomes
            .iter()
            .filter(|o| matches!(o, CommitGraphOutcome::Started { .. }))
            .count();
        let already_started = outcomes
            .iter()
            .filter(|o| matches!(o, CommitGraphOutcome::AlreadyStarted))
            .count();
        assert_eq!(started, 1, "exactly one attempt must win and write its helpers");
        assert_eq!(
            already_started, 1,
            "the other attempt must see AlreadyStarted, not also win"
        );

        // Exactly one set of helper records exists -- not doubled. Before
        // the fix this would be 4 (two full sets of the sample graph's 2
        // helpers) instead of 2.
        let session = store::read_session(&root, "sess-1").unwrap();
        let helper_records: Vec<_> = session
            .executions
            .iter()
            .filter(|e| e.parent_execution_id.is_some())
            .collect();
        assert_eq!(
            helper_records.len(),
            sample_graph().helpers.len(),
            "only the winning attempt's helper records may be persisted, never both"
        );

        // The winning set of helper records references exactly one of the
        // two candidate worktree paths -- never both, and never neither.
        let ready_helper = helper_records
            .iter()
            .find(|e| e.worktree_path.is_some())
            .expect("the ready helper (research) must be among the winning records");
        let winning_path = ready_helper.worktree_path.clone().unwrap();
        assert!(
            winning_path == "C:/fake/worktree-a" || winning_path == "C:/fake/worktree-b",
            "winning worktree path must be exactly one candidate's, got {winning_path}"
        );
    }

    // -- changed_files_since / read_file_at_revision (R6.7) --

    fn init_repo_with_commit(dir: &std::path::Path, files: &[(&str, &str)]) -> (git2::Repository, String) {
        let repo = git2::Repository::init(dir).expect("repo");
        {
            let mut config = repo.config().expect("config");
            config.set_str("user.name", "Graph Test").expect("name");
            config.set_str("user.email", "graph@example.com").expect("email");
        }
        for (name, content) in files {
            std::fs::write(dir.join(name), content).expect("write");
        }
        let mut index = repo.index().expect("index");
        for (name, _) in files {
            index.add_path(std::path::Path::new(name)).expect("add");
        }
        index.write().expect("write index");
        // Scoped so the `Tree` (which borrows `repo`) is dropped before `repo`
        // is moved into the return value.
        let oid = {
            let tree = repo.find_tree(index.write_tree().expect("tree id")).expect("tree");
            let sig = git2::Signature::now("Graph Test", "graph@example.com").expect("sig");
            repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
                .expect("commit")
        };
        (repo, oid.to_string())
    }

    fn commit_all(repo: &git2::Repository, message: &str) {
        let mut index = repo.index().expect("index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("add all");
        index.write().expect("write index");
        let tree = repo.find_tree(index.write_tree().expect("tree id")).expect("tree");
        let sig = git2::Signature::now("Graph Test", "graph@example.com").expect("sig");
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .expect("commit");
    }

    #[test]
    fn changed_files_since_lists_only_paths_touched_after_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a"), ("b.txt", "b")]);
        std::fs::write(dir.path().join("a.txt"), "a-changed").unwrap();
        commit_all(&repo, "change a");

        let changed = changed_files_since(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert_eq!(changed, vec!["a.txt".to_string()]);
    }

    #[test]
    fn changed_files_since_reports_nothing_when_head_equals_base() {
        let dir = tempfile::tempdir().unwrap();
        let (_repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a")]);
        let changed = changed_files_since(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert!(changed.is_empty());
    }

    #[test]
    fn read_file_at_revision_reads_the_base_text() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "original")]);
        std::fs::write(dir.path().join("a.txt"), "changed").unwrap();
        commit_all(&repo, "change a");

        let base_text = read_file_at_revision(dir.path().to_str().unwrap(), &base_oid, "a.txt");
        assert_eq!(base_text.as_deref(), Some("original"));
    }

    #[test]
    fn read_file_at_revision_returns_empty_string_for_a_file_that_did_not_exist_at_base() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a")]);
        std::fs::write(dir.path().join("new.txt"), "brand new").unwrap();
        commit_all(&repo, "add new.txt");

        let base_text = read_file_at_revision(dir.path().to_str().unwrap(), &base_oid, "new.txt");
        assert_eq!(base_text.as_deref(), Some(""));
    }

    // -- record_helper_launch_failure / maybe_finish_graph (R6.9) --

    #[test]
    fn record_helper_launch_failure_marks_only_the_named_execution_failed() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            s.executions.push(ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Ready,
                now_rfc3339(),
                None,
                0,
            ));
            s.executions.push(ExecutionRecord::minimal(
                "helper-b".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Ready,
                now_rfc3339(),
                None,
                0,
            ));
        });

        record_helper_launch_failure(&locks, &root, "sess-1", "helper-a", "no workspace");

        let session = store::read_session(&root, "sess-1").unwrap();
        let a = session.executions.iter().find(|e| e.execution_id == "helper-a").unwrap();
        let b = session.executions.iter().find(|e| e.execution_id == "helper-b").unwrap();
        assert_eq!(a.state, SessionState::Failed);
        assert_eq!(a.output_summary.as_deref(), Some("no workspace"));
        assert_eq!(b.state, SessionState::Ready, "a sibling helper must be untouched");
    }

    #[test]
    fn maybe_finish_graph_marks_the_lead_finished_once_every_helper_is_terminal() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            s.executions.push(ExecutionRecord::minimal(
                "lead".into(),
                s.header.session_id.clone(),
                None,
                SessionState::Working,
                now_rfc3339(),
                None,
                0,
            ));
            s.executions.push(ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Finished,
                now_rfc3339(),
                None,
                0,
            ));
            s.executions.push(ExecutionRecord::minimal(
                "helper-b".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Failed,
                now_rfc3339(),
                None,
                0,
            ));
        });

        let session = store::read_session(&root, "sess-1").unwrap();
        maybe_finish_graph(&locks, &root, "sess-1", &session);

        let session = store::read_session(&root, "sess-1").unwrap();
        let lead = session.executions.iter().find(|e| e.execution_id == "lead").unwrap();
        assert_eq!(lead.state, SessionState::Finished);
        assert_eq!(session.header.state, SessionState::Finished);
    }

    #[test]
    fn maybe_finish_graph_does_nothing_while_a_helper_is_still_active() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            s.executions.push(ExecutionRecord::minimal(
                "lead".into(),
                s.header.session_id.clone(),
                None,
                SessionState::Working,
                now_rfc3339(),
                None,
                0,
            ));
            s.executions.push(ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Working,
                now_rfc3339(),
                None,
                0,
            ));
        });

        let session = store::read_session(&root, "sess-1").unwrap();
        maybe_finish_graph(&locks, &root, "sess-1", &session);

        let session = store::read_session(&root, "sess-1").unwrap();
        let lead = session.executions.iter().find(|e| e.execution_id == "lead").unwrap();
        assert_eq!(lead.state, SessionState::Working, "must not finish while helper-a is still Working");
    }

    // -- integrate_helper_result (R6.7/R6.8): both sides of a real conflict
    // are preserved through the git-backed diff path, not merely the pure
    // `graph::detect_conflict` function tested elsewhere. --

    #[test]
    fn integrate_helper_result_records_a_typed_conflict_when_the_lead_also_changed_the_file() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();

        let lead_dir = tempfile::tempdir().unwrap();
        let (lead_repo, base_oid) = init_repo_with_commit(lead_dir.path(), &[("shared.txt", "base")]);
        // The lead's own checkout already integrated a different change.
        std::fs::write(lead_dir.path().join("shared.txt"), "lead changed it").unwrap();
        commit_all(&lead_repo, "lead edit");

        let helper_dir = tempfile::tempdir().unwrap();
        let (helper_repo, _helper_base) = init_repo_with_commit(helper_dir.path(), &[("shared.txt", "base")]);
        std::fs::write(helper_dir.path().join("shared.txt"), "helper changed it").unwrap();
        commit_all(&helper_repo, "helper edit");

        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            s.header.repo_path = lead_dir.path().to_string_lossy().into_owned();
            let mut helper = ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Finished,
                now_rfc3339(),
                None,
                0,
            );
            helper.worktree_path = Some(helper_dir.path().to_string_lossy().into_owned());
            helper.base_oid = Some(base_oid.clone());
            s.executions.push(helper);
        });

        let session = store::read_session(&root, "sess-1").unwrap();
        integrate_helper_result(&locks, &root, "sess-1", "helper-a", &session);

        let session = store::read_session(&root, "sess-1").unwrap();
        let helper = session.executions.iter().find(|e| e.execution_id == "helper-a").unwrap();
        assert_eq!(helper.state, SessionState::NeedsInput, "a real conflict must pause this node, not silently pick a side");
        let conflict = helper.conflict.as_ref().expect("conflict must be recorded");
        assert_eq!(conflict.helper_text, "helper changed it");
        assert_eq!(conflict.integrated_text, "lead changed it");
        assert_eq!(conflict.base_text, "base");
    }

    #[test]
    fn integrate_helper_result_integrates_cleanly_when_nothing_else_touched_the_file() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();

        let lead_dir = tempfile::tempdir().unwrap();
        let (_lead_repo, base_oid) = init_repo_with_commit(lead_dir.path(), &[("only_helper.txt", "base")]);
        // Lead's checkout never touched this file after the base.

        let helper_dir = tempfile::tempdir().unwrap();
        let (helper_repo, _helper_base) = init_repo_with_commit(helper_dir.path(), &[("only_helper.txt", "base")]);
        std::fs::write(helper_dir.path().join("only_helper.txt"), "helper wrote this").unwrap();
        commit_all(&helper_repo, "helper edit");

        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            s.header.repo_path = lead_dir.path().to_string_lossy().into_owned();
            let mut helper = ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Finished,
                now_rfc3339(),
                None,
                0,
            );
            helper.worktree_path = Some(helper_dir.path().to_string_lossy().into_owned());
            helper.base_oid = Some(base_oid.clone());
            s.executions.push(helper);
        });

        let session = store::read_session(&root, "sess-1").unwrap();
        integrate_helper_result(&locks, &root, "sess-1", "helper-a", &session);

        let session = store::read_session(&root, "sess-1").unwrap();
        let helper = session.executions.iter().find(|e| e.execution_id == "helper-a").unwrap();
        assert_eq!(helper.state, SessionState::Finished, "no conflict means the node stays Finished");
        assert!(helper.conflict.is_none());
    }
}
