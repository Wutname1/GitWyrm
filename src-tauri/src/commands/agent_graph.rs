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
use tauri::AppHandle;

use crate::agentdesk::graph::{
    self, GraphNodeView, GraphValidationError, IntegrationConflict, ProposedGraph,
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
    manager: tauri::State<'_, RepoManager>,
    session_id: SessionId,
) -> Result<StartGraphOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    let manager_owned = manager.inner();
    Ok(start_graph_at(&locks_arc, &root, manager_owned, &session_id))
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
    }
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
            let (execution_id, state, worktree_path, branch) = match is_ready_now {
                Some(p) => (
                    p.execution_id.clone(),
                    SessionState::Ready,
                    Some(p.path.clone()),
                    Some(p.branch.clone()),
                ),
                None => (
                    crate::agentdesk::execution_id_for_run_session(&new_id()),
                    SessionState::Draft,
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
    use crate::agentdesk::graph::{
        CompletionCondition, HelperRole, JobBudget, ProposedGraph, ProposedHelperJob,
    };
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
        }];
        let provisioned_b = vec![ProvisionedHelper {
            node_id: "research".into(),
            execution_id: "helper-from-b".into(),
            branch: "agent-desk/lead/research-b".into(),
            path: "C:/fake/worktree-b".into(),
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
}
