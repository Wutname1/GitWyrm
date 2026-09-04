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
    self, ApplyOperationOutcome, FileContent, FileExecutable, FileOperation, GraphNodeView,
    GraphValidationError, HelperRole, IntegrationConflict, ProposedGraph, ProposedHelperJob,
};
use crate::agentdesk::model::{
    AgentSession, ExecutionId, ExecutionRecord, SessionId, SessionLoadError, SessionState,
};
use crate::agentdesk::store::{self, SessionStoreRoot};
use crate::commands::agent_desk::{openspec_target_of, resolve_openspec_context};
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

/// Serializes every step that touches a session's dedicated integration
/// worktree -- provisioning it, folding one helper's delta into it, and
/// launching the lead review turn against it -- behind ONE lock per session
/// (P1 "serialize helper integration").
///
/// Two helpers can legitimately finish within milliseconds of each other
/// (`advance_graph_after_helper_completion` runs from each helper's own
/// completion callback, independently, on whichever thread that helper's
/// `run_task` happened to finish on). Before this lock, both completions
/// could call `integrate_helper_result` concurrently against the SAME
/// on-disk worktree -- reading its current file contents, computing a delta,
/// and writing back -- with no ordering guarantee between them, which is
/// exactly the kind of interleaved read-modify-write `agentdesk::SessionLocks`
/// exists to prevent for the session file itself. Reuses that same lock
/// registry under a distinct key namespace (`"integration:{session_id}"`)
/// rather than a second lock type: the guarantee needed here -- one holder
/// at a time per session, `SessionLock`'s own wait/hold logging for a stall
/// -- is identical, and a session's ordinary read-modify-write lock (keyed by
/// the bare `session_id`) is deliberately a DIFFERENT key so a slow
/// integration pass (a large delta, a slow filesystem) never blocks an
/// unrelated read/write of the session file itself (e.g. the UI polling
/// `agent_session_get` while a helper's integration is still running).
fn integration_lock_key(session_id: &str) -> String {
    format!("integration:{session_id}")
}

/// Runs `f` while holding this session's integration lock -- see
/// [`integration_lock_key`]'s doc comment for what this serializes and why
/// it is a distinct key from the session's own read-modify-write lock.
#[track_caller]
fn with_integration_lock<T>(locks: &crate::agentdesk::SessionLocks, session_id: &str, f: impl FnOnce() -> T) -> T {
    locks.with_session_lock(&integration_lock_key(session_id), f)
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

pub(crate) fn propose_graph_at(
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
// R6.1: turn a live Plan-mode turn's finished reply into a persisted
// proposal, or a visible refusal -- the production entry point is
// `commands::airun::route_to_agent_desk`'s completion hook, which calls this
// once the lead's execution reaches `Finished`. See
// `agentdesk::plan_proposal`'s module doc for why a fenced block (not a new
// ACP tool) is the mechanism this reads back out of the transcript.
// ---------------------------------------------------------------------------

/// What happened when a just-finished Plan-mode lead turn was checked for a
/// graph proposal. Internal (not `Type`/IPC) -- the caller
/// (`finish_plan_mode_execution_at`, called only from the durable-event
/// completion path) reports its own outcome via the ordinary transcript
/// `Note` mechanism, never back across the IPC boundary; there is no command
/// that returns this directly.
#[derive(Debug)]
enum PlanProposalCompletion {
    /// Not a Plan-mode lead turn at all (Ask/Auto, a helper, a Plan-mode
    /// Solo run) -- nothing to check.
    NotApplicable,
    /// A proposal was found, validated, and persisted as a fresh
    /// `AwaitingStart` execution (`propose_graph_at`).
    Proposed {
        execution_id: ExecutionId,
        auto_start: bool,
    },
    /// The lead's reply did not contain a usable proposal -- `outcome`
    /// carries exactly why (`agentdesk::plan_proposal::ProposalOutcome`,
    /// minus the `Found` case, which would have taken the `Proposed` branch
    /// above instead).
    Refused { outcome: crate::agentdesk::plan_proposal::ProposalOutcome },
    /// The proposal was found and valid, but persisting it failed for an
    /// ordinary session-store reason (session gone, damaged, write failed).
    PersistFailed { outcome: ProposeGraphOutcome },
}

/// Reads `finished_execution_id`'s own accumulated transcript text and, if
/// this was a Plan-mode lead turn, checks it for a graph proposal.
///
/// Deliberately reads the session itself (rather than accepting the text as
/// a parameter) so the Plan-mode-lead check (`mode`/`team`/
/// `parent_execution_id`) and the text-gathering both read the SAME
/// snapshot -- an execution that raced from Plan into something else between
/// two separate reads could otherwise be checked against stale mode/team
/// while the text came from after the race.
fn finish_plan_mode_execution_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    finished_execution_id: &str,
) -> PlanProposalCompletion {
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(_) => return PlanProposalCompletion::NotApplicable,
    };
    let Some(record) = session
        .executions
        .iter()
        .find(|e| e.execution_id == finished_execution_id)
    else {
        return PlanProposalCompletion::NotApplicable;
    };
    // Only a LEAD's own Plan-mode turn ever proposes a graph -- a helper
    // never does (`parent_execution_id.is_some()`), and only `mode ==
    // "plan"` with `team == "lead"` was ever told to (see
    // `commands::agent_desk::start_execution_at`'s prompt augmentation,
    // gated on the exact same pair).
    if record.parent_execution_id.is_some() {
        return PlanProposalCompletion::NotApplicable;
    }
    let mode = record.mode.as_deref();
    if !matches!(mode, Some("Plan" | "Auto")) || record.team.as_deref() != Some("Lead") {
        return PlanProposalCompletion::NotApplicable;
    }

    let text: String = session
        .messages
        .iter()
        .filter(|m| m.execution_id.as_deref() == Some(finished_execution_id))
        .map(|m| m.plain_content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    match crate::agentdesk::plan_proposal::extract_graph_proposal(&text) {
        crate::agentdesk::plan_proposal::ProposalOutcome::Found { graph } => {
            match propose_graph_at(locks, root, session_id, graph) {
                ProposeGraphOutcome::AwaitingStart { execution_id, .. } => {
                    PlanProposalCompletion::Proposed {
                        execution_id,
                        auto_start: mode == Some("Auto"),
                    }
                }
                other => PlanProposalCompletion::PersistFailed { outcome: other },
            }
        }
        other => PlanProposalCompletion::Refused { outcome: other },
    }
}

/// Plain-language explanation for every non-`Found` `ProposalOutcome`, shown
/// as a system note in the transcript -- the reset doc's "fail visibly when
/// the model returns something unparseable, never silently fall back to
/// solo without telling the user."
fn plain_proposal_refusal(outcome: &crate::agentdesk::plan_proposal::ProposalOutcome) -> Option<String> {
    use crate::agentdesk::plan_proposal::ProposalOutcome;
    match outcome {
        // Not a refusal to report: the model was simply not asked to
        // propose a graph, or (for `NoProposalFound`) chose not to per the
        // prompt's own "say so in plain language" instruction, which it
        // already did in its own reply -- adding a second note would be
        // redundant, not clarifying.
        ProposalOutcome::Found { .. } => None,
        ProposalOutcome::NoProposalFound => None,
        ProposalOutcome::MultipleProposalsFound { count } => Some(format!(
            "This plan included {count} proposed graphs instead of one, so none of them could be started. Try asking again for a single plan."
        )),
        ProposalOutcome::MalformedJson { .. } => Some(
            "This plan's proposal was not written in a format GitWyrm could read, so no graph was started. Try asking again, or start this task yourself.".to_string(),
        ),
        ProposalOutcome::SchemaMismatch { .. } => Some(
            "This plan's proposal was missing something GitWyrm needs, so no graph was started. Try asking again, or start this task yourself.".to_string(),
        ),
        ProposalOutcome::Invalid { reason } => Some(format!(
            "This plan's proposed graph could not be started: {}",
            plain_validation_reason(reason)
        )),
    }
}

fn plain_validation_reason(reason: &GraphValidationError) -> String {
    match reason {
        GraphValidationError::TooManyHelpers { found, max } => {
            format!("it proposed {found} helpers, more than the {max} allowed at once.")
        }
        GraphValidationError::DuplicateNodeId { node_id } => {
            format!("two helpers were both named \"{node_id}\".")
        }
        GraphValidationError::EmptyJob { node_id, detail } => {
            format!("the helper \"{node_id}\" was missing something: {detail}.")
        }
        GraphValidationError::MissingAllowedPaths { node_id } => {
            format!("the helper \"{node_id}\" can write but was not given any files it may change.")
        }
        GraphValidationError::UnknownDependency { node_id, missing } => {
            format!("the helper \"{node_id}\" depends on \"{missing}\", which does not exist in the plan.")
        }
        GraphValidationError::Cycle { node_ids } => {
            format!("helpers {} depend on each other in a loop.", node_ids.join(", "))
        }
    }
}

/// Appends a plain system note to the session's transcript, outside the
/// ordinary run-event/bridge path -- used only for the refusal message
/// above, which has no `RunEventKind` of its own to ride in on (the lead's
/// own turn already ended with its real `Ended` event by the time this
/// runs). Mirrors `commands::agent_desk::append_user_message_at`'s
/// segment-reuse shape, but writes a `System`-role/`System`-kind message
/// with no `execution_id` -- it did not come from any execution's event
/// stream.
pub(crate) fn append_system_note(locks: &crate::agentdesk::SessionLocks, root: &SessionStoreRoot, session_id: &str, text: &str) {
    let _ = update_session_at(locks, root, session_id, |s| {
        let now = now_rfc3339();
        let segment_id = match s.segments.last() {
            Some(seg) => seg.segment_id.clone(),
            None => {
                let id = new_id();
                s.segments.push(crate::agentdesk::model::ConversationSegment {
                    segment_id: id.clone(),
                    label: "Conversation".into(),
                    started_at: now.clone(),
                });
                id
            }
        };
        s.messages.push(crate::agentdesk::model::SessionMessage {
            message_id: new_id(),
            segment_id,
            role: crate::agentdesk::model::MessageRole::System,
            timestamp: now,
            plain_content: text.to_string(),
            rendered_content: None,
            provider: None,
            model: None,
            kind: crate::agentdesk::model::MessageKind::System,
            execution_id: None,
            sequence: None,
            import: None,
            targets: Vec::new(),
        });
    });
}

/// Production entry point called from `commands::airun::route_to_agent_desk`
/// once a durable execution reaches `Finished`: checks whether it was a
/// Plan-mode lead turn and, if so, either persists its proposal or appends a
/// visible refusal note explaining why none was started.
pub(crate) fn finish_lead_graph_proposal(
    app: &AppHandle,
    locks: &std::sync::Arc<crate::agentdesk::SessionLocks>,
    root: &SessionStoreRoot,
    session_id: &str,
    finished_execution_id: &str,
) {
    match finish_plan_mode_execution_at(locks, root, session_id, finished_execution_id) {
        PlanProposalCompletion::NotApplicable => {}
        PlanProposalCompletion::Proposed { auto_start: false, .. } => {}
        PlanProposalCompletion::Proposed { auto_start: true, .. } => {
            let links = app.state::<crate::agentdesk::RunSessionLinks>();
            let executions = app.state::<crate::agentdesk::ExecutionRegistry>();
            let manager = app.state::<RepoManager>();
            let outcome = start_graph_and_launch(
                app,
                locks,
                root,
                links.inner(),
                executions.inner(),
                manager.inner(),
                session_id,
            );
            if !matches!(outcome, StartGraphOutcome::Started { .. }) {
                append_system_note(
                    locks,
                    root,
                    session_id,
                    "The helper plan was ready, but GitWyrm could not start it. Open the helper panel to review the plan and try again.",
                );
            }
        }
        PlanProposalCompletion::Refused { outcome } => {
            if let Some(text) = plain_proposal_refusal(&outcome) {
                append_system_note(locks, root, session_id, &text);
            }
        }
        PlanProposalCompletion::PersistFailed { outcome } => {
            let text = match outcome {
                ProposeGraphOutcome::Invalid { reason } => format!(
                    "This plan's proposed graph could not be started: {}",
                    plain_validation_reason(&reason)
                ),
                _ => "This plan proposed a graph, but it could not be saved. Try asking again.".to_string(),
            };
            append_system_note(locks, root, session_id, &text);
        }
    }
}

/// Store-only completion hook retained for focused tests and callers that do
/// not own an app handle. Auto launch is deliberately performed only by
/// `finish_lead_graph_proposal`, the production event path above.
#[cfg(test)]
fn finish_plan_mode_proposal(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    finished_execution_id: &str,
) {
    match finish_plan_mode_execution_at(locks, root, session_id, finished_execution_id) {
        PlanProposalCompletion::NotApplicable | PlanProposalCompletion::Proposed { .. } => {}
        PlanProposalCompletion::Refused { outcome } => {
            if let Some(text) = plain_proposal_refusal(&outcome) {
                append_system_note(locks, root, session_id, &text);
            }
        }
        PlanProposalCompletion::PersistFailed { outcome } => {
            let text = match outcome {
                ProposeGraphOutcome::Invalid { reason } => format!(
                    "This plan's proposed graph could not be started: {}",
                    plain_validation_reason(&reason)
                ),
                _ => "This plan proposed a graph, but it could not be saved. Try asking again.".to_string(),
            };
            append_system_note(locks, root, session_id, &text);
        }
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
    /// The proposal named no helpers, which is the lead saying it will do
    /// the work alone. The proposal is cleared and the session handed back
    /// as an ordinary solo chat, ready for its next message. Distinct from
    /// `Started`: nothing was launched, and from `Invalid`: nothing was
    /// wrong.
    NoHelpersRunSolo { session: AgentSession },
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
    /// Task 3.3 ("detect task/spec changes after draft and block Start until
    /// refreshed or explicitly accepted"): this lead's proposal was drafted
    /// from an OpenSpec context (`ExecutionRecord::context_fingerprint`) that
    /// no longer matches the change's current files. Start refuses outright
    /// -- no worktree is provisioned, nothing is written -- until the user
    /// either accepts the drift via `agent_session_accept_stale_openspec_context`
    /// or asks the lead to re-plan. `current_fingerprint` lets the caller
    /// persist acceptance against the exact drift being accepted, so a
    /// second, later change cannot ride through on an old acceptance.
    Stale {
        lead_execution_id: ExecutionId,
        current_fingerprint: String,
    },
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
    let outcome = start_graph_and_launch(
        &app,
        &locks_arc,
        &root,
        links.inner(),
        executions.inner(),
        manager_owned,
        &session_id,
    );

    Ok(outcome)
}

/// Consumes a persisted graph and launches every dependency-ready helper.
/// Both the visible Plan-mode Start button and Auto mode use this one path,
/// so automatic orchestration cannot drift from the reviewed/manual flow.
fn start_graph_and_launch(
    app: &AppHandle,
    locks: &std::sync::Arc<crate::agentdesk::SessionLocks>,
    root: &SessionStoreRoot,
    links: &crate::agentdesk::RunSessionLinks,
    executions: &crate::agentdesk::ExecutionRegistry,
    manager: &RepoManager,
    session_id: &str,
) -> StartGraphOutcome {
    let outcome = start_graph_at(locks, root, manager, session_id);
    if let StartGraphOutcome::Started { ref session, ref started_helpers, .. } = outcome {
        for helper_execution_id in started_helpers {
            launch_helper(
                app,
                locks,
                root,
                links,
                executions,
                session,
                helper_execution_id,
            );
        }
    }
    outcome
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

    // A proposal with no helpers is the lead saying it will do this alone
    // (the parser accepts that shape on purpose). Starting it as a team put
    // the graph in Working with no helper that could ever become terminal,
    // so the run never completed and Auto mode reached this by itself.
    // Answered the same way the Use solo button answers it: drop the
    // proposal and hand the session back as an ordinary solo chat.
    if proposed.helpers.is_empty() {
        return match update_session_at(locks, root, session_id, |s| {
            s.executions.retain(|e| e.proposed_graph.is_none());
            s.header.active_execution_id = None;
            s.header.state = SessionState::Ready;
        }) {
            UpdateOutcome::Updated { session } => StartGraphOutcome::NoHelpersRunSolo { session },
            UpdateOutcome::NotFound => StartGraphOutcome::NotFound,
            UpdateOutcome::Damaged { reason } => StartGraphOutcome::Damaged { reason },
            UpdateOutcome::WriteFailed { detail } => StartGraphOutcome::WriteFailed { detail },
            UpdateOutcome::Unavailable { detail } => StartGraphOutcome::Unavailable { detail },
        };
    }

    // Task 3.3 ("detect task/spec changes after draft and block Start until
    // refreshed/accepted"): a Plan-mode lead drafted this graph from an
    // OpenSpec context whose fingerprint is stamped on the lead's own
    // execution record (`start_execution_at`, R5.3). If the session's source
    // is an OpenSpec change/task, recompute that fingerprint from the LIVE
    // files right now and compare -- if it moved, and the user has not
    // already accepted this exact drift via
    // `agent_session_accept_stale_openspec_context`, refuse before a single
    // worktree is provisioned or a single helper is minted. A session with no
    // OpenSpec source (`openspec_target_of` returns `None`) or a lead that
    // never had a fingerprint (drafted before this ran, or not OpenSpec at
    // all) has nothing to compare and proceeds exactly as before.
    if let Some(launched_fingerprint) = &lead.context_fingerprint {
        if let Some(target) = openspec_target_of(&session.header.source) {
            let repo_path = std::path::Path::new(&session.header.repo_path);
            if let Some(ctx) = resolve_openspec_context(repo_path, &target) {
                let current_fingerprint = crate::agentdesk::openspec_context::fingerprint(&ctx);
                let drifted = crate::agentdesk::openspec_context::context_changed_since(
                    &current_fingerprint,
                    launched_fingerprint,
                );
                if drifted {
                    let accepted = lead
                        .accepted_stale_context_fingerprint
                        .as_deref()
                        == Some(current_fingerprint.as_str());
                    if !accepted {
                        return StartGraphOutcome::Stale {
                            lead_execution_id,
                            current_fingerprint,
                        };
                    }
                }
            }
        }
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

// ---------------------------------------------------------------------------
// Task 3.3: accept a stale OpenSpec context (the other half of
// `StartGraphOutcome::Stale` -- refresh is just calling
// `agent_session_start_graph` again after the source stopped drifting;
// this is the explicit "start anyway" path).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AcceptStaleOpenSpecContextOutcome {
    /// The lead's record now carries `current_fingerprint` as accepted --
    /// calling `agent_session_start_graph` again will not refuse for THIS
    /// drift again (though a further change after acceptance still will,
    /// since `start_graph_at` compares against the freshly recomputed
    /// fingerprint every time, not against `launched_fingerprint`).
    Accepted { session: AgentSession },
    /// No `NeedsInput` proposal with a fingerprint was found to accept
    /// against -- nothing to do (Start was never blocked, or already
    /// resolved by a concurrent call).
    NoProposal,
    NotFound,
    Damaged { reason: String },
    Unavailable { detail: String },
    WriteFailed { detail: String },
}

fn accept_stale_openspec_context_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
) -> AcceptStaleOpenSpecContextOutcome {
    let mut found = false;
    let outcome = update_session_at(locks, root, session_id, |session| {
        // Recompute the current fingerprint from inside the lock, from the
        // session's own recorded source/repo path -- never trust a
        // caller-supplied fingerprint string, which could be stale by the
        // time this write lands (the same "never trust the client's copy"
        // reasoning every other mutating command here follows).
        let repo_path = std::path::PathBuf::from(&session.header.repo_path);
        let Some(target) = openspec_target_of(&session.header.source) else {
            return;
        };
        let Some(ctx) = resolve_openspec_context(&repo_path, &target) else {
            return;
        };
        let current_fingerprint = crate::agentdesk::openspec_context::fingerprint(&ctx);
        if let Some(lead) = session
            .executions
            .iter_mut()
            .find(|e| e.parent_execution_id.is_none() && e.proposed_graph.is_some())
        {
            lead.accepted_stale_context_fingerprint = Some(current_fingerprint);
            found = true;
        }
    });
    match outcome {
        UpdateOutcome::Updated { session } if found => {
            AcceptStaleOpenSpecContextOutcome::Accepted { session }
        }
        UpdateOutcome::Updated { .. } => AcceptStaleOpenSpecContextOutcome::NoProposal,
        UpdateOutcome::NotFound => AcceptStaleOpenSpecContextOutcome::NotFound,
        UpdateOutcome::Damaged { reason } => AcceptStaleOpenSpecContextOutcome::Damaged { reason },
        UpdateOutcome::WriteFailed { detail } => {
            AcceptStaleOpenSpecContextOutcome::WriteFailed { detail }
        }
        UpdateOutcome::Unavailable { detail } => {
            AcceptStaleOpenSpecContextOutcome::Unavailable { detail }
        }
    }
}

/// Production entry point for the "Start anyway" choice on a
/// `StartGraphOutcome::Stale` refusal. The frontend calls this, then retries
/// `agent_session_start_graph` -- kept as two separate calls (rather than one
/// combined "accept and start") so the retry goes through the exact same
/// Start path and re-validation everything else in this file already trusts,
/// instead of a parallel "start after accepting" branch that could drift from
/// it.
#[tauri::command]
#[specta::specta]
pub async fn agent_session_accept_stale_openspec_context(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<crate::agentdesk::SessionLocks>>,
    session_id: SessionId,
) -> Result<AcceptStaleOpenSpecContextOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    Ok(accept_stale_openspec_context_at(&locks_arc, &root, &session_id))
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
            record.completion = Some(job.completion.clone());
            record.worktree_path = worktree_path;
            record.branch = branch;
            // R6.7: the merge base `integrate_helper_result` diffs this
            // helper's worktree against once it finishes.
            record.base_oid = base_oid;
            // R6.4: carried from the proposal so `launch_helper` can hand it
            // to `cli_run::run_task` for actual enforcement -- see
            // `ExecutionRecord::budget`'s own doc comment.
            record.budget = Some(job.budget);
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
        // P1-B: the durable authority transition. `start_execution_at`
        // (`commands::agent_desk`) reads this on every future call for this
        // session to decide `cli_run::run_task`'s `started` flag -- once set
        // here, every subsequent turn (including a helper's own turns,
        // which are policy-gated separately by their role/allowed_paths,
        // not by this flag) is free to write, and this session's very next
        // read of its own header proves that decision durably rather than
        // through any in-memory or graph-shape signal that would not
        // survive a restart. Never cleared once set -- Start is a one-way
        // transition for the lifetime of this session.
        s.header.graph_started_at = Some(now_rfc3339());
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
    // R6.4: the actual enforcement handoff -- `budget` was copied onto this
    // record from the proposal at graph-start time
    // (`commit_started_graph_if_still_proposed`); `run_task` is what checks
    // it turn-by-turn and against its own wall clock.
    let budget = helper.budget;
    let allowed_paths = helper.allowed_paths.clone();
    // Builder always writes; a Researcher/Verifier writes only if it was
    // explicitly given an allowance -- the same rule `graph::validate_graph`
    // already enforces at proposal time (`MissingAllowedPaths`), so a
    // helper that reached `Ready` at all is guaranteed to satisfy it.
    let can_write = helper.helper_role.as_deref() == Some("builder") || !allowed_paths.is_empty();
    let policy = crate::agentdesk::policy::ExecutionPolicy::resolve_for_helper(can_write, allowed_paths);

    // Link THIS HELPER's own execution ID to the session -- never the
    // repository (`RunSessionLinks` is execution-addressed, see its own doc
    // comment: the P0 fix for "an event can reach the wrong chat"). The
    // lead's own link (from `start_execution_at`) is a separate mapping under
    // its own execution ID and is never touched here, so a lead and any
    // number of concurrent helpers -- even across two different sessions that
    // happen to share this repository -- each keep their own entry.
    links.link(&helper_execution_id, &session_id);

    // `discover_for` so a read-only helper (Researcher, Verifier) cannot be
    // handed a tool that has no way to refuse a write -- the helper's own
    // policy already knows whether it may change anything.
    let agent = match crate::ai::agent::cli_agent::CliAgent::discover_for(
        &policy,
        true,
        std::path::PathBuf::from(&worktree_path),
    ) {
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
            budget,
            // A helper's work is checked over in its own worktree before
            // it is reported finished, so a hollow helper is caught before
            // its changes are folded into the combined result.
            Some(std::path::PathBuf::from(&worktree_path)),
            String::new(),
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

    // A helper is only done when it did what it was asked to do. Its
    // `CompletionCondition` (make this check pass, change these files) was
    // recorded on the proposal and then never evaluated, so a helper told to
    // make the tests pass could stop early and still count as finished --
    // and its work would be folded into the integration worktree on that
    // basis. Judged BEFORE integration, so unmet work is not merged.
    let session = match enforce_completion_condition(locks, root, session_id, finished_execution_id, session) {
        Some(updated) => updated,
        // Marked Failed instead: nothing to integrate, and the scheduler
        // treats it like any other helper that ended badly.
        None => match store::read_session(root, session_id) {
            Ok(s) => {
                maybe_start_review_or_finish_graph(app, locks, root, session_id, &s);
                return;
            }
            Err(_) => return,
        },
    };

    // P1 "serialize helper integration": one integration pass for this
    // session at a time, so two helpers finishing back-to-back cannot
    // interleave their reads/writes of the same integration worktree. See
    // `with_integration_lock`'s doc comment.
    with_integration_lock(locks, session_id, || {
        integrate_helper_result(app, locks, root, session_id, finished_execution_id, &session);
    });

    // Re-read after integration may have changed this node's state.
    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut decision = graph::schedule(&session.executions);
    // A helper waiting on something that ended badly can never start. Left
    // in Draft it is not terminal, so the graph never reaches its review and
    // the run hangs. Mark it here, once, with the reason in plain words.
    let session = if decision.abandoned.is_empty() {
        session
    } else {
        mark_abandoned_helpers(locks, root, session_id, &decision.abandoned);
        match store::read_session(root, session_id) {
            Ok(s) => {
                decision = graph::schedule(&s.executions);
                s
            }
            Err(_) => return,
        }
    };
    if decision.ready.is_empty() {
        maybe_start_review_or_finish_graph(app, locks, root, session_id, &session);
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

// ---------------------------------------------------------------------------
// P0-C: dedicated integration worktree + real helper delta
// ---------------------------------------------------------------------------

/// Ensures the LEAD execution has its own dedicated integration worktree,
/// creating one on first need and persisting its path on the lead's own
/// `ExecutionRecord` (`integration_worktree_path`). This is the fix for "the
/// lead integration path uses the session repository path, which is the
/// user's open checkout" (audit P0-C): every helper's result is folded into
/// THIS worktree, never into `session.header.repo_path`, so the user's own
/// open checkout is never touched until they explicitly land the result
/// themselves.
///
/// Branches from the repository's current HEAD (via `RepoManager`, the same
/// way `start_graph_at` resolves `main_workdir`) the first time this is
/// called for a session; every subsequent call for the same session reuses
/// the already-provisioned worktree, so two helpers finishing back-to-back
/// integrate into the same tree rather than each getting a fresh one.
fn ensure_integration_worktree(
    app: &AppHandle,
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    session: &AgentSession,
) -> Result<String, String> {
    let Some(lead) = session.executions.iter().find(|e| e.parent_execution_id.is_none()) else {
        return Err("this session has no lead execution".into());
    };
    if let Some(existing) = &lead.integration_worktree_path {
        if std::path::Path::new(existing).is_dir() {
            return Ok(existing.clone());
        }
        // The recorded worktree is gone from disk (manually deleted, or a
        // prior cleanup) -- fall through and provision a fresh one rather
        // than handing back a path nothing can read or write.
    }

    let manager = app
        .try_state::<RepoManager>()
        .ok_or_else(|| "the repository manager is not available".to_string())?;
    let open = manager
        .get(&session.header.repo_id)
        .map_err(|e| e.to_string())?;
    let main_workdir = {
        let repo = open.repo.lock().unwrap_or_else(|e| e.into_inner());
        worktree::main_workdir(&repo)
    };
    let Some(main_workdir) = main_workdir else {
        return Err("this project has no working folder".into());
    };
    let main_workdir_str = main_workdir.to_string_lossy().into_owned();

    let branch = format!("agent-desk/{}/integration", short(&lead.execution_id));
    let path = worktree::suggest_path(&main_workdir, &branch);
    worktree::add(&main_workdir_str, &path, &branch, true, Some("HEAD")).map_err(|e| e.to_string())?;
    if let Ok(marked_repo) = git2::Repository::open(&main_workdir_str) {
        let _ = worktree::mark_as_run_worktree(&marked_repo, &worktree_admin_name(&path));
    }

    let lead_execution_id = lead.execution_id.clone();
    let outcome = update_session_at(locks, root, session_id, |s| {
        if let Some(exec) = s.executions.iter_mut().find(|e| e.execution_id == lead_execution_id) {
            // Another concurrent completion may have already provisioned one
            // between this call's unlocked `add` above and this locked
            // write -- if so, keep that one rather than overwriting it with
            // ours (both are valid, empty-of-conflicting-work worktrees at
            // this point, but only one should be the durable record so a
            // later cleanup does not orphan the other).
            if exec.integration_worktree_path.is_none() {
                exec.integration_worktree_path = Some(path.clone());
            }
        }
    });
    match outcome {
        UpdateOutcome::Updated { session } => {
            let lead = session
                .executions
                .iter()
                .find(|e| e.execution_id == lead_execution_id)
                .and_then(|e| e.integration_worktree_path.clone());
            lead.ok_or_else(|| "could not persist the integration worktree".to_string())
        }
        UpdateOutcome::NotFound => Err("this session no longer exists".into()),
        UpdateOutcome::Damaged { reason } => Err(reason),
        UpdateOutcome::WriteFailed { detail } | UpdateOutcome::Unavailable { detail } => Err(detail),
    }
}

// P1 "do not destroy the evidence": earlier builds of this file removed the
// session's dedicated integration worktree the moment the graph reached
// `Finished` (a `cleanup_integration_worktree` call from what is now
// `finish_graph_with_combined_result`). That destroyed the very evidence the
// combined result needs to exist at all -- the worktree IS
// `ResultRecord::worktree_path` for the combined result once `Finished`
// actually means "reviewed and there is a result to look at" rather than
// "the node count reached zero". Graph finish no longer touches this
// worktree at all. Its cleanup now goes through the SAME path every other
// result's worktree cleanup already goes through --
// `commands::agent_result::agent_result_cleanup_worktree` /
// `cleanup_worktree_at` -- which only removes a worktree once its result has
// reached `Committed` or `Discarded` (task 5.1's "only after safe
// integration or confirmed discard"), using `DirtyChoice::Refuse` rather
// than the unconditional `Discard` this file used to reach for. No special
// case was needed: `cleanup_worktree_at` keys off `ResultRecord.worktree_path`
// alone, and the combined record's `worktree_path` is the integration
// worktree, so the existing Keep/Undo/Commit/cleanup flow (`commands::agent_result`)
// already covers it, unchanged.

/// Reads `path` out of `repo` at `oid` as a typed [`FileContent`] -- the
/// blob's raw bytes are never decoded as UTF-8 (P0-D: "cannot faithfully
/// represent ... binary"); a `Symlink` tree entry is read back as its link
/// target string, matching how git itself stores a symlink blob's content as
/// the target path rather than file bytes.
fn read_content_at(repo: &git2::Repository, oid: git2::Oid, executable: FileExecutable) -> Result<FileContent, git2::Error> {
    let blob = repo.find_blob(oid)?;
    let bytes = blob.content().to_vec();
    if blob.is_binary() {
        return Ok(FileContent::Binary { bytes, executable });
    }
    Ok(FileContent::Text { bytes, executable })
}

fn executable_of(mode: git2::FileMode) -> FileExecutable {
    match mode {
        git2::FileMode::BlobExecutable => FileExecutable::Yes,
        _ => FileExecutable::No,
    }
}

/// One [`FileOperation`] per delta from `diff`, resolved against `repo`'s
/// object database. A `Delta::Typechange` (e.g. a regular file replaced by a
/// symlink at the same path) is represented as a `Modify` carrying the new
/// side's content -- the type of thing at that path changed, but the path
/// itself is still a single modify from the integration target's point of
/// view. `Delta::Renamed`/`Copied` both carry the new side's full content
/// (never an empty placeholder), so a rename-with-edits round-trips exactly.
/// The content a diff entry points at, from the object database when it has
/// been written there, and from the WORKING DIRECTORY when it has not.
///
/// This second case is the whole shipped path: `diff_tree_to_workdir_with_index`
/// reports an unstaged edit with `id() == Oid::zero()`, because the new bytes
/// only exist on disk -- nothing has hashed them into the ODB yet. Helpers
/// never stage or commit, so EVERY change they make arrives this way. Reading
/// only blobs (and skipping zero OIDs) silently dropped all of it.
fn content_for_delta_side(
    repo: &git2::Repository,
    file: &git2::DiffFile,
) -> Option<FileContent> {
    let executable = executable_of(file.mode());
    if file.id() != git2::Oid::zero() {
        return if file.mode() == git2::FileMode::Link {
            read_symlink_target(repo, file.id())
        } else {
            read_content_at(repo, file.id(), executable).ok()
        };
    }

    // Not in the ODB yet: read it off disk, relative to the worktree root.
    let workdir = repo.workdir()?;
    let rel = file.path()?;
    let full = workdir.join(rel);
    let meta = std::fs::symlink_metadata(&full).ok()?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(&full).ok()?;
        return Some(FileContent::Symlink {
            target: target.to_string_lossy().to_string(),
        });
    }
    let bytes = std::fs::read(&full).ok()?;
    // Both variants carry raw bytes; the split is a classification, not a
    // conversion, so nothing is ever decoded and re-encoded. A NUL byte is
    // git's own binary heuristic and matches how `read_content_at` classifies
    // a committed blob.
    if bytes.contains(&0) {
        Some(FileContent::Binary { bytes, executable })
    } else {
        Some(FileContent::Text { bytes, executable })
    }
}

fn operations_from_diff(repo: &git2::Repository, diff: &git2::Diff) -> Vec<FileOperation> {
    let mut ops = Vec::new();
    for delta in diff.deltas() {
        let new_file = delta.new_file();
        let old_file = delta.old_file();
        let new_path = new_file.path().and_then(|p| p.to_str()).map(str::to_string);
        let old_path = old_file.path().and_then(|p| p.to_str()).map(str::to_string);

        match delta.status() {
            git2::Delta::Deleted => {
                if let Some(path) = old_path {
                    ops.push(FileOperation::Delete { path });
                }
            }
            git2::Delta::Added | git2::Delta::Untracked => {
                let Some(path) = new_path else { continue };
                if let Some(content) = content_for_delta_side(repo, &new_file) {
                    ops.push(FileOperation::Add { path, content });
                }
            }
            git2::Delta::Modified | git2::Delta::Typechange => {
                let Some(path) = new_path else { continue };
                if let Some(content) = content_for_delta_side(repo, &new_file) {
                    ops.push(FileOperation::Modify { path, content });
                }
            }
            git2::Delta::Renamed | git2::Delta::Copied => {
                let (Some(path), Some(from_path)) = (new_path, old_path) else { continue };
                if let Some(content) = content_for_delta_side(repo, &new_file) {
                    ops.push(FileOperation::Rename { from_path, path, content });
                }
            }
            // Unmodified/Ignored/Conflicted/Unreadable: nothing to integrate
            // from these -- a `Conflicted` index entry in particular is left
            // for the user to resolve in their own checkout, not silently
            // folded into the integration worktree.
            _ => {}
        }
    }
    ops
}

fn read_symlink_target(repo: &git2::Repository, oid: git2::Oid) -> Option<FileContent> {
    let blob = repo.find_blob(oid).ok()?;
    let target = String::from_utf8(blob.content().to_vec()).ok()?;
    Some(FileContent::Symlink { target })
}

/// Every [`FileOperation`] a helper's worktree represents relative to its own
/// recorded `base_oid`, covering staged, unstaged, AND committed work in one
/// pass (P0-C: "Helpers have no production commit tool ... a helper's real
/// edits are invisible" under the old committed-tree-only diff).
///
/// Uses `Repository::diff_tree_to_workdir_with_index`, which git2 documents
/// as comparing a tree against the union of the index and the working
/// directory -- exactly "staged, unstaged, and committed" in one delta set,
/// with no shell-out and no assumption that the helper ever ran `git commit`
/// (which, in the shipped path, it cannot: helpers have no commit tool).
/// Rename detection is enabled via `find_similar` so a moved-and-edited file
/// round-trips as one `Rename` operation rather than an unrelated
/// delete+add.
fn helper_delta(worktree_path: &str, base_oid: &str) -> Result<Vec<FileOperation>, git2::Error> {
    let repo = git2::Repository::open(worktree_path)?;
    let base = git2::Oid::from_str(base_oid)?;
    let base_tree = repo.find_commit(base)?.tree()?;
    // `include_untracked`/`recurse_untracked_dirs` are OFF by default in
    // git2 -- without them, a brand-new file the helper created and never
    // `git add`ed (an ordinary `Add` in the shipped path, since helpers
    // never stage or commit) would be invisible to this diff entirely,
    // silently dropping the helper's work rather than integrating it.
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    let mut diff = repo.diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))?;
    let mut find_opts = git2::DiffFindOptions::new();
    find_opts.renames(true);
    // A helper's renamed file is UNTRACKED on the new side (it never staged
    // anything), and by default `find_similar` only pairs tracked entries --
    // so the rename arrived as an unrelated delete+add. Both preserve the
    // bytes, but the relationship is worth keeping: it is the difference
    // between "moved this file" and "deleted one, wrote another".
    find_opts.for_untracked(true);
    find_opts.rewrites(true);
    // A `find_similar` failure (e.g. a very large diff) is not fatal --
    // falling back to unmatched delete+add deltas still preserves every
    // byte, it just loses the rename relationship, so this is a quality
    // trade-off, not a correctness one.
    let _ = diff.find_similar(Some(&mut find_opts));
    Ok(operations_from_diff(&repo, &diff))
}

/// The full [`FileContent`] of `path` as it stood in `worktree_path`'s tree
/// at `base_oid`, or `None` if the path did not exist there yet (a file the
/// helper created from scratch) -- `None` here is a legitimate "did not
/// exist" answer, not a read failure; `graph::detect_conflict`'s "helper
/// never touched this file" comparison is done on `FileOperation`s directly
/// by `integrate_helper_result` rather than needing this to fabricate an
/// empty base text.
fn read_content_at_revision(worktree_path: &str, base_oid: &str, path: &str) -> Option<FileContent> {
    let repo = git2::Repository::open(worktree_path).ok()?;
    let base = git2::Oid::from_str(base_oid).ok()?;
    let tree = repo.find_commit(base).ok()?.tree().ok()?;
    let entry = tree.get_path(std::path::Path::new(path)).ok()?;
    if entry.filemode() == i32::from(git2::FileMode::Link) {
        return read_symlink_target(&repo, entry.id());
    }
    read_content_at(&repo, entry.id(), executable_of_i32(entry.filemode())).ok()
}

fn executable_of_i32(mode: i32) -> FileExecutable {
    if mode == i32::from(git2::FileMode::BlobExecutable) {
        FileExecutable::Yes
    } else {
        FileExecutable::No
    }
}

/// Lossy UTF-8 text view of a [`FileContent`], for the existing text-level
/// `graph::detect_conflict` three-way check -- conflict detection stays
/// text-only exactly as it was before this change (task requirement:
/// "preserve their behavior for text files"); typed fidelity is about how a
/// NON-conflicting change is applied, not about widening what conflict
/// detection itself compares. A `Symlink`'s target string is compared as
/// text too, which is correct: two helpers repointing the same symlink
/// differently IS a reviewable conflict.
fn as_text(content: &FileContent) -> String {
    match content {
        FileContent::Text { bytes, .. } | FileContent::Binary { bytes, .. } => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        FileContent::Symlink { target } => target.clone(),
    }
}

/// Applies one [`FileOperation`] to `target_repo_path`, byte- and
/// mode-faithful, via the same temp-file-then-rename atomicity the old
/// `write_file_atomic` used -- widened to also cover delete, rename, and the
/// executable bit, none of which a text overwrite can express (P0-D).
///
/// A `Rename` first attempts to move the file at `from_path` (preserving
/// history-adjacent semantics on a plain filesystem move) and falls back to
/// writing `content` fresh at `path` if the source is missing (already
/// integrated by an earlier pass, or the source path was itself never
/// materialized) -- either way `path` ends up holding `content`'s exact
/// bytes, which is the only externally-observable guarantee this function
/// makes. Every branch either fully succeeds or leaves `target_repo_path`
/// exactly as it stood before the call for the path(s) involved -- never a
/// partial write (P0-D: "never report a partial operation as Finished").
fn apply_operation(target_repo_path: &str, operation: &FileOperation) -> ApplyOperationOutcome {
    match operation {
        FileOperation::Delete { path } => {
            let target = std::path::Path::new(target_repo_path).join(path);
            if !target.exists() {
                // Already gone -- a retry after a prior successful delete,
                // or a delete for a path the integration worktree never had
                // to begin with. Idempotent, not a failure.
                return ApplyOperationOutcome::Applied;
            }
            match std::fs::remove_file(&target) {
                Ok(()) => ApplyOperationOutcome::Applied,
                Err(e) => ApplyOperationOutcome::Failed {
                    path: path.clone(),
                    detail: format!("could not delete \"{path}\": {e}"),
                },
            }
        }
        FileOperation::Add { path, content } | FileOperation::Modify { path, content } => {
            write_content_atomic(target_repo_path, path, content)
        }
        FileOperation::Rename { from_path, path, content } => {
            let from_target = std::path::Path::new(target_repo_path).join(from_path);
            let to_target = std::path::Path::new(target_repo_path).join(path);
            if from_target.is_file() {
                if let Some(parent) = to_target.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return ApplyOperationOutcome::Failed {
                            path: path.clone(),
                            detail: format!("could not create the folder for \"{path}\": {e}"),
                        };
                    }
                }
                if std::fs::rename(&from_target, &to_target).is_ok() {
                    // The rename may have carried the OLD content/mode (a
                    // pure rename with no edits) or none at all if the
                    // source was a stale copy -- always follow up with an
                    // atomic content write so `path` ends up with exactly
                    // the recorded `content`, regardless of what the raw
                    // filesystem move happened to carry.
                    return write_content_atomic(target_repo_path, path, content);
                }
                // Fall through to a fresh write below -- the rename attempt
                // (e.g. a cross-device move, or the destination directory
                // problem above) failed, but the destination content is
                // still fully specified by `content`.
            }
            write_content_atomic(target_repo_path, path, content)
        }
    }
}

/// Writes `content`'s exact bytes (and, on a platform that supports it, its
/// executable bit) to `repo_path.join(relative_path)` via temp-file-then-
/// rename, so a failure partway through never leaves a truncated or
/// partially-written file where the real one used to be. A `Symlink` is
/// recreated as a real symlink pointing at `target` rather than a text file
/// containing the target string.
fn write_content_atomic(repo_path: &str, relative_path: &str, content: &FileContent) -> ApplyOperationOutcome {
    let target = std::path::Path::new(repo_path).join(relative_path);
    if let Some(parent) = target.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ApplyOperationOutcome::Failed {
                path: relative_path.to_string(),
                detail: format!("could not create the folder for \"{relative_path}\": {e}"),
            };
        }
    }

    if let FileContent::Symlink { target: link_target } = content {
        // Symlinks cannot go through a temp-file-then-rename in the same way
        // (there is no "write bytes then rename" for a link) -- instead,
        // build the new link at a temp path and rename that, so a failure
        // still never leaves a half-made link at the real path.
        let temp_name = format!(".gitwyrm-integrate-{}-{}.tmp", std::process::id(), relative_path.replace(['/', '\\'], "_"));
        let temp_path = target.with_file_name(temp_name);
        let _ = std::fs::remove_file(&temp_path);
        let create_result = create_symlink(link_target, &temp_path);
        return match create_result {
            Ok(()) => match std::fs::rename(&temp_path, &target) {
                Ok(()) => ApplyOperationOutcome::Applied,
                Err(e) => {
                    let _ = std::fs::remove_file(&temp_path);
                    ApplyOperationOutcome::Failed {
                        path: relative_path.to_string(),
                        detail: format!("could not save \"{relative_path}\": {e}"),
                    }
                }
            },
            Err(e) => ApplyOperationOutcome::Failed {
                path: relative_path.to_string(),
                detail: format!("could not create a link at \"{relative_path}\": {e}"),
            },
        };
    }

    let bytes: &[u8] = match content {
        FileContent::Text { bytes, .. } | FileContent::Binary { bytes, .. } => bytes,
        FileContent::Symlink { .. } => unreachable!("handled above"),
    };
    let executable = match content {
        FileContent::Text { executable, .. } | FileContent::Binary { executable, .. } => *executable,
        FileContent::Symlink { .. } => unreachable!("handled above"),
    };

    let temp_name = format!(
        ".gitwyrm-integrate-{}-{}.tmp",
        std::process::id(),
        target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    );
    let temp_path = target.with_file_name(temp_name);
    if let Err(e) = std::fs::write(&temp_path, bytes) {
        return ApplyOperationOutcome::Failed {
            path: relative_path.to_string(),
            detail: format!("could not write \"{relative_path}\": {e}"),
        };
    }
    if let Err(e) = set_executable(&temp_path, executable) {
        let _ = std::fs::remove_file(&temp_path);
        return ApplyOperationOutcome::Failed {
            path: relative_path.to_string(),
            detail: format!("could not set permissions for \"{relative_path}\": {e}"),
        };
    }
    if let Err(e) = std::fs::rename(&temp_path, &target) {
        let _ = std::fs::remove_file(&temp_path);
        return ApplyOperationOutcome::Failed {
            path: relative_path.to_string(),
            detail: format!("could not save \"{relative_path}\": {e}"),
        };
    }
    ApplyOperationOutcome::Applied
}

#[cfg(unix)]
fn create_symlink(target: &str, at: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(windows)]
fn create_symlink(target: &str, at: &std::path::Path) -> std::io::Result<()> {
    // Windows distinguishes file vs. directory symlinks and typically
    // requires developer mode or elevation to create either. Best-effort:
    // try a file link first (the common case for tracked repo content),
    // then a directory link, so this still works for the common shapes
    // without requiring the caller to know in advance which kind `target`
    // is.
    match std::os::windows::fs::symlink_file(target, at) {
        Ok(()) => Ok(()),
        Err(file_err) => match std::os::windows::fs::symlink_dir(target, at) {
            Ok(()) => Ok(()),
            Err(_) => Err(file_err),
        },
    }
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path, executable: FileExecutable) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    let mode = perms.mode();
    let new_mode = match executable {
        FileExecutable::Yes => mode | 0o111,
        FileExecutable::No => mode & !0o111,
    };
    perms.set_mode(new_mode);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path, _executable: FileExecutable) -> std::io::Result<()> {
    // Windows has no POSIX executable bit to set on an ordinary file --
    // nothing to do; the byte content (already written) is what round-trips
    // there.
    Ok(())
}

/// R6.7/R6.8/P0-C/P0-D: when a helper finishes cleanly, compute its REAL
/// worktree delta (staged, unstaged, and committed -- `helper_delta`)
/// relative to its own recorded base, and apply each resulting typed
/// [`FileOperation`] into the session's dedicated integration worktree
/// (`ensure_integration_worktree`) -- never into `session.header.repo_path`,
/// the user's own open checkout.
///
/// A file that only the helper touched is applied via [`apply_operation`],
/// byte- and mode-faithful (add, modify, delete, rename, binary, symlink,
/// executable bit all survive). A file changed by BOTH the helper and
/// something already integrated (another finished sibling, or the lead's own
/// edits since the base) is still reported as a typed [`IntegrationConflict`]
/// at the TEXT level, exactly as before: both texts preserved, neither
/// applied, until a person resolves it (tasks.md 5.3/5.4, `graph::detect_conflict`).
///
/// A helper that did not finish cleanly (`Stopped`/`Failed`/`Interrupted`)
/// has nothing to integrate -- its worktree stands as evidence of what it
/// was doing, but nothing from it is folded into the integration worktree.
///
/// Deliberately never runs `git commit` -- applying to the working tree is
/// this function's whole job; committing stays the user's own explicit
/// action through the ordinary review/commit flow.
fn integrate_helper_result(
    app: &AppHandle,
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

    // P0-C: the integration TARGET is this session's own dedicated
    // worktree, never `session.header.repo_path`. Provisioned lazily here so
    // a session whose graph never produces a clean helper result never pays
    // for one.
    let integration_path = match ensure_integration_worktree(app, locks, root, session_id, session) {
        Ok(p) => p,
        Err(detail) => {
            let _ = update_session_at(locks, root, session_id, |s| {
                if let Some(exec) = s.executions.iter_mut().find(|e| e.execution_id == helper_execution_id) {
                    exec.output_summary = Some(format!(
                        "Finished, but its results could not be integrated: {detail}. Try again once the graph completes."
                    ));
                }
            });
            return;
        }
    };

    integrate_helper_into(locks, root, session_id, helper_execution_id, session, &integration_path);
}

/// The actual P0-C/P0-D integration pass, taking the integration target's
/// path directly rather than resolving it itself -- separated out from
/// [`integrate_helper_result`] purely so tests can exercise the real delta
/// computation and typed apply against a plain `tempfile` directory, without
/// needing a Tauri `AppHandle`/`RepoManager` to resolve
/// `ensure_integration_worktree`.
fn integrate_helper_into(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    helper_execution_id: &str,
    session: &AgentSession,
    integration_path: &str,
) {
    let Some(helper) = session
        .executions
        .iter()
        .find(|e| e.execution_id == helper_execution_id)
    else {
        return;
    };
    let (Some(worktree_path), Some(base_oid)) = (&helper.worktree_path, &helper.base_oid) else {
        // No base revision recorded (a helper launched before this field was
        // wired, or one whose worktree failed to provision cleanly) -- there
        // is no merge base to diff against, so this helper's changes are
        // left for the lead/user to review manually via the worktree path
        // already shown in the inspector, rather than guessing at a base.
        return;
    };

    // P0-C: the REAL delta -- staged, unstaged, and committed work in the
    // helper's own worktree, not just what it happened to commit (which, in
    // the shipped path, is nothing -- helpers have no commit tool).
    let changed = match helper_delta(worktree_path, base_oid) {
        Ok(ops) => ops,
        Err(_) => return,
    };

    for operation in changed {
        let path = operation.path().to_string();
        let base_content = read_content_at_revision(worktree_path, base_oid, &path);
        let base_text = base_content.as_ref().map(as_text).unwrap_or_default();
        let helper_text = as_text_of_operation(&operation);
        // A file that is simply ABSENT from the integration worktree has not
        // been "changed to empty" -- nothing has touched it there yet, so its
        // state is still the base. Defaulting a missing file to "" made every
        // first integration into a fresh worktree look like a third divergent
        // version, and `detect_conflict` correctly (given bad input) called it
        // a conflict. Only a file that exists and cannot be read is unknown,
        // and that is treated as base too rather than inventing a difference.
        let integrated_path = std::path::Path::new(&integration_path).join(&path);
        let integrated_text = match std::fs::read_to_string(&integrated_path) {
            Ok(text) => text,
            Err(_) => base_text.clone(),
        };

        let sibling = session
            .executions
            .iter()
            .find(|e| e.parent_execution_id.is_some() && e.execution_id != helper_execution_id && e.state == SessionState::Finished);
        let conflicting_with = sibling
            .map(|s| s.execution_id.clone())
            .unwrap_or_else(|| "lead".to_string());

        match graph::detect_conflict(&path, &conflicting_with, &base_text, &helper_text, &integrated_text) {
            graph::IntegrationState::Conflicted { conflict } => {
                let _ = update_session_at(locks, root, session_id, |s| {
                    if let Some(exec) = s.executions.iter_mut().find(|e| e.execution_id == helper_execution_id) {
                        exec.conflict = Some(conflict.clone());
                        exec.state = SessionState::NeedsInput;
                    }
                });
                // Only the first conflicted file for this helper is recorded
                // per pass -- `agent_session_resolve_conflict` clears it and
                // a later re-run of this function (triggered by the next
                // graph event) will surface the next one, so nothing is
                // lost, only shown one at a time. Operations already applied
                // above this one in the loop stay applied; the conflicted
                // path itself is left exactly as `integrated_text` (nothing
                // written) until a person resolves it.
                return;
            }
            graph::IntegrationState::Integrated => {
                if helper_text == integrated_text && !matches!(operation, FileOperation::Delete { .. }) {
                    // Either the helper made no real change to this path, or
                    // its result is already sitting in the integration
                    // worktree (a re-run of this function after a partial
                    // earlier apply) -- nothing to write. A `Delete` still
                    // goes through `apply_operation` below even when both
                    // texts read as empty, since "the file is gone" is not
                    // observable by comparing empty strings alone.
                    continue;
                }
                match apply_operation(&integration_path, &operation) {
                    ApplyOperationOutcome::Applied => {}
                    ApplyOperationOutcome::Failed { path, detail } => {
                        // Leave this one operation for a later retry (the
                        // next completion event re-runs this same function)
                        // rather than silently dropping the helper's work or
                        // crashing the whole batch over one unwritable
                        // path. Reported in this node's own summary so the
                        // failure is visible, not smeared across other
                        // operations that may have already applied cleanly
                        // above. This node's state is left exactly as it
                        // was (`Finished`) -- a failed integration write is
                        // never reported as a completed graph, but it also
                        // does not fabricate a `NeedsInput`/conflict state
                        // that was never actually detected.
                        let _ = update_session_at(locks, root, session_id, |s| {
                            if let Some(exec) = s.executions.iter_mut().find(|e| e.execution_id == helper_execution_id) {
                                exec.output_summary = Some(format!(
                                    "Finished, but \"{path}\" could not be brought into your working files: {detail}"
                                ));
                            }
                        });
                    }
                }
            }
            graph::IntegrationState::Pending => {}
        }
    }
}

/// Lossy text view of what a [`FileOperation`] leaves at its path -- empty
/// for a `Delete` (matching the old contract: an absent file reads as empty
/// text for the three-way text comparison), the new content's text
/// otherwise.
fn as_text_of_operation(operation: &FileOperation) -> String {
    match operation {
        FileOperation::Delete { .. } => String::new(),
        FileOperation::Add { content, .. } | FileOperation::Modify { content, .. } | FileOperation::Rename { content, .. } => {
            as_text(content)
        }
    }
}

/// P1 "Finished is not a combined graph result": once every helper node has
/// reached a terminal state (`Finished`/`Stopped`/`Failed`/`Interrupted` --
/// nothing left `Ready`, `Draft`, `Preparing`, `Working`, or blocked-with-a-
/// conflict `NeedsInput`), this is the single entry point that turns "the
/// helpers are done" into "the graph is Finished" -- and it now does that in
/// three ordered steps, never by substituting a completed-node COUNT for any
/// of them:
///
/// 1. If no review turn has been launched yet for this lead
///    (`ExecutionRecord::review_execution_id.is_none()`), launch one --
///    [`launch_lead_review`] -- scoped to the session's own integration
///    worktree (never `session.header.repo_path`), and return without
///    touching `Finished` yet. The graph stays `Working` while this turn
///    runs, exactly like it stays `Working` while any helper runs.
/// 2. Once that review execution itself reaches a terminal state, build the
///    ONE combined [`crate::agentdesk::result::ResultRecord`] from the
///    integration worktree (`build_combined_result`), linking every helper's
///    own execution ID onto it, and only THEN mark the lead (and the session
///    header) `Finished`.
/// 3. The integration worktree is deliberately left standing here. It IS the
///    combined result's `worktree_path`, so destroying it at graph finish
///    would delete the evidence the user is about to review. Cleanup belongs
///    to the landing flow instead: `commands::agent_result::cleanup_worktree_at`
///    runs it once the result is Kept-and-committed or Discarded.
///
/// A thin `AppHandle`-aware wrapper around the state machine in
/// [`start_or_check_lead_review`], which holds the actual transition rules
/// and needs no `AppHandle` for the parts that do not launch an engine --
/// kept separate so those rules stay directly unit-testable without standing
/// up Tauri's test harness just to exercise them.
fn maybe_start_review_or_finish_graph(
    app: &AppHandle,
    locks: &std::sync::Arc<crate::agentdesk::SessionLocks>,
    root: &SessionStoreRoot,
    session_id: &str,
    session: &AgentSession,
) {
    match start_or_check_lead_review(locks, root, session_id, session) {
        LeadReviewStep::NotYetAllTerminal | LeadReviewStep::AlreadyFinished => {}
        LeadReviewStep::NeedsReviewTurn { integration_path } => {
            launch_lead_review(app, locks, root, session_id, &integration_path);
        }
        LeadReviewStep::ReviewStillRunning => {
            // Nothing to do -- `advance_lead_review_after_completion` (the
            // review turn's own completion callback, mirroring
            // `advance_graph_after_helper_completion`) is what re-enters this
            // function once the review execution itself finishes.
        }
        LeadReviewStep::ReviewFinishedBuildResultAndFinish { review_execution_id } => {
            finish_graph_with_combined_result(locks, root, session_id, session, &review_execution_id);
        }
    }
}

/// Judges a finished helper against its own completion condition.
///
/// Returns the session unchanged when the condition was met (or there was
/// none). When it was not, the helper is marked `Failed` with the reason in
/// plain words and `None` comes back, so the caller skips integration
/// entirely: work that did not meet its condition must not be merged on the
/// strength of the helper having stopped.
///
/// Only a `Finished` helper is judged. One that already failed or was
/// stopped has its own reason, and re-labelling it here would replace a true
/// account with a narrower one.
fn enforce_completion_condition(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    helper_execution_id: &str,
    session: AgentSession,
) -> Option<AgentSession> {
    use crate::agentdesk::completion::{judge, CompletionVerdict};

    let helper = session
        .executions
        .iter()
        .find(|e| e.execution_id == helper_execution_id)?;
    if helper.state != SessionState::Finished || helper.parent_execution_id.is_none() {
        return Some(session);
    }
    let condition = helper.completion.clone();
    if condition.is_none() {
        return Some(session);
    }

    let checks = crate::commands::agent_result::checks_for_execution(&session, helper_execution_id);
    let results = crate::agentdesk::result::read_results(root, session_id).unwrap_or_default();
    let result = results.iter().find(|r| r.execution_id == helper_execution_id);

    let CompletionVerdict::Unmet { reason } = judge(condition.as_ref(), &checks, result) else {
        return Some(session);
    };

    let title = helper
        .job_title
        .clone()
        .unwrap_or_else(|| "A helper".to_string());
    let _ = update_session_at(locks, root, session_id, |s| {
        if let Some(helper) = s.executions.iter_mut().find(|e| e.execution_id == helper_execution_id) {
            helper.state = SessionState::Failed;
            helper.ended_at = Some(now_rfc3339());
            helper.output_summary = Some(reason.clone());
        }
    });
    append_system_note(locks, root, session_id, &format!("\"{title}\" stopped without finishing its job. {reason}"));
    None
}

/// Marks every helper that can never start as `Failed`, and says why in the
/// transcript.
///
/// `Failed` rather than a state of its own: to everything downstream (the
/// review gate, the graph's own completion, the recovery sweep) this is a
/// helper that ended without doing its work, which is exactly what Failed
/// already means. A new state would have to be taught to each of them.
fn mark_abandoned_helpers(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    abandoned: &[crate::agentdesk::graph::AbandonedNode],
) {
    let mut notes: Vec<String> = Vec::new();
    let _ = update_session_at(locks, root, session_id, |s| {
        for node in abandoned {
            let blockers: Vec<String> = node
                .because_of
                .iter()
                .map(|dep| {
                    s.executions
                        .iter()
                        .find(|e| &e.execution_id == dep)
                        .and_then(|e| e.job_title.clone())
                        .unwrap_or_else(|| "an earlier step".to_string())
                })
                .collect();
            let Some(helper) = s.executions.iter_mut().find(|e| e.execution_id == node.execution_id) else {
                continue;
            };
            if !matches!(helper.state, SessionState::Draft | SessionState::Ready) {
                continue;
            }
            let title = helper.job_title.clone().unwrap_or_else(|| "A helper".to_string());
            helper.state = SessionState::Failed;
            helper.ended_at = Some(now_rfc3339());
            helper.output_summary = Some(format!(
                "Never started: it needed {} to finish first.",
                blockers.join(" and ")
            ));
            notes.push(format!(
                "\"{title}\" never started because {} did not finish.",
                blockers.join(" and ")
            ));
        }
    });
    for note in notes {
        append_system_note(locks, root, session_id, &note);
    }
}

/// What [`start_or_check_lead_review`] found, and what its `AppHandle`-aware
/// caller should do about it. A closed enum rather than an `Option` because
/// "needs a review turn launched" and "the review turn already finished, go
/// build the result" require entirely different follow-up actions -- folding
/// them into one `Option<T>` would leave the caller re-deriving which case it
/// is in anyway.
#[derive(Debug)]
enum LeadReviewStep {
    /// Not every helper is terminal yet -- nothing to do.
    NotYetAllTerminal,
    /// The lead is not `Working`, or has no helpers -- already finished, or
    /// this was never a graph session in the first place.
    AlreadyFinished,
    /// Every helper is terminal and no review turn has been launched yet.
    /// The caller should launch one against `integration_path`.
    NeedsReviewTurn { integration_path: String },
    /// A review turn is already recorded and still running (or the write
    /// that would have recorded a fresh one lost a race) -- nothing to do,
    /// the review turn's own completion path re-enters this function.
    ReviewStillRunning,
    /// The review turn reached a terminal state -- the caller should build
    /// the combined result and mark the graph `Finished`.
    ReviewFinishedBuildResultAndFinish { review_execution_id: ExecutionId },
}

/// The actual P1 state-transition rule, with no `AppHandle` dependency for
/// the two read-only branches (`NotYetAllTerminal`/`AlreadyFinished`) and a
/// SINGLE locked write for the one branch that mutates anything
/// (`NeedsReviewTurn`, which durably records `review_execution_id` before
/// the caller ever launches an engine against it -- a launch failure or a
/// crash between this write and the engine actually starting is recoverable
/// the same way `record_helper_launch_failure` recovers a helper launch
/// failure, rather than leaving `review_execution_id` unset and this
/// function trying to launch a second review turn on the next completion
/// event).
fn start_or_check_lead_review(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    session: &AgentSession,
) -> LeadReviewStep {
    let Some(lead) = session.executions.iter().find(|e| e.parent_execution_id.is_none()) else {
        return LeadReviewStep::AlreadyFinished;
    };
    if lead.state != SessionState::Working {
        return LeadReviewStep::AlreadyFinished;
    }
    // The review execution itself is recorded with `parent_execution_id:
    // Some(lead)` (see `start_or_check_lead_review`'s write below) so the
    // bridge treats its events as a helper's for `active_execution_id`/
    // `session.header.state` protection purposes -- but it is NOT one of
    // the helpers this function is waiting on, and must never count toward
    // `all_terminal`/`finished_count`/`linked_execution_ids` as if it were
    // one. Excluded here by ID rather than by any state-based guess.
    let helpers: Vec<&ExecutionRecord> = session
        .executions
        .iter()
        .filter(|e| e.parent_execution_id.is_some() && lead.review_execution_id.as_deref() != Some(e.execution_id.as_str()))
        .collect();
    if helpers.is_empty() {
        return LeadReviewStep::AlreadyFinished;
    }
    let all_terminal = helpers.iter().all(|h| {
        matches!(
            h.state,
            SessionState::Finished | SessionState::Stopped | SessionState::Failed | SessionState::Interrupted
        )
    });
    if !all_terminal {
        return LeadReviewStep::NotYetAllTerminal;
    }

    // A review turn is already recorded: either it is still running (its
    // own completion path will re-enter this function) or it just reached a
    // terminal state (go build the combined result).
    if let Some(review_execution_id) = &lead.review_execution_id {
        let review = session.executions.iter().find(|e| &e.execution_id == review_execution_id);
        let review_terminal = review.is_some_and(|r| {
            matches!(
                r.state,
                SessionState::Finished | SessionState::Stopped | SessionState::Failed | SessionState::Interrupted
            )
        });
        return if review_terminal {
            LeadReviewStep::ReviewFinishedBuildResultAndFinish {
                review_execution_id: review_execution_id.clone(),
            }
        } else {
            LeadReviewStep::ReviewStillRunning
        };
    }

    // No review turn recorded yet: mint its execution ID and record it on
    // the lead NOW, under this session's lock, before any engine has been
    // asked to run it -- so a concurrent re-entry (a second helper's
    // completion racing this one) sees `review_execution_id` already set and
    // takes the `ReviewStillRunning` branch instead of also trying to launch
    // one. This is the review-turn equivalent of `record_execution_if_not_running`'s
    // own "record before launch" ordering.
    let Some(integration_path) = lead.integration_worktree_path.clone() else {
        // No helper ever finished cleanly enough to provision one -- nothing
        // to review. Finish the graph directly with an honest summary
        // instead of waiting forever for a review turn that has nothing to
        // review.
        let finished = helpers.iter().filter(|h| h.state == SessionState::Finished).count();
        let _ = update_session_at(locks, root, session_id, |s| {
            if let Some(lead) = s.executions.iter_mut().find(|e| e.parent_execution_id.is_none()) {
                lead.state = SessionState::Finished;
                lead.ended_at = Some(now_rfc3339());
                lead.output_summary = Some(format!(
                    "{finished} of {} helpers finished, but none produced anything to review.",
                    helpers.len()
                ));
            }
            s.header.state = SessionState::Finished;
        });
        return LeadReviewStep::AlreadyFinished;
    };

    let review_execution_id = crate::agentdesk::execution_id_for_run_session(&new_id());
    let lead_execution_id = lead.execution_id.clone();
    let outcome = update_session_at(locks, root, session_id, |s| {
        // Re-check inside the lock: another thread may have recorded one
        // between the unlocked read above and this write.
        let already = s
            .executions
            .iter()
            .find(|e| e.parent_execution_id.is_none())
            .and_then(|l| l.review_execution_id.clone());
        if already.is_some() {
            return;
        }
        if let Some(lead) = s.executions.iter_mut().find(|e| e.execution_id == lead_execution_id) {
            lead.review_execution_id = Some(review_execution_id.clone());
        }
        // Pre-create the review's OWN `ExecutionRecord` here, with
        // `parent_execution_id: Some(lead)`, exactly the way
        // `commit_started_graph_if_still_proposed` pre-creates every
        // helper's record before that helper's engine ever runs. This is
        // load-bearing, not cosmetic: `agentdesk::bridge::apply_run_event`'s
        // `find_or_start_execution` creates a BRAND-NEW record with
        // `parent_execution_id: None` for any execution ID it has never seen
        // before, and `active_execution_id`/`session.header.state` are only
        // protected from a helper's own events by checking
        // `parent_execution_id.is_some()` (`bridge.rs`'s own "Helper
        // executions are exempt" comment). Without pre-creating this record,
        // the review turn's first `Working` event would be indistinguishable
        // from a brand-new LEAD execution to the bridge -- it would hijack
        // `active_execution_id` away from the actual lead and flip
        // `session.header.state` off of the review turn's own transient
        // states, exactly the bug that guard exists to prevent for helpers.
        // `Preparing`, not `Ready`/`Draft` -- `graph::schedule` treats any
        // `parent_execution_id.is_some()` node in `Ready`/`Draft` as a
        // launchable HELPER candidate. This record is never meant to be
        // picked up by that scheduler at all (it is launched directly by
        // `launch_lead_review`, immediately after this write, never via the
        // ready-queue path `advance_graph_after_helper_completion` drives
        // real helpers through), so it must never appear schedulable even
        // for the brief window between this write and the engine's first
        // event turning it `Working`. Matches
        // `record_execution_if_not_running`'s own choice of `Preparing` as
        // the seed state for a not-yet-launched execution.
        let mut review_record = crate::agentdesk::model::ExecutionRecord::minimal(
            review_execution_id.clone(),
            s.header.session_id.clone(),
            Some(lead_execution_id.clone()),
            SessionState::Preparing,
            now_rfc3339(),
            None,
            0,
        );
        // Cosmetic, but load-bearing for the Graph panel not showing this as
        // an unlabeled "Helper" row: `AgentGraphPanel`/`agentGraphProjection.ts`
        // both fall back to "Helper" only when `jobTitle` is `None`.
        review_record.job_title = Some("Lead review".to_string());
        review_record.job_description = Some(
            "Reviewing every helper's combined result in a dedicated worktree before finishing.".to_string(),
        );
        s.executions.push(review_record);
    });
    match outcome {
        UpdateOutcome::Updated { session } => {
            let lead = session.executions.iter().find(|e| e.parent_execution_id.is_none());
            match lead.and_then(|l| l.review_execution_id.clone()) {
                // Either this call's write won, or a concurrent one did --
                // either way SOME review execution id is now durably
                // recorded. If it is not the one this call minted, another
                // caller already owns launching it.
                Some(recorded) if recorded == review_execution_id => LeadReviewStep::NeedsReviewTurn { integration_path },
                Some(_) => LeadReviewStep::ReviewStillRunning,
                None => LeadReviewStep::ReviewStillRunning,
            }
        }
        _ => LeadReviewStep::ReviewStillRunning,
    }
}

/// Launches the lead's own REVIEW turn against the session's integration
/// worktree -- the same `ExecutionPolicy`/`ExecutionRegistry`/`run_task` path
/// [`launch_helper`] and `commands::agent_desk::start_execution_at` both use,
/// scoped by working directory rather than by a new mechanism.
///
/// Deliberately mirrors `launch_helper`'s own shape (register the link and
/// the cancel handle BEFORE the first event, a watchdog task that turns a
/// panic into a typed `Failed` event, remove the gate-answer channel and mark
/// the execution complete in the SAME places) rather than importing it,
/// because the two differ in exactly the same ways `launch_helper`'s own doc
/// comment already explains a shared helper/solo function would not cleanly
/// cover: different prompt source (every helper's own `output_summary`, not
/// one job description), different working directory resolution (already
/// known -- the integration worktree -- not derived from a `worktree_path`
/// field lookup), and a different policy constructor
/// (`ExecutionPolicy::resolve_for_lead_review`, unrestricted write, vs.
/// `resolve_for_helper`'s path-scoped one).
fn launch_lead_review(
    app: &AppHandle,
    locks: &std::sync::Arc<crate::agentdesk::SessionLocks>,
    root: &SessionStoreRoot,
    session_id: &str,
    integration_path: &str,
) {
    let Some(links) = app.try_state::<crate::agentdesk::RunSessionLinks>() else {
        return;
    };
    let Some(executions) = app.try_state::<crate::agentdesk::ExecutionRegistry>() else {
        return;
    };

    let session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(_) => return,
    };
    let Some(lead) = session.executions.iter().find(|e| e.parent_execution_id.is_none()) else {
        return;
    };
    let Some(review_execution_id) = lead.review_execution_id.clone() else {
        return;
    };

    // Every helper's own report, so the review turn actually reads what was
    // produced rather than re-discovering it from the diff alone -- "it
    // reviews what the helpers produced; it is not a formatting pass over a
    // count."
    let mut helper_reports = String::new();
    for helper in session.executions.iter().filter(|e| e.parent_execution_id.is_some()) {
        let title = helper.job_title.as_deref().unwrap_or("Helper");
        let state = format!("{:?}", helper.state);
        let summary = helper.output_summary.as_deref().unwrap_or("(no summary)");
        helper_reports.push_str(&format!("- {title} [{state}]: {summary}\n"));
    }
    let prompt = format!(
        "Every helper in this task has finished. Their results have been integrated into \
         your current working directory (a dedicated review worktree combining every helper's \
         changes). Review the combined result: confirm it is coherent, run any checks that make \
         sense, and fix anything that is broken or inconsistent across helpers' work. Report a \
         short summary of what you found and did.\n\nHelper reports:\n{helper_reports}"
    );

    let policy = crate::agentdesk::policy::ExecutionPolicy::resolve_for_lead_review();

    // Same execution-addressed link this whole package now requires (never
    // the repository) -- see `RunSessionLinks`'s own doc comment.
    links.link(&review_execution_id, &session_id.to_string());

    // The lead's review turn is a writing one, but it goes through the same
    // policy-aware discovery as every other launch so there is one path, not
    // a special case that could drift.
    let agent = match crate::ai::agent::cli_agent::CliAgent::discover_for(
        &policy,
        true,
        std::path::PathBuf::from(integration_path),
    ) {
        Ok(a) => a,
        Err(e) => {
            record_helper_launch_failure(
                locks,
                root,
                session_id,
                &review_execution_id,
                &crate::ai::agent::select::plain_explanation(&e),
            );
            // The lead review turn could not even start -- finish the graph
            // now with an honest summary rather than leaving it stuck
            // waiting forever for a review execution that will never
            // report in.
            if let Ok(session) = store::read_session(root, session_id) {
                finish_graph_with_combined_result(locks, root, session_id, &session, &review_execution_id);
            }
            return;
        }
    };

    let repo_id = session.header.repo_id.clone();
    let (answer_tx, answer_rx) = std::sync::mpsc::channel::<crate::airun::driver::GateAnswer>();
    crate::commands::airun::gate_answers()
        .lock()
        .unwrap()
        .insert((session_id.to_string(), review_execution_id.clone()), answer_tx);

    let cancel_handle = crate::airun::cli_run::CancelHandle::new();
    executions.register(session_id.to_string(), review_execution_id.clone(), cancel_handle.clone());

    let app_for_task = app.clone();
    let repo_for_task = repo_id.clone();
    let session_id_for_task = session_id.to_string();
    let review_execution_id_for_task = review_execution_id.clone();
    let executions_for_task = executions.inner().clone();
    let root_for_task = root.clone();
    let locks_for_task = locks.clone();
    // Owned before the move: the review turn is audited against the same
    // integration worktree it runs in, and a borrow cannot outlive this call.
    let integration_worktree = std::path::PathBuf::from(integration_path);
    let join_handle = tauri::async_runtime::spawn(async move {
        let sink: crate::airun::engine::Sink = {
            let app = app_for_task.clone();
            let repo = repo_for_task.clone();
            let sid = review_execution_id_for_task.clone();
            std::sync::Arc::new(move |state, step| {
                crate::commands::airun::emit_agent_desk_only(&app, &repo, &sid, state, step);
            })
        };

        crate::airun::cli_run::run_task(&agent, &format!("{}\n\nThe task:\n{}", crate::ai::agent::run::SYSTEM_PROMPT, prompt), sink, answer_rx, policy, true, cancel_handle, None, Some(integration_worktree.clone()), String::new()).await;

        crate::commands::airun::gate_answers()
            .lock()
            .unwrap()
            .remove(&(session_id_for_task.clone(), review_execution_id_for_task.clone()));
        executions_for_task.complete(&session_id_for_task, &review_execution_id_for_task);

        // The review turn's own completion: build the combined result and
        // mark the graph Finished. Re-reads current state under the session
        // lock rather than using anything captured before this task started.
        if let Ok(session) = store::read_session(&root_for_task, &session_id_for_task) {
            finish_graph_with_combined_result(&locks_for_task, &root_for_task, &session_id_for_task, &session, &review_execution_id_for_task);
        }
    });

    let app_for_watchdog = app.clone();
    let repo_for_watchdog = repo_id;
    let session_id_for_watchdog = session_id.to_string();
    let review_execution_id_for_watchdog = review_execution_id.clone();
    let executions_for_watchdog = executions.inner().clone();
    let locks_for_watchdog = locks.clone();
    let root_for_watchdog = root.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(join_error) = join_handle.await {
            log::error!(
                "agent desk lead review {} for session {} panicked: {join_error}",
                review_execution_id_for_watchdog,
                session_id_for_watchdog
            );
            crate::commands::airun::emit_agent_desk_only(
                &app_for_watchdog,
                &repo_for_watchdog,
                &review_execution_id_for_watchdog,
                crate::airun::driver::RunState::Failed,
                crate::airun::driver::RunStep::Ended {
                    state: crate::airun::driver::RunState::Failed,
                    detail: "Something went wrong while reviewing the combined result, and it never got to report why. The integration worktree is untouched.".into(),
                },
            );
            crate::commands::airun::gate_answers()
                .lock()
                .unwrap()
                .remove(&(session_id_for_watchdog.clone(), review_execution_id_for_watchdog.clone()));
            executions_for_watchdog.complete(&session_id_for_watchdog, &review_execution_id_for_watchdog);
            if let Ok(session) = store::read_session(&root_for_watchdog, &session_id_for_watchdog) {
                finish_graph_with_combined_result(
                    &locks_for_watchdog,
                    &root_for_watchdog,
                    &session_id_for_watchdog,
                    &session,
                    &review_execution_id_for_watchdog,
                );
            }
        }
    });
}

/// Builds the ONE combined [`crate::agentdesk::result::ResultRecord`] from
/// the session's integration worktree, keyed to the LEAD's own execution ID
/// (matching `ResultRecord`'s own doc comment: "a combined record whose
/// `execution_id` is the lead's own"), links every helper's execution ID onto
/// it (`linked_execution_ids`), and only then marks the lead (and the session
/// header) `Finished` -- the last step in the P1 sequence: "serialize helper
/// integration, run a lead review/check over the combined tree, build one
/// primary result, link helper-scoped results, then and only then mark
/// Finished."
///
/// Idempotent against being called twice for the same review execution (the
/// ordinary completion path AND the watchdog's panic path both call this,
/// and only one of them will find genuine work to do): `update_session_at`'s
/// mutation only flips the lead to `Finished` if it is still `Working`, so a
/// second call after the first already finished it is a harmless no-op
/// write.
fn finish_graph_with_combined_result(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    session: &AgentSession,
    review_execution_id: &str,
) {
    let Some(lead) = session.executions.iter().find(|e| e.parent_execution_id.is_none()) else {
        return;
    };
    if lead.state != SessionState::Working {
        // Already finished (the other completion path won the race) or this
        // session was never a graph in the first place.
        return;
    }
    if lead.review_execution_id.as_deref() != Some(review_execution_id) {
        // Not the review execution this lead is actually waiting on (a stale
        // call, or one from a superseded attempt) -- do nothing rather than
        // finishing the graph on the wrong execution's say-so.
        return;
    }

    // Excludes the review execution itself -- see `start_or_check_lead_review`'s
    // identical exclusion for why it carries `parent_execution_id: Some(lead)`
    // without being one of the helpers being linked/counted here.
    let helpers: Vec<&ExecutionRecord> = session
        .executions
        .iter()
        .filter(|e| e.parent_execution_id.is_some() && e.execution_id != review_execution_id)
        .collect();
    let finished_count = helpers.iter().filter(|h| h.state == SessionState::Finished).count();
    let linked_execution_ids: Vec<ExecutionId> = helpers.iter().map(|h| h.execution_id.clone()).collect();

    let integration_path = lead.integration_worktree_path.clone();
    let checks = crate::commands::agent_result::checks_for_execution(session, review_execution_id);
    let openspec_change_id = crate::commands::agent_desk::openspec_change_id_of(&session.header.source);

    // P1 "build one primary result": the combined record's `worktree_path`
    // is the integration worktree itself -- the same tree the review turn
    // just checked -- so the existing Keep/Undo/Commit/cleanup machinery
    // (`commands::agent_result`) works on it completely unchanged; it has no
    // idea this worktree came from a graph rather than a solo Fix run.
    let build = crate::commands::agent_result::build_result_at(
        locks,
        root,
        session_id,
        lead.execution_id.clone(),
        crate::agentdesk::result::ResultOutcomeKind::Finished,
        integration_path,
        None,
        lead.base_oid.clone(),
        checks,
        openspec_change_id,
    );
    let result_built = matches!(&build, crate::commands::agent_result::BuildResultOutcome::Built { .. });
    if result_built {
        let _ = crate::agentdesk::result::link_helper_results(locks, root, session_id, &lead.execution_id, &linked_execution_ids);
    }

    let review_state = session
        .executions
        .iter()
        .find(|e| e.execution_id == review_execution_id)
        .map(|e| e.state);

    // Finished has one meaning in this app: reviewed work is sitting there
    // ready to look over. Three things can each make that false, and each
    // used to be papered over with a note while the graph still said
    // Finished -- the exact failure an unattended run cannot detect.
    let outcome = graph_finish_outcome(review_state, result_built, describe_build_failure(&build));
    let lead_execution_id = lead.execution_id.clone();
    let helper_count = helpers.len();
    let _ = update_session_at(locks, root, session_id, |s| {
        if let Some(lead) = s.executions.iter_mut().find(|e| e.execution_id == lead_execution_id) {
            if lead.state != SessionState::Working {
                return;
            }
            lead.state = outcome.state;
            lead.ended_at = Some(now_rfc3339());
            lead.output_summary = Some(format!(
                "{finished_count} of {helper_count} helpers finished. {}",
                outcome.note
            ));
        }
        s.header.state = outcome.state;
    });
}

/// How a graph run actually ended, once the review turn and the combined
/// result are both accounted for.
#[derive(Debug, PartialEq, Eq)]
struct GraphFinishOutcome {
    state: SessionState,
    note: String,
}

/// A one-line reason when the combined result could not be written, or
/// `None` when it was.
fn describe_build_failure(build: &crate::commands::agent_result::BuildResultOutcome) -> Option<String> {
    use crate::commands::agent_result::BuildResultOutcome;
    match build {
        BuildResultOutcome::Built { .. } => None,
        BuildResultOutcome::SessionNotFound => Some("the chat could not be found".into()),
        BuildResultOutcome::ExecutionNotFound => Some("the run could not be found".into()),
        BuildResultOutcome::SessionDamaged { reason } => Some(reason.clone()),
        BuildResultOutcome::SessionUnavailable { detail } | BuildResultOutcome::WriteFailed { detail } => {
            Some(detail.clone())
        }
    }
}

/// The honest end state for a finished graph.
///
/// `Finished` is reserved for the one case that earns it: the lead's review
/// turn finished AND the combined result was written, so there is something
/// reviewed to open. A review that failed or was stopped, or a result that
/// could not be persisted, ends the graph as `Failed`/`Stopped` with the
/// reason said plainly. The helpers' own work is still on disk either way,
/// and the note says so, but the session never claims a review that did not
/// happen.
fn graph_finish_outcome(
    review_state: Option<SessionState>,
    result_built: bool,
    build_failure: Option<String>,
) -> GraphFinishOutcome {
    const HELPERS_KEPT: &str = "Every helper's own work is still here to look over.";
    if !result_built {
        let detail = build_failure.unwrap_or_else(|| "the combined result could not be saved".into());
        return GraphFinishOutcome {
            state: SessionState::Failed,
            note: format!("The combined result could not be saved ({detail}). {HELPERS_KEPT}"),
        };
    }
    match review_state {
        Some(SessionState::Finished) => GraphFinishOutcome {
            state: SessionState::Finished,
            note: "The lead reviewed the combined result.".into(),
        },
        Some(SessionState::Stopped) => GraphFinishOutcome {
            state: SessionState::Stopped,
            note: format!("The lead's review was stopped before finishing, so nothing was checked over. {HELPERS_KEPT}"),
        },
        Some(SessionState::Failed) => GraphFinishOutcome {
            state: SessionState::Failed,
            note: format!("The lead's review could not finish, so nothing was checked over. {HELPERS_KEPT}"),
        },
        _ => GraphFinishOutcome {
            state: SessionState::Failed,
            note: format!("The lead's review ended without a clear outcome, so nothing was checked over. {HELPERS_KEPT}"),
        },
    }
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
        Ok(s) => {
            // Results live in a sidecar file; a missing or unreadable one
            // only means no node can offer View changes yet, never that the
            // graph itself is unavailable.
            let results = crate::agentdesk::result::read_results(&root, &session_id).unwrap_or_default();
            GraphViewOutcome::Found {
                nodes: graph::project_graph(&s.executions, &s.messages, &results),
            }
        }
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
    let outcome = resolve_conflict_at(&locks_arc, &root, &session_id, &execution_id, resolution);

    // Resolving a conflict is a TERMINAL transition for that helper -- and it
    // can be the LAST one, since a conflicted helper is exactly the node
    // everything else was waiting on. Every other completion path calls this;
    // without it the graph sat in `Working` forever with every node showing
    // done, no review turn, no result, and nothing polling to recover it.
    if matches!(outcome, ResolveConflictOutcome::Resolved { .. }) {
        advance_graph_after_helper_completion(&app, &locks_arc, &root, &session_id, &execution_id);
    }

    Ok(outcome)
}

fn resolve_conflict_at(
    locks: &crate::agentdesk::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: &str,
    resolution: ConflictResolution,
) -> ResolveConflictOutcome {
    // Step 1: locked read-modify-write to take the conflict off the record
    // and compute what the resolution decided -- this is also where
    // `found`/`NoConflict` is decided, unchanged from before. The actual
    // filesystem write happens OUTSIDE this closure (see step 2's comment
    // for why), so this first pass does not yet flip the node to `Finished`
    // -- it stages the conflict's `path` and the resolved text, and restores
    // the conflict onto the record if the write later fails, so a person
    // never loses the choice they just made.
    let mut resolved_text = String::new();
    let mut conflict_path = String::new();
    let mut found = false;
    let stage_outcome = update_session_at(locks, root, session_id, |s| {
        if let Some(exec) = s
            .executions
            .iter_mut()
            .find(|e| e.execution_id == execution_id)
        {
            if let Some(conflict) = &exec.conflict {
                found = true;
                conflict_path = conflict.path.clone();
                resolved_text = match &resolution {
                    ConflictResolution::KeepHelper => conflict.helper_text.clone(),
                    ConflictResolution::KeepIntegrated => conflict.integrated_text.clone(),
                    ConflictResolution::UseMerged { text } => text.clone(),
                };
            }
        }
    });

    if !found {
        return ResolveConflictOutcome::NoConflict;
    }

    // The conflict was DETECTED in the integration worktree and the combined
    // result is BUILT from it, so the user's choice has to be written there
    // too. Writing to `session.header.repo_path` -- the user's own live
    // checkout -- put the resolved text somewhere the reviewed result never
    // reads, so the choice silently vanished from the result while a stray
    // write landed in the working copy. Falls back to `repo_path` only when
    // no integration worktree exists (a solo run's conflict), which is the
    // case that path was originally written for.
    let lead_path = match &stage_outcome {
        UpdateOutcome::Updated { session } => session
            .executions
            .iter()
            .find(|e| e.parent_execution_id.is_none())
            .and_then(|lead| lead.integration_worktree_path.clone())
            .unwrap_or_else(|| session.header.repo_path.clone()),
        UpdateOutcome::NotFound => return ResolveConflictOutcome::NotFound,
        UpdateOutcome::Damaged { reason } => {
            return ResolveConflictOutcome::Damaged {
                reason: reason.clone(),
            }
        }
        UpdateOutcome::WriteFailed { detail } => {
            return ResolveConflictOutcome::WriteFailed {
                detail: detail.clone(),
            }
        }
        UpdateOutcome::Unavailable { detail } => {
            return ResolveConflictOutcome::Unavailable {
                detail: detail.clone(),
            }
        }
    };

    // Step 2: the actual write into the lead's checkout -- the whole point
    // of resolving a conflict is that the user's choice ends up in their
    // working files, not just in the session record. Deliberately done
    // OUTSIDE the session lock: filesystem I/O should never happen while
    // holding a lock other mutating session calls are waiting on (see
    // `locks.rs`'s own module doc and the house rule against holding a
    // session lock across slow filesystem work).
    // Conflict resolution stays text-only and targets the LEAD's own
    // checkout directly (this is the user's explicit landing action, not the
    // automated P0-C integration pass, which never touches
    // `session.header.repo_path`) -- `FileExecutable::No` is a safe default
    // here since the conflict record itself only ever carried text.
    let resolved_content = FileContent::Text {
        bytes: resolved_text.clone().into_bytes(),
        executable: FileExecutable::No,
    };
    let write_failed_detail = match write_content_atomic(&lead_path, &conflict_path, &resolved_content) {
        ApplyOperationOutcome::Applied => None,
        ApplyOperationOutcome::Failed { detail, .. } => Some(detail),
    };

    // Step 3: only now, having confirmed whether the write actually landed,
    // either clear the conflict and move the node to Finished, or leave the
    // conflict exactly as it was (never taken off the record) so the user's
    // choice is not silently discarded -- they see the same conflict again
    // with a plain-language reason attached, and can retry.
    let outcome = match &write_failed_detail {
        None => update_session_at(locks, root, session_id, |s| {
            if let Some(exec) = s
                .executions
                .iter_mut()
                .find(|e| e.execution_id == execution_id)
            {
                exec.conflict = None;
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
        }),
        Some(detail) => update_session_at(locks, root, session_id, |s| {
            if let Some(exec) = s
                .executions
                .iter_mut()
                .find(|e| e.execution_id == execution_id)
            {
                // Conflict stays set (never taken) -- the choice was
                // computed but could not be saved, so it must still be
                // resolvable again rather than lost.
                exec.output_summary = Some(format!(
                    "Your choice for \"{conflict_path}\" could not be saved: {detail}. Try resolving it again."
                ));
            }
        }),
    };

    if let Some(detail) = write_failed_detail {
        return match outcome {
            UpdateOutcome::Updated { .. } => ResolveConflictOutcome::WriteFailed { detail },
            UpdateOutcome::NotFound => ResolveConflictOutcome::NotFound,
            UpdateOutcome::Damaged { reason } => ResolveConflictOutcome::Damaged { reason },
            UpdateOutcome::WriteFailed { detail } => ResolveConflictOutcome::WriteFailed { detail },
            UpdateOutcome::Unavailable { detail } => ResolveConflictOutcome::Unavailable { detail },
        };
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
            graph_started_at: None,
            preferred_provider: None,
            preferred_mode: None,
            preferred_team: None,
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

    // -- Task 3.3: "detect task/spec changes after draft and block Start
    // until refreshed or explicitly accepted." --

    mod stale_openspec_context {
        use super::*;
        use crate::agentdesk::model::SourceSnapshot;
        use std::fs;

        /// A real git repo (`start_graph_at` needs `RepoManager::open` to
        /// succeed) with a single-task OpenSpec change, and a session whose
        /// source is `OpenSpecTask` pointed at that task -- the shape
        /// `start_graph_at`'s new staleness check reads via
        /// `openspec_target_of`/`resolve_openspec_context`.
        fn repo_with_change_and_session(root: &std::path::Path, session_root: &SessionStoreRoot) -> (crate::state::RepoManager, String) {
            let repo = git2::Repository::init(root).expect("init repo");
            let change_dir = root.join("openspec").join("changes").join("add-thing");
            fs::create_dir_all(&change_dir).unwrap();
            fs::write(
                change_dir.join("proposal.md"),
                "# Change: Add thing\n\n## Why\n\nBecause.\n",
            )
            .unwrap();
            fs::write(change_dir.join("tasks.md"), "## 1. Group\n\n- [ ] 1.1 Do it\n").unwrap();

            // `start_graph_at` provisions a helper worktree from `HEAD` --
            // an unborn HEAD (no commits yet) makes `git worktree add` fail,
            // which is not what these tests are about, so give the repo one
            // real commit exactly like `Gate 4`'s own fixture pattern implies
            // a real repo needs (`commands::agent_desk`'s `repo_with_change`
            // never exercises `start_graph_at`, so it gets away without one --
            // this fixture does need it).
            {
                let mut index = repo.index().expect("repo index");
                index.add_path(std::path::Path::new("openspec/changes/add-thing/proposal.md")).unwrap();
                index.add_path(std::path::Path::new("openspec/changes/add-thing/tasks.md")).unwrap();
                index.write().unwrap();
                let tree_id = index.write_tree().unwrap();
                let tree = repo.find_tree(tree_id).unwrap();
                let sig = git2::Signature::now("Test", "test@example.com").unwrap();
                repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
            }

            let manager = crate::state::RepoManager::default();
            let (repo_id, _open, _reused) =
                manager.open(root.to_str().expect("utf8 path")).expect("open repo");

            let header = AgentSessionHeader {
                schema_version: CURRENT_SCHEMA_VERSION,
                session_id: "sess-1".into(),
                repo_id: repo_id.clone(),
                repo_path: root.to_string_lossy().into_owned(),
                repo_name: "widgets".into(),
                title: "Add thing".into(),
                source: SessionSource::OpenSpecTask {
                    change_id: "add-thing".into(),
                    task_index: 0,
                    task_text: "1.1 Do it".into(),
                    snapshot: SourceSnapshot {
                        title: "Add thing".into(),
                        summary: "1.1 Do it".into(),
                        captured_at: now_rfc3339(),
                        live_unavailable: false,
                    },
                },
                intent: SessionIntent::Fix,
                state: SessionState::Ready,
                created_at: now_rfc3339(),
                updated_at: now_rfc3339(),
                unread: false,
                changed_file_count: 0,
                active_execution_id: None,
                archived: false,
                graph_started_at: None,
                preferred_provider: None,
                preferred_mode: None,
                preferred_team: None,
            };
            store::write_session(session_root, &AgentSession::new(header)).unwrap();

            (manager, repo_id)
        }

        /// Computes the context fingerprint for the seeded change exactly the
        /// way `start_execution_at` would have when the lead was launched --
        /// shared by both tests below so "matches current" and "does not
        /// match current" are unambiguous relative to the same computation.
        fn fingerprint_for(root: &std::path::Path) -> String {
            let target = crate::commands::agent_desk::OpenSpecTarget::Task {
                change_id: "add-thing".into(),
                task_index: 0,
                task_text: "1.1 Do it".into(),
                snapshot_title: "Add thing".into(),
            };
            let ctx = resolve_openspec_context(root, &target).expect("context builds");
            crate::agentdesk::openspec_context::fingerprint(&ctx)
        }

        #[test]
        fn start_refuses_with_stale_when_tasks_md_changed_since_the_lead_was_launched() {
            let (dir, root) = temp_root();
            let locks = crate::agentdesk::SessionLocks::new();
            let (manager, _repo_id) = repo_with_change_and_session(dir.path(), &root);

            let launched_fingerprint = fingerprint_for(dir.path());

            // Persist a lead execution stamped with the ORIGINAL fingerprint,
            // then change tasks.md -- the file the fingerprint was built
            // from -- so the live fingerprint no longer matches.
            update_session_at(&locks, &root, "sess-1", |s| {
                let mut lead = ExecutionRecord::minimal(
                    "lead-1".into(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::NeedsInput,
                    now_rfc3339(),
                    None,
                    0,
                );
                lead.context_fingerprint = Some(launched_fingerprint.clone());
                lead.proposed_graph = Some(sample_graph());
                s.executions.push(lead);
            });
            fs::write(
                dir.path().join("openspec/changes/add-thing/tasks.md"),
                "## 1. Group\n\n- [ ] 1.1 Do it\n- [ ] 1.2 A new task inserted after drafting\n",
            )
            .unwrap();

            let outcome = start_graph_at(&locks, &root, &manager, "sess-1");
            match outcome {
                StartGraphOutcome::Stale { lead_execution_id, current_fingerprint } => {
                    assert_eq!(lead_execution_id, "lead-1");
                    assert_ne!(current_fingerprint, launched_fingerprint, "the whole point: they must differ");
                }
                other => panic!("expected Stale, got {other:?}"),
            }

            // Nothing was provisioned or written -- no helper worktree, no
            // state change on the session.
            let session = store::read_session(&root, "sess-1").unwrap();
            assert_eq!(session.header.state, SessionState::Ready);
            assert!(session.executions.iter().all(|e| e.worktree_path.is_none()));
        }

        #[test]
        fn start_proceeds_when_nothing_changed_since_the_lead_was_launched() {
            let (dir, root) = temp_root();
            let locks = crate::agentdesk::SessionLocks::new();
            let (manager, _repo_id) = repo_with_change_and_session(dir.path(), &root);

            let launched_fingerprint = fingerprint_for(dir.path());
            update_session_at(&locks, &root, "sess-1", |s| {
                let mut lead = ExecutionRecord::minimal(
                    "lead-1".into(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::NeedsInput,
                    now_rfc3339(),
                    None,
                    0,
                );
                lead.context_fingerprint = Some(launched_fingerprint);
                lead.proposed_graph = Some(sample_graph());
                s.executions.push(lead);
            });

            let outcome = start_graph_at(&locks, &root, &manager, "sess-1");
            assert!(
                matches!(outcome, StartGraphOutcome::Started { .. }),
                "nothing changed, so Start must not refuse: {outcome:?}"
            );
        }

        #[test]
        fn start_proceeds_once_the_user_has_accepted_the_exact_current_drift() {
            let (dir, root) = temp_root();
            let locks = crate::agentdesk::SessionLocks::new();
            let (manager, _repo_id) = repo_with_change_and_session(dir.path(), &root);

            let launched_fingerprint = fingerprint_for(dir.path());
            update_session_at(&locks, &root, "sess-1", |s| {
                let mut lead = ExecutionRecord::minimal(
                    "lead-1".into(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::NeedsInput,
                    now_rfc3339(),
                    None,
                    0,
                );
                lead.context_fingerprint = Some(launched_fingerprint.clone());
                lead.proposed_graph = Some(sample_graph());
                s.executions.push(lead);
            });
            fs::write(
                dir.path().join("openspec/changes/add-thing/tasks.md"),
                "## 1. Group\n\n- [ ] 1.1 Do it\n- [ ] 1.2 A new task inserted after drafting\n",
            )
            .unwrap();

            // First call still refuses -- acceptance has not happened yet.
            assert!(matches!(
                start_graph_at(&locks, &root, &manager, "sess-1"),
                StartGraphOutcome::Stale { .. }
            ));

            let accept_outcome = accept_stale_openspec_context_at(&locks, &root, "sess-1");
            assert!(
                matches!(accept_outcome, AcceptStaleOpenSpecContextOutcome::Accepted { .. }),
                "expected Accepted, got {accept_outcome:?}"
            );

            let outcome = start_graph_at(&locks, &root, &manager, "sess-1");
            assert!(
                matches!(outcome, StartGraphOutcome::Started { .. }),
                "the accepted drift must not refuse Start again: {outcome:?}"
            );
        }

        #[test]
        fn a_further_change_after_acceptance_refuses_again() {
            let (dir, root) = temp_root();
            let locks = crate::agentdesk::SessionLocks::new();
            let (manager, _repo_id) = repo_with_change_and_session(dir.path(), &root);

            let launched_fingerprint = fingerprint_for(dir.path());
            update_session_at(&locks, &root, "sess-1", |s| {
                let mut lead = ExecutionRecord::minimal(
                    "lead-1".into(),
                    s.header.session_id.clone(),
                    None,
                    SessionState::NeedsInput,
                    now_rfc3339(),
                    None,
                    0,
                );
                lead.context_fingerprint = Some(launched_fingerprint);
                lead.proposed_graph = Some(sample_graph());
                s.executions.push(lead);
            });
            fs::write(
                dir.path().join("openspec/changes/add-thing/tasks.md"),
                "## 1. Group\n\n- [ ] 1.1 Do it\n- [ ] 1.2 First drift\n",
            )
            .unwrap();
            accept_stale_openspec_context_at(&locks, &root, "sess-1");

            // A SECOND, later change after acceptance -- the accepted
            // fingerprint no longer matches the (now different again)
            // current one, so Start must refuse once more rather than
            // treating the earlier acceptance as a blanket waiver.
            fs::write(
                dir.path().join("openspec/changes/add-thing/tasks.md"),
                "## 1. Group\n\n- [ ] 1.1 Do it\n- [ ] 1.2 First drift\n- [ ] 1.3 Second drift\n",
            )
            .unwrap();

            let outcome = start_graph_at(&locks, &root, &manager, "sess-1");
            assert!(
                matches!(outcome, StartGraphOutcome::Stale { .. }),
                "a further drift after acceptance must refuse again: {outcome:?}"
            );
        }
    }

    #[test]
    fn record_conflict_then_resolve_keep_helper_preserves_both_texts_until_resolved() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        // A real directory: resolving now writes the chosen text to disk, so
        // the fake `C:/code/widgets` `seed_session` default cannot be used
        // here.
        let lead_dir = tempfile::tempdir().unwrap();
        update_session_at(&locks, &root, "sess-1", |s| {
            s.header.repo_path = lead_dir.path().to_string_lossy().into_owned();
        });

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
        // Real directory -- resolving now writes to disk.
        let lead_dir = tempfile::tempdir().unwrap();
        update_session_at(&locks, &root, "sess-1", |s| {
            s.header.repo_path = lead_dir.path().to_string_lossy().into_owned();
        });

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
        let views = graph::project_graph(&session.executions, &session.messages, &[]);
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

    // -- helper_delta / read_content_at_revision (R6.7, P0-C, P0-D) --

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

    /// Builds a fresh helper worktree at `base_files`, then writes
    /// `edits` on top and leaves them UNCOMMITTED -- staged and unstaged,
    /// never `git commit`ed -- matching the shipped path: helpers have no
    /// production commit tool, so a real helper's edits are exactly this
    /// shape when it finishes. Returns the worktree dir and its base OID.
    fn commit_helper_worktree(base_files: &[(&str, &str)], edits: &[(&str, &str)]) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let (repo, base_oid) = init_repo_with_commit(dir.path(), base_files);
        for (name, content) in edits {
            std::fs::write(dir.path().join(name), content).unwrap();
        }
        // Half the edits go through the index (staged), the other half stay
        // as plain unstaged worktree edits -- so this fixture exercises both
        // halves of "staged, unstaged, and committed" in one pass, matching
        // what `diff_tree_to_workdir_with_index` is specifically meant to
        // see in a single diff.
        if let Some((first_name, _)) = edits.first() {
            let mut index = repo.index().expect("index");
            index.add_path(std::path::Path::new(first_name)).expect("stage");
            index.write().expect("write index");
        }
        (dir, base_oid)
    }

    // P0-C: `helper_delta` must see UNSTAGED and STAGED work, not only
    // committed work -- this is the test that proves the shipped path
    // works, since helpers have no production commit tool.

    #[test]
    fn helper_delta_sees_an_unstaged_uncommitted_edit() {
        let dir = tempfile::tempdir().unwrap();
        let (_repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a"), ("b.txt", "b")]);
        // Deliberately NOT staged, NOT committed -- exactly what a helper
        // leaves behind in the shipped path.
        std::fs::write(dir.path().join("a.txt"), "a-changed").unwrap();

        let changed = helper_delta(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        match &changed[0] {
            FileOperation::Modify { path, content } => {
                assert_eq!(path, "a.txt");
                assert_eq!(as_text(content), "a-changed");
            }
            other => panic!("expected Modify, got {other:?}"),
        }
    }

    #[test]
    fn helper_delta_sees_a_staged_uncommitted_edit() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a")]);
        std::fs::write(dir.path().join("a.txt"), "a-staged").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
        // Still no commit.

        let changed = helper_delta(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        match &changed[0] {
            FileOperation::Modify { path, content } => {
                assert_eq!(path, "a.txt");
                assert_eq!(as_text(content), "a-staged");
            }
            other => panic!("expected Modify, got {other:?}"),
        }
    }

    #[test]
    fn helper_delta_reports_nothing_when_workdir_equals_base() {
        let dir = tempfile::tempdir().unwrap();
        let (_repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a")]);
        let changed = helper_delta(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert!(changed.is_empty());
    }

    #[test]
    fn helper_delta_sees_committed_work_too() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a")]);
        std::fs::write(dir.path().join("a.txt"), "a-committed").unwrap();
        commit_all(&repo, "change a");

        let changed = helper_delta(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        assert_eq!(changed[0].path(), "a.txt");
    }

    // P0-D: delete, rename, binary, and executable-bit changes must each
    // survive `helper_delta` faithfully as their own typed operation.

    #[test]
    fn helper_delta_represents_a_delete_as_a_typed_delete_not_an_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let (_repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a"), ("b.txt", "b")]);
        std::fs::remove_file(dir.path().join("a.txt")).unwrap();

        let changed = helper_delta(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        match &changed[0] {
            FileOperation::Delete { path } => assert_eq!(path, "a.txt"),
            other => panic!("expected Delete, got {other:?}"),
        }
    }

    #[test]
    fn helper_delta_represents_a_rename_with_its_from_path() {
        let dir = tempfile::tempdir().unwrap();
        let (_repo, base_oid) =
            init_repo_with_commit(dir.path(), &[("old_name.txt", "the exact same content, long enough to detect as a rename by similarity")]);
        std::fs::rename(dir.path().join("old_name.txt"), dir.path().join("new_name.txt")).unwrap();

        let changed = helper_delta(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        match &changed[0] {
            FileOperation::Rename { from_path, path, content } => {
                assert_eq!(from_path, "old_name.txt");
                assert_eq!(path, "new_name.txt");
                assert_eq!(
                    as_text(content),
                    "the exact same content, long enough to detect as a rename by similarity"
                );
            }
            other => panic!("expected Rename, got {other:?}"),
        }
    }

    #[test]
    fn helper_delta_represents_binary_content_as_raw_bytes_never_utf8_decoded() {
        let dir = tempfile::tempdir().unwrap();
        // Invalid UTF-8 byte sequence plus a NUL, which git's own binary
        // heuristic also treats as binary -- this must never round-trip
        // through `String::from_utf8_lossy` and come out changed.
        let binary_bytes: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01, 0x02, 0xC3, 0x28];
        let (_repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "placeholder")]);
        std::fs::write(dir.path().join("image.bin"), &binary_bytes).unwrap();

        let changed = helper_delta(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        match &changed[0] {
            FileOperation::Add { path, content: FileContent::Binary { bytes, .. } } => {
                assert_eq!(path, "image.bin");
                assert_eq!(bytes, &binary_bytes, "binary bytes must round-trip exactly");
            }
            other => panic!("expected a binary Add, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn helper_delta_represents_an_executable_bit_change() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let (_repo, base_oid) = init_repo_with_commit(dir.path(), &[("script.sh", "#!/bin/sh\necho hi\n")]);
        let path = dir.path().join("script.sh");
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(perms.mode() | 0o111);
        std::fs::set_permissions(&path, perms).unwrap();

        let changed = helper_delta(dir.path().to_str().unwrap(), &base_oid).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        match &changed[0] {
            FileOperation::Modify { content: FileContent::Text { executable, .. }, .. } => {
                assert_eq!(*executable, FileExecutable::Yes);
            }
            other => panic!("expected an executable-bit Modify, got {other:?}"),
        }
    }

    #[test]
    fn read_content_at_revision_reads_the_base_text() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "original")]);
        std::fs::write(dir.path().join("a.txt"), "changed").unwrap();
        commit_all(&repo, "change a");

        let base_content = read_content_at_revision(dir.path().to_str().unwrap(), &base_oid, "a.txt");
        assert_eq!(base_content.map(|c| as_text(&c)), Some("original".to_string()));
    }

    #[test]
    fn read_content_at_revision_returns_none_for_a_file_that_did_not_exist_at_base() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, base_oid) = init_repo_with_commit(dir.path(), &[("a.txt", "a")]);
        std::fs::write(dir.path().join("new.txt"), "brand new").unwrap();
        commit_all(&repo, "add new.txt");

        let base_content = read_content_at_revision(dir.path().to_str().unwrap(), &base_oid, "new.txt");
        assert!(base_content.is_none(), "a path absent at the base must read as None, not a fabricated empty file");
    }

    // -- apply_operation (P0-D): a read/write failure must leave the target
    // path untouched and report a typed outcome. --

    #[test]
    fn apply_operation_delete_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("gone.txt"), "still here").unwrap();
        let outcome = apply_operation(dir.path().to_str().unwrap(), &FileOperation::Delete { path: "gone.txt".into() });
        assert_eq!(outcome, ApplyOperationOutcome::Applied);
        assert!(!dir.path().join("gone.txt").exists());
    }

    #[test]
    fn apply_operation_rename_moves_content_to_the_new_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.txt"), "original content").unwrap();
        let outcome = apply_operation(
            dir.path().to_str().unwrap(),
            &FileOperation::Rename {
                from_path: "old.txt".into(),
                path: "new.txt".into(),
                content: FileContent::Text { bytes: b"original content".to_vec(), executable: FileExecutable::No },
            },
        );
        assert_eq!(outcome, ApplyOperationOutcome::Applied);
        assert!(!dir.path().join("old.txt").exists());
        assert_eq!(std::fs::read_to_string(dir.path().join("new.txt")).unwrap(), "original content");
    }

    #[test]
    fn a_write_failure_leaves_the_target_path_untouched_and_reports_a_typed_outcome() {
        let dir = tempfile::tempdir().unwrap();
        // "blocked" is a FILE, not a directory -- writing to
        // "blocked/nested.txt" cannot succeed, simulating a real integration
        // failure without relying on OS-specific permission APIs.
        std::fs::write(dir.path().join("blocked"), "not a directory").unwrap();

        let outcome = apply_operation(
            dir.path().to_str().unwrap(),
            &FileOperation::Add {
                path: "blocked/nested.txt".into(),
                content: FileContent::Text { bytes: b"new content".to_vec(), executable: FileExecutable::No },
            },
        );
        match outcome {
            ApplyOperationOutcome::Failed { path, detail } => {
                assert_eq!(path, "blocked/nested.txt");
                assert!(!detail.is_empty());
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        // The pre-existing "blocked" file must be exactly as it was --
        // never partially overwritten or removed by the failed attempt.
        assert_eq!(std::fs::read_to_string(dir.path().join("blocked")).unwrap(), "not a directory");
        assert!(!dir.path().join("blocked/nested.txt").exists());
    }
    // -- Tasks 5.5/5.10: the two integration gaps the 2026-08-22 audit left
    // open as "reasoned but unproven". Both claims now have a test. --

    /// Task 5.10: a symlink must be integrated as a real link, not as a text
    /// file whose contents happen to be the link target. `create_symlink`
    /// and `read_symlink_target` have existed since 5.4 with no test, and on
    /// Windows link creation needs developer mode or elevation -- so this
    /// asserts the real behaviour where links can be made and, where they
    /// cannot, asserts the typed failure rather than silently "passing" by
    /// writing a text file that looks like a link.
    ///
    /// KNOWN COVERAGE LIMIT: on a Windows machine without developer mode or
    /// elevation -- including the machine this was written on -- symlink
    /// creation is refused by the OS, so this test takes the `Failed` branch
    /// and proves only that a refused link fails cleanly (typed outcome, no
    /// text stand-in, no temp file left behind). The `Applied` branch is
    /// exercised on Unix and on Windows with developer mode on. Do not read
    /// a green run here as proof that real link creation works on this box.
    #[test]
    fn apply_operation_writes_a_symlink_as_a_real_link_or_fails_typed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("real.txt"), "the real file").unwrap();

        let outcome = apply_operation(
            dir.path().to_str().unwrap(),
            &FileOperation::Add {
                path: "link.txt".into(),
                content: FileContent::Symlink { target: "real.txt".into() },
            },
        );

        let link_path = dir.path().join("link.txt");
        match outcome {
            ApplyOperationOutcome::Applied => {
                // A real link, not a regular file containing "real.txt".
                let meta = std::fs::symlink_metadata(&link_path).unwrap();
                assert!(meta.file_type().is_symlink(), "integration must create a real symlink, not a text file holding the target path");
                // And it must resolve to the file it names.
                assert_eq!(std::fs::read_to_string(&link_path).unwrap(), "the real file");
            }
            ApplyOperationOutcome::Failed { path, detail } => {
                // Windows without developer mode/elevation. The failure must
                // be typed and name the path, and must NOT have left a
                // stand-in regular file behind that a later read would
                // mistake for the link.
                assert_eq!(path, "link.txt");
                assert!(!detail.is_empty(), "a failed link must explain itself");
                assert!(!link_path.exists(), "a failed symlink must leave nothing behind, never a text stand-in");
            }
        }

        // Either way, no temp file may survive the attempt.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".gitwyrm-integrate-"))
            .collect();
        assert!(leftovers.is_empty(), "integration left temp files behind: {leftovers:?}");
    }

    /// Tasks 5.5 and 5.10: the exactly-once claim. A batch interrupted
    /// partway must, on restart, finish the remaining operations and leave
    /// the already-applied ones exactly as they were -- never applying one
    /// twice and never reporting a partial batch as complete.
    ///
    /// The interruption is modelled the way a real crash presents itself:
    /// the process stops after operation 2 of 5, and the "restart" re-runs
    /// the WHOLE batch from the beginning, which is precisely what
    /// `integrate_helper_into` does on the next completion event. The proof
    /// is that re-running is safe: every path holds its recorded content
    /// once, and a delete already performed stays deleted rather than
    /// failing the retry.
    #[test]
    fn an_interrupted_integration_batch_resumes_exactly_once_on_restart() {
        let dir = tempfile::tempdir().unwrap();
        let repo_path = dir.path().to_str().unwrap();
        // Pre-existing tree: one file to modify, one to delete, one to rename.
        std::fs::write(dir.path().join("modify.txt"), "before").unwrap();
        std::fs::write(dir.path().join("delete.txt"), "doomed").unwrap();
        std::fs::write(dir.path().join("old-name.txt"), "moving").unwrap();

        let batch = vec![
            FileOperation::Modify {
                path: "modify.txt".into(),
                content: FileContent::Text { bytes: b"after".to_vec(), executable: FileExecutable::No },
            },
            FileOperation::Delete { path: "delete.txt".into() },
            FileOperation::Add {
                path: "nested/added.txt".into(),
                content: FileContent::Text { bytes: b"added".to_vec(), executable: FileExecutable::No },
            },
            FileOperation::Rename {
                from_path: "old-name.txt".into(),
                path: "new-name.txt".into(),
                content: FileContent::Text { bytes: b"moving".to_vec(), executable: FileExecutable::No },
            },
            FileOperation::Add {
                path: "last.txt".into(),
                content: FileContent::Text { bytes: b"last".to_vec(), executable: FileExecutable::No },
            },
        ];

        // First pass: the "crash" lands after operation 2 of 5.
        for operation in batch.iter().take(2) {
            assert_eq!(apply_operation(repo_path, operation), ApplyOperationOutcome::Applied);
        }
        assert_eq!(std::fs::read_to_string(dir.path().join("modify.txt")).unwrap(), "after");
        assert!(!dir.path().join("delete.txt").exists());
        // The tail of the batch has genuinely not happened yet.
        assert!(!dir.path().join("nested/added.txt").exists());
        assert!(!dir.path().join("new-name.txt").exists());
        assert!(!dir.path().join("last.txt").exists());

        // Restart: the whole batch replays, including the two already done.
        for operation in batch.iter() {
            assert_eq!(
                apply_operation(repo_path, operation),
                ApplyOperationOutcome::Applied,
                "replaying an already-applied operation must succeed, not fail the resumed batch"
            );
        }

        // Every operation is now applied exactly once.
        assert_eq!(std::fs::read_to_string(dir.path().join("modify.txt")).unwrap(), "after");
        assert!(!dir.path().join("delete.txt").exists(), "a replayed delete must stay deleted");
        assert_eq!(std::fs::read_to_string(dir.path().join("nested/added.txt")).unwrap(), "added");
        assert_eq!(std::fs::read_to_string(dir.path().join("new-name.txt")).unwrap(), "moving");
        assert!(!dir.path().join("old-name.txt").exists(), "a replayed rename must not resurrect the old path");
        assert_eq!(std::fs::read_to_string(dir.path().join("last.txt")).unwrap(), "last");

        // No temp files survive either pass.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".gitwyrm-integrate-"))
            .collect();
        assert!(leftovers.is_empty(), "resumed integration left temp files behind: {leftovers:?}");
    }

    // -- record_helper_launch_failure / start_or_check_lead_review /
    // finish_graph_with_combined_result (P1 "Finished is not a combined
    // graph result") --

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

    /// P1 "a graph is NOT Finished until the lead review turn has run":
    /// once every helper is terminal but no review turn has been launched
    /// yet, `start_or_check_lead_review` must ask the caller to launch one
    /// (`NeedsReviewTurn`) -- and the lead must stay `Working`, never jump
    /// straight to `Finished` off the helper count alone the way the old
    /// `mark_graph_finished_if_all_terminal` did.
    #[test]
    fn start_or_check_lead_review_asks_for_a_review_turn_once_every_helper_is_terminal() {
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
            lead.integration_worktree_path = Some("C:/fake/integration".into());
            s.executions.push(lead);
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
        let step = start_or_check_lead_review(&locks, &root, "sess-1", &session);
        match step {
            LeadReviewStep::NeedsReviewTurn { integration_path } => {
                assert_eq!(integration_path, "C:/fake/integration");
            }
            other => panic!("expected NeedsReviewTurn, got {other:?}"),
        }

        // The lead must stay Working -- Finished is not decided by helper
        // count alone anymore.
        let session = store::read_session(&root, "sess-1").unwrap();
        let lead = session.executions.iter().find(|e| e.execution_id == "lead").unwrap();
        assert_eq!(lead.state, SessionState::Working, "must not finish before a review turn has even run");
        assert!(lead.review_execution_id.is_some(), "the review execution id must be durably recorded before launch");
    }

    #[test]
    fn start_or_check_lead_review_does_nothing_while_a_helper_is_still_active() {
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
        let step = start_or_check_lead_review(&locks, &root, "sess-1", &session);
        assert!(matches!(step, LeadReviewStep::NotYetAllTerminal));

        let session = store::read_session(&root, "sess-1").unwrap();
        let lead = session.executions.iter().find(|e| e.execution_id == "lead").unwrap();
        assert_eq!(lead.state, SessionState::Working, "must not finish while helper-a is still Working");
    }

    /// Once a review execution id is recorded and that execution has itself
    /// reached a terminal state, `start_or_check_lead_review` must report
    /// `ReviewFinishedBuildResultAndFinish` -- the caller
    /// (`finish_graph_with_combined_result`) is what actually builds the
    /// result and flips the lead to `Finished`.
    #[test]
    fn start_or_check_lead_review_reports_ready_to_finish_once_the_review_turn_is_terminal() {
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
            lead.integration_worktree_path = Some("C:/fake/integration".into());
            lead.review_execution_id = Some("review-1".into());
            s.executions.push(lead);
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
                "review-1".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Finished,
                now_rfc3339(),
                None,
                0,
            ));
        });

        let session = store::read_session(&root, "sess-1").unwrap();
        let step = start_or_check_lead_review(&locks, &root, "sess-1", &session);
        match step {
            LeadReviewStep::ReviewFinishedBuildResultAndFinish { review_execution_id } => {
                assert_eq!(review_execution_id, "review-1");
            }
            other => panic!("expected ReviewFinishedBuildResultAndFinish, got {other:?}"),
        }
    }

    /// A review execution id recorded but still `Working` must report
    /// `ReviewStillRunning` -- never re-launch a second review turn and
    /// never finish early.
    #[test]
    fn start_or_check_lead_review_waits_while_the_review_turn_itself_is_still_running() {
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
            lead.integration_worktree_path = Some("C:/fake/integration".into());
            lead.review_execution_id = Some("review-1".into());
            s.executions.push(lead);
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
                "review-1".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Working,
                now_rfc3339(),
                None,
                0,
            ));
        });

        let session = store::read_session(&root, "sess-1").unwrap();
        let step = start_or_check_lead_review(&locks, &root, "sess-1", &session);
        assert!(matches!(step, LeadReviewStep::ReviewStillRunning));
    }

    /// `finish_graph_with_combined_result` is where the graph actually
    /// becomes `Finished` -- and only once the combined `ResultRecord` has
    /// been built from the integration worktree and every helper's execution
    /// id is linked onto it (P1's whole required outcome, verbatim: "build
    /// one primary result, link helper-scoped results, then and only then
    /// mark Finished").
    #[test]
    fn finish_graph_with_combined_result_builds_the_result_and_links_every_helper() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");

        // `changed_paths_for_worktree` (`commands::agent_result`) opens the
        // worktree with git2 and walks its status -- a plain non-git
        // tempdir would silently read back as zero changed paths
        // (`unwrap_or_default()` at the call site), which would make this
        // test pass even if the combined result never actually saw the
        // file. Init a real repo so the assertion below is meaningful.
        let integration_dir = tempfile::tempdir().unwrap();
        let _ = git2::Repository::init(integration_dir.path()).unwrap();
        std::fs::write(integration_dir.path().join("combined.txt"), "from the integration worktree").unwrap();
        let integration_path = integration_dir.path().to_string_lossy().into_owned();

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
            lead.integration_worktree_path = Some(integration_path.clone());
            lead.review_execution_id = Some("review-1".into());
            s.executions.push(lead);
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
                SessionState::Finished,
                now_rfc3339(),
                None,
                0,
            ));
            s.executions.push(ExecutionRecord::minimal(
                "review-1".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::Finished,
                now_rfc3339(),
                None,
                0,
            ));
        });

        let session = store::read_session(&root, "sess-1").unwrap();
        finish_graph_with_combined_result(&locks, &root, "sess-1", &session, "review-1");

        let session = store::read_session(&root, "sess-1").unwrap();
        let lead = session.executions.iter().find(|e| e.execution_id == "lead").unwrap();
        assert_eq!(lead.state, SessionState::Finished, "Finished only after the combined result was built");
        assert_eq!(session.header.state, SessionState::Finished);

        let results = crate::agentdesk::result::read_results(&root, "sess-1").unwrap();
        let combined = results.iter().find(|r| r.execution_id == "lead").expect("combined result must exist, keyed to the lead's own execution id");
        assert_eq!(combined.worktree_path.as_deref(), Some(integration_path.as_str()), "the combined result must reference the integration worktree, not session.header.repo_path");
        assert!(
            combined.changed_paths.iter().any(|p| p.path == "combined.txt"),
            "the combined result must list every helper's changes, read from the integration worktree"
        );
        let mut linked = combined.linked_execution_ids.clone();
        linked.sort();
        assert_eq!(linked, vec!["helper-a".to_string(), "helper-b".to_string()], "every helper's execution id must be linked onto the combined result");

        // The integration worktree must NOT have been removed by finishing
        // the graph -- P1 "do not destroy the evidence": cleanup belongs to
        // the result landing flow (Keep/Commit/Undo -> cleanup), not to
        // graph finish.
        assert!(integration_dir.path().is_dir(), "the integration worktree must survive Finished");
        assert!(integration_dir.path().join("combined.txt").exists());
    }

    /// Two helpers whose completion races land on the SAME integration
    /// worktree must not interleave: `with_integration_lock` must serialize
    /// them so the second call always sees the first's already-applied
    /// change rather than a half-written intermediate state.
    #[test]
    fn helper_integration_is_serialized_per_session_not_interleaved() {
        let locks = std::sync::Arc::new(crate::agentdesk::SessionLocks::new());
        let integration_dir = tempfile::tempdir().unwrap();
        let integration_path = integration_dir.path().to_string_lossy().into_owned();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        let run = |tag: &'static str, locks: std::sync::Arc<crate::agentdesk::SessionLocks>, barrier: std::sync::Arc<std::sync::Barrier>, integration_path: String| {
            // The barrier is OUTSIDE the lock on purpose. Inside it, the
            // first thread to take the lock would wait for a peer that can
            // never arrive -- it is blocked on the very lock being held --
            // and the test deadlocks rather than proving anything. Out here
            // it does its real job: both threads reach the lock at the same
            // moment, so whichever loses genuinely contends for it.
            barrier.wait();
            with_integration_lock(&locks, "sess-1", || {
                // Simulate a slow read-modify-write against the shared
                // integration worktree: read current content, sleep (widen
                // the interleave window if the lock did not hold), append,
                // write back.
                let marker = std::path::Path::new(&integration_path).join("order.txt");
                let before = std::fs::read_to_string(&marker).unwrap_or_default();
                std::thread::sleep(std::time::Duration::from_millis(20));
                std::fs::write(&marker, format!("{before}{tag}\n")).unwrap();
            });
        };

        std::thread::scope(|scope| {
            let l1 = locks.clone();
            let b1 = barrier.clone();
            let p1 = integration_path.clone();
            let t1 = scope.spawn(move || run("A", l1, b1, p1));
            let l2 = locks.clone();
            let b2 = barrier.clone();
            let p2 = integration_path.clone();
            let t2 = scope.spawn(move || run("B", l2, b2, p2));
            t1.join().unwrap();
            t2.join().unwrap();
        });

        let contents = std::fs::read_to_string(integration_dir.path().join("order.txt")).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "both writers must have run, in serial, with neither losing an update");
    }

    // -- integrate_helper_into (R6.7/R6.8, P0-C, P0-D): both sides of a real
    // conflict are preserved through the git-backed real-delta path, not
    // merely the pure `graph::detect_conflict` function tested elsewhere.
    // `integrate_helper_into` is exercised directly against a plain
    // `tempfile` integration directory (not `session.header.repo_path`),
    // matching production's `ensure_integration_worktree` target without
    // needing a Tauri `AppHandle` in these tests. --

    #[test]
    fn integrate_helper_result_records_a_typed_conflict_when_the_lead_also_changed_the_file() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();

        // The integration worktree already has another change integrated
        // into it -- standing in for a sibling helper's or the lead's own
        // prior edit.
        let integration_dir = tempfile::tempdir().unwrap();
        std::fs::write(integration_dir.path().join("shared.txt"), "lead changed it").unwrap();

        let (helper_dir, base_oid) = commit_helper_worktree(&[("shared.txt", "base")], &[("shared.txt", "helper changed it")]);

        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
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
        integrate_helper_into(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            &session,
            &integration_dir.path().to_string_lossy(),
        );

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

        // The integration worktree never touched this file after the base.
        let integration_dir = tempfile::tempdir().unwrap();

        let (helper_dir, base_oid) =
            commit_helper_worktree(&[("only_helper.txt", "base")], &[("only_helper.txt", "helper wrote this")]);

        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
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
        integrate_helper_into(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            &session,
            &integration_dir.path().to_string_lossy(),
        );

        let session = store::read_session(&root, "sess-1").unwrap();
        let helper = session.executions.iter().find(|e| e.execution_id == "helper-a").unwrap();
        assert_eq!(helper.state, SessionState::Finished, "no conflict means the node stays Finished");
        assert!(helper.conflict.is_none());
        // The whole point of integration: the helper's isolated work must
        // actually land in the integration worktree, not just get marked
        // clean.
        assert_eq!(
            std::fs::read_to_string(integration_dir.path().join("only_helper.txt")).unwrap(),
            "helper wrote this",
            "a clean helper change must be written into the integration worktree"
        );
    }

    #[test]
    fn a_conflicting_file_is_not_written_and_stays_conflicted() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();

        let integration_dir = tempfile::tempdir().unwrap();
        std::fs::write(integration_dir.path().join("shared.txt"), "lead changed it").unwrap();

        let (helper_dir, base_oid) = commit_helper_worktree(&[("shared.txt", "base")], &[("shared.txt", "helper changed it")]);

        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
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
        integrate_helper_into(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            &session,
            &integration_dir.path().to_string_lossy(),
        );

        // The integration worktree's file must be untouched by the
        // conflicting write -- still exactly what was already integrated.
        assert_eq!(
            std::fs::read_to_string(integration_dir.path().join("shared.txt")).unwrap(),
            "lead changed it",
            "a conflicting file must never be overwritten before the user resolves it"
        );
        let session = store::read_session(&root, "sess-1").unwrap();
        let helper = session.executions.iter().find(|e| e.execution_id == "helper-a").unwrap();
        assert_eq!(helper.state, SessionState::NeedsInput);
        assert!(helper.conflict.is_some());
    }

    /// P0-C's core guarantee: integration writes into the DEDICATED
    /// integration worktree, never into `session.header.repo_path` (the
    /// user's own open checkout) -- even when that checkout happens to have
    /// the same repo-relative path present.
    #[test]
    fn integration_never_writes_into_the_sessions_repo_path() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();

        // Stands in for the user's own open checkout -- untouched by
        // anything in this test other than this initial state.
        let users_checkout = tempfile::tempdir().unwrap();
        std::fs::write(users_checkout.path().join("only_helper.txt"), "the user's own untouched file").unwrap();

        let integration_dir = tempfile::tempdir().unwrap();
        let (helper_dir, base_oid) =
            commit_helper_worktree(&[("only_helper.txt", "base")], &[("only_helper.txt", "helper wrote this")]);

        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            s.header.repo_path = users_checkout.path().to_string_lossy().into_owned();
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
        integrate_helper_into(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            &session,
            &integration_dir.path().to_string_lossy(),
        );

        assert_eq!(
            std::fs::read_to_string(users_checkout.path().join("only_helper.txt")).unwrap(),
            "the user's own untouched file",
            "the user's own open checkout must never be touched by integration"
        );
        assert_eq!(
            std::fs::read_to_string(integration_dir.path().join("only_helper.txt")).unwrap(),
            "helper wrote this",
            "the dedicated integration worktree must receive the helper's result"
        );
    }

    /// P0-D: a write failure during integration must never be reported as a
    /// silent success. `apply_operation`'s own outcome is exercised directly
    /// (see `a_write_failure_leaves_the_target_path_untouched_and_reports_a_typed_outcome`
    /// above); this test proves `integrate_helper_into` itself surfaces that
    /// same failure on the helper's own record rather than swallowing it,
    /// and never mistakes an unwritable path for a detected content
    /// conflict.
    #[test]
    fn a_failed_integration_write_is_reported_and_does_not_silently_succeed() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();

        // "blocked" occupies its own path with a DIRECTORY in the
        // integration worktree, so writing the FILE "blocked" there fails
        // deterministically without relying on OS permission APIs.
        let integration_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(integration_dir.path().join("blocked")).unwrap();
        std::fs::write(integration_dir.path().join("blocked").join("occupied.txt"), "taking the slot").unwrap();

        // The helper's own worktree has a clean, ordinary uncommitted edit
        // to a file named "blocked" -- from the helper's point of view this
        // is an unremarkable change; only the integration TARGET makes it
        // unwritable.
        let (helper_dir, base_oid) = commit_helper_worktree(&[("blocked", "base content")], &[("blocked", "helper wants this here")]);

        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
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
        integrate_helper_into(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            &session,
            &integration_dir.path().to_string_lossy(),
        );

        let session = store::read_session(&root, "sess-1").unwrap();
        let helper = session.executions.iter().find(|e| e.execution_id == "helper-a").unwrap();
        // The node is not silently reported as a clean success: its summary
        // names the failure. It is never `NeedsInput` either -- this was not
        // a detected content conflict, so it must not be misreported as one.
        let summary = helper.output_summary.as_deref().unwrap_or_default();
        assert!(
            summary.contains("could not be brought into your working files"),
            "expected a visible failure summary, got {summary:?}"
        );
        assert_ne!(helper.state, SessionState::NeedsInput, "a write failure is not a content conflict");

        // The pre-existing directory-occupied "blocked" path is untouched --
        // no partial/mixed write landed there.
        assert!(integration_dir.path().join("blocked").is_dir());
        assert_eq!(
            std::fs::read_to_string(integration_dir.path().join("blocked").join("occupied.txt")).unwrap(),
            "taking the slot"
        );

        // A graph whose only helper failed to integrate has no lead record
        // in this fixture, so `start_or_check_lead_review` is a no-op here
        // (proven by the `start_or_check_lead_review_*` tests elsewhere) --
        // the point this test proves is narrower and already established
        // above: the failed write is visible on the helper's own record,
        // never silently reported as success.
    }

    // -- resolve_conflict_at (R6.7/R6.8): the chosen resolution must actually
    // land on disk in the lead's checkout, not merely clear the record. --

    fn seed_conflicted_helper(
        root: &SessionStoreRoot,
        locks: &crate::agentdesk::SessionLocks,
        lead_dir: &std::path::Path,
        relative_path: &str,
        base_text: &str,
        helper_text: &str,
        integrated_text: &str,
    ) {
        seed_session(root, "sess-1");
        std::fs::write(lead_dir.join(relative_path), integrated_text).unwrap();
        update_session_at(locks, root, "sess-1", |s| {
            s.header.repo_path = lead_dir.to_string_lossy().into_owned();
            let mut helper = ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::NeedsInput,
                now_rfc3339(),
                None,
                0,
            );
            helper.conflict = Some(IntegrationConflict {
                path: relative_path.to_string(),
                conflicting_with: "lead".into(),
                base_text: base_text.to_string(),
                helper_text: helper_text.to_string(),
                integrated_text: integrated_text.to_string(),
            });
            s.executions.push(helper);
        });
    }

    #[test]
    fn resolving_keep_helper_writes_the_helper_text_into_the_leads_file() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let lead_dir = tempfile::tempdir().unwrap();
        seed_conflicted_helper(&root, &locks, lead_dir.path(), "shared.txt", "base", "helper version", "lead version");

        let outcome = resolve_conflict_at(&locks, &root, "sess-1", "helper-a", ConflictResolution::KeepHelper);
        assert!(matches!(outcome, ResolveConflictOutcome::Resolved { .. }), "{outcome:?}");

        assert_eq!(
            std::fs::read_to_string(lead_dir.path().join("shared.txt")).unwrap(),
            "helper version"
        );
    }

    /// Auditor finding: the conflict is DETECTED in the integration worktree
    /// and the combined result is BUILT from it, so the resolution must land
    /// there. Writing to `header.repo_path` instead put the user's choice
    /// somewhere the reviewed result never reads -- the choice vanished from
    /// the result and a stray write hit the live checkout. Fails against that.
    #[test]
    fn resolving_a_conflict_writes_into_the_integration_worktree_not_the_users_checkout() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let users_checkout = tempfile::tempdir().unwrap();
        let integration_dir = tempfile::tempdir().unwrap();

        seed_conflicted_helper(
            &root,
            &locks,
            users_checkout.path(),
            "shared.txt",
            "base",
            "helper version",
            "lead version",
        );
        // The graph's dedicated integration worktree, holding the conflicted
        // file exactly as integration left it.
        std::fs::write(integration_dir.path().join("shared.txt"), "lead version").unwrap();
        // `seed_conflicted_helper` seeds only the helper; a real graph also has
        // the lead record that owns the integration worktree.
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
            lead.integration_worktree_path = Some(integration_dir.path().to_string_lossy().into_owned());
            s.executions.push(lead);
        });

        let outcome = resolve_conflict_at(&locks, &root, "sess-1", "helper-a", ConflictResolution::KeepHelper);
        assert!(matches!(outcome, ResolveConflictOutcome::Resolved { .. }), "{outcome:?}");

        assert_eq!(
            std::fs::read_to_string(integration_dir.path().join("shared.txt")).unwrap(),
            "helper version",
            "the choice must land where the combined result is built from"
        );
        assert_eq!(
            std::fs::read_to_string(users_checkout.path().join("shared.txt")).unwrap(),
            "lead version",
            "the user's own checkout must not be written to by resolving a conflict"
        );
    }

    #[test]
    fn resolving_keep_integrated_writes_the_integrated_text_into_the_leads_file() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let lead_dir = tempfile::tempdir().unwrap();
        seed_conflicted_helper(&root, &locks, lead_dir.path(), "shared.txt", "base", "helper version", "lead version");

        let outcome = resolve_conflict_at(&locks, &root, "sess-1", "helper-a", ConflictResolution::KeepIntegrated);
        assert!(matches!(outcome, ResolveConflictOutcome::Resolved { .. }), "{outcome:?}");

        assert_eq!(
            std::fs::read_to_string(lead_dir.path().join("shared.txt")).unwrap(),
            "lead version"
        );
    }

    #[test]
    fn resolving_use_merged_writes_the_supplied_text_into_the_leads_file() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let lead_dir = tempfile::tempdir().unwrap();
        seed_conflicted_helper(&root, &locks, lead_dir.path(), "shared.txt", "base", "helper version", "lead version");

        let outcome = resolve_conflict_at(
            &locks,
            &root,
            "sess-1",
            "helper-a",
            ConflictResolution::UseMerged {
                text: "hand-merged result".into(),
            },
        );
        assert!(matches!(outcome, ResolveConflictOutcome::Resolved { .. }), "{outcome:?}");

        assert_eq!(
            std::fs::read_to_string(lead_dir.path().join("shared.txt")).unwrap(),
            "hand-merged result"
        );
    }

    /// A write failure (here: the target directory itself does not exist and
    /// cannot be created because a FILE sits where a parent directory would
    /// need to go) must be reported as a typed `WriteFailed`, and the
    /// conflict must stay on the record -- the user's choice is not silently
    /// discarded, they can try again.
    #[test]
    fn a_failed_apply_reports_a_typed_outcome_and_leaves_the_conflict_in_place() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        let lead_dir = tempfile::tempdir().unwrap();

        // "blocked" is a FILE, not a directory -- writing to
        // "blocked/nested.txt" cannot succeed, simulating a real integration
        // failure (permission denied, path removed, etc.) without relying on
        // OS-specific permission APIs.
        std::fs::write(lead_dir.path().join("blocked"), "not a directory").unwrap();

        seed_session(&root, "sess-1");
        update_session_at(&locks, &root, "sess-1", |s| {
            s.header.repo_path = lead_dir.path().to_string_lossy().into_owned();
            let mut helper = ExecutionRecord::minimal(
                "helper-a".into(),
                s.header.session_id.clone(),
                Some("lead".into()),
                SessionState::NeedsInput,
                now_rfc3339(),
                None,
                0,
            );
            helper.conflict = Some(IntegrationConflict {
                path: "blocked/nested.txt".into(),
                conflicting_with: "lead".into(),
                base_text: "base".into(),
                helper_text: "helper version".into(),
                integrated_text: "lead version".into(),
            });
            s.executions.push(helper);
        });

        let outcome = resolve_conflict_at(&locks, &root, "sess-1", "helper-a", ConflictResolution::KeepHelper);
        match outcome {
            ResolveConflictOutcome::WriteFailed { detail } => assert!(!detail.is_empty()),
            other => panic!("expected WriteFailed, got {other:?}"),
        }

        let session = store::read_session(&root, "sess-1").unwrap();
        let helper = session.executions.iter().find(|e| e.execution_id == "helper-a").unwrap();
        assert!(
            helper.conflict.is_some(),
            "a failed write must leave the conflict in place, not silently drop the user's choice"
        );
        assert_eq!(
            helper.state,
            SessionState::NeedsInput,
            "the node must stay conflicted, not jump to Finished, when the write failed"
        );
    }

    // -- R6.1: a live Plan-mode turn's finished reply becomes a persisted
    // proposal, or a visible refusal -- `finish_plan_mode_execution_at` is
    // the pure "read + decide" half `finish_plan_mode_proposal` (the actual
    // production entry point, called from
    // `commands::airun::route_to_agent_desk`) wraps with the transcript-note
    // side effect. Tested directly here since it needs no live provider.

    /// Seeds a lead `ExecutionRecord` with the given mode/team and a single
    /// assistant message carrying `reply_text` as that execution's own final
    /// content -- exactly what `finish_plan_mode_execution_at` reads back.
    /// The failure this closes: a helper told to make a check pass could stop
    /// without running it, be marked Finished, and have its work folded into
    /// the integration worktree anyway. Now it is marked Failed with the
    /// reason, and the caller skips integration entirely.
    #[test]
    fn a_helper_that_did_not_meet_its_condition_is_not_treated_as_finished() {
        use crate::agentdesk::graph::CompletionCondition;

        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");

        let mut session = store::read_session(&root, "sess-1").unwrap();
        let mut helper = ExecutionRecord::minimal(
            "helper-1".into(),
            "sess-1".into(),
            Some("lead-1".into()),
            SessionState::Finished,
            now_rfc3339(),
            Some(now_rfc3339()),
            2,
        );
        helper.job_title = Some("Fix the parser".into());
        helper.completion = Some(CompletionCondition::ChecksPass {
            command: "cargo test".into(),
        });
        session.executions.push(helper);
        store::write_session(&root, &session).unwrap();
        let session = store::read_session(&root, "sess-1").unwrap();

        // It never ran the check, so the condition is unmet.
        let outcome = enforce_completion_condition(&locks, &root, "sess-1", "helper-1", session);
        assert!(outcome.is_none(), "an unmet condition must stop integration");

        let after = store::read_session(&root, "sess-1").unwrap();
        let helper = after
            .executions
            .iter()
            .find(|e| e.execution_id == "helper-1")
            .unwrap();
        assert_eq!(helper.state, SessionState::Failed);
        let summary = helper.output_summary.clone().unwrap_or_default();
        assert!(summary.contains("never ran that check"), "{summary}");
        // And the chat says so, naming the helper.
        assert!(
            after
                .messages
                .iter()
                .any(|m| m.plain_content.contains("Fix the parser") && m.plain_content.contains("cargo test")),
            "the transcript should explain why"
        );
    }

    /// A helper with no condition, or one that reports its own result, is
    /// untouched -- the common case must not become stricter by accident.
    #[test]
    fn a_helper_with_nothing_to_prove_passes_straight_through() {
        use crate::agentdesk::graph::CompletionCondition;

        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");

        for (id, completion) in [
            ("helper-none", None),
            ("helper-reports", Some(CompletionCondition::ReportsResult)),
        ] {
            let mut session = store::read_session(&root, "sess-1").unwrap();
            let mut helper = ExecutionRecord::minimal(
                id.into(),
                "sess-1".into(),
                Some("lead-1".into()),
                SessionState::Finished,
                now_rfc3339(),
                Some(now_rfc3339()),
                1,
            );
            helper.completion = completion;
            session.executions.push(helper);
            store::write_session(&root, &session).unwrap();
            let session = store::read_session(&root, "sess-1").unwrap();

            assert!(
                enforce_completion_condition(&locks, &root, "sess-1", id, session).is_some(),
                "{id} should carry on to integration"
            );
            let after = store::read_session(&root, "sess-1").unwrap();
            let helper = after.executions.iter().find(|e| e.execution_id == id).unwrap();
            assert_eq!(helper.state, SessionState::Finished, "{id}");
        }
    }

    fn seed_lead_turn(root: &SessionStoreRoot, session_id: &str, mode: &str, team: &str, reply_text: &str) -> String {
        let mut session = store::read_session(root, session_id).unwrap();
        let execution_id = new_id();
        let mut record = ExecutionRecord::minimal(
            execution_id.clone(),
            session_id.to_string(),
            None,
            SessionState::Finished,
            now_rfc3339(),
            Some(now_rfc3339()),
            1,
        );
        record.mode = Some(mode.to_string());
        record.team = Some(team.to_string());
        session.executions.push(record);
        session.messages.push(crate::agentdesk::model::SessionMessage {
            message_id: new_id(),
            segment_id: "seg-1".into(),
            role: crate::agentdesk::model::MessageRole::Assistant,
            timestamp: now_rfc3339(),
            plain_content: reply_text.to_string(),
            rendered_content: None,
            provider: None,
            model: None,
            kind: crate::agentdesk::model::MessageKind::Assistant,
            execution_id: Some(execution_id.clone()),
            sequence: Some(1),
            import: None,
            targets: Vec::new(),
        });
        store::write_session(root, &session).unwrap();
        execution_id
    }

    fn fenced_proposal() -> String {
        "Here's my plan.\n\n```graph-proposal\n{ \"leadSummary\": \"Just going to look\", \"helpers\": [] }\n```\n"
            .to_string()
    }

    /// Finished has one meaning: reviewed work is there to look over. Each
    /// of these used to end as Finished with an explanatory note, which is
    /// exactly the false success an unattended run cannot catch.
    #[test]
    fn a_graph_only_finishes_when_the_review_and_the_result_both_landed() {
        let ok = graph_finish_outcome(Some(SessionState::Finished), true, None);
        assert_eq!(ok.state, SessionState::Finished);
        assert!(ok.note.contains("reviewed"), "{}", ok.note);

        let stopped = graph_finish_outcome(Some(SessionState::Stopped), true, None);
        assert_eq!(stopped.state, SessionState::Stopped);
        assert!(stopped.note.contains("nothing was checked over"), "{}", stopped.note);
        assert!(stopped.note.contains("still here"), "{}", stopped.note);

        let failed = graph_finish_outcome(Some(SessionState::Failed), true, None);
        assert_eq!(failed.state, SessionState::Failed);
        assert!(failed.note.contains("could not finish"), "{}", failed.note);

        let unclear = graph_finish_outcome(None, true, None);
        assert_eq!(unclear.state, SessionState::Failed);

        // A review that went perfectly cannot rescue a result nobody could
        // save: there is nothing on disk to open.
        let unsaved = graph_finish_outcome(
            Some(SessionState::Finished),
            false,
            Some("the disk was full".into()),
        );
        assert_eq!(unsaved.state, SessionState::Failed);
        assert!(unsaved.note.contains("the disk was full"), "{}", unsaved.note);
        assert!(unsaved.note.contains("still here"), "{}", unsaved.note);
    }

    /// A proposal with no helpers has nothing to start. Started anyway it
    /// moved the graph to Working with no helper that could ever become
    /// terminal, so it never completed; Auto mode reached this on its own.
    #[test]
    fn a_team_with_no_helpers_is_refused_at_start() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let manager = crate::state::RepoManager::default();
        let empty = crate::agentdesk::graph::ProposedGraph {
            lead_summary: "nothing to split up".into(),
            helpers: Vec::new(),
            proposed_at: now_rfc3339(),
        };
        // The proposal itself is legitimate: zero helpers is how a lead says
        // it will work alone, and the parser accepts that shape.
        match propose_graph_at(&locks, &root, "sess-1", empty) {
            ProposeGraphOutcome::AwaitingStart { .. } => {}
            other => panic!("expected the proposal to persist, got {other:?}"),
        }
        match start_graph_at(&locks, &root, &manager, "sess-1") {
            StartGraphOutcome::NoHelpersRunSolo { session } => {
                assert_eq!(session.header.state, SessionState::Ready);
                assert!(session.executions.iter().all(|e| e.proposed_graph.is_none()));
            }
            other => panic!("expected NoHelpersRunSolo, got {other:?}"),
        }
        let session = store::read_session(&root, "sess-1").unwrap();
        assert_ne!(
            session.header.state,
            SessionState::Working,
            "a team of nobody must never leave the chat running forever"
        );
        assert_eq!(session.header.state, SessionState::Ready);
    }

    #[test]
    fn an_auto_lead_proposal_is_marked_for_immediate_start() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let exec = seed_lead_turn(&root, "sess-1", "Auto", "Lead", &fenced_proposal());

        let outcome = finish_plan_mode_execution_at(&locks, &root, "sess-1", &exec);
        assert!(matches!(
            outcome,
            PlanProposalCompletion::Proposed {
                auto_start: true,
                ..
            }
        ));
    }

    #[test]
    fn an_ask_mode_execution_is_not_applicable() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let exec = seed_lead_turn(&root, "sess-1", "Ask", "Lead", &fenced_proposal());

        let outcome = finish_plan_mode_execution_at(&locks, &root, "sess-1", &exec);
        assert!(matches!(outcome, PlanProposalCompletion::NotApplicable));
    }

    #[test]
    fn a_plan_mode_solo_execution_is_not_applicable() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let exec = seed_lead_turn(&root, "sess-1", "Plan", "Solo", &fenced_proposal());

        let outcome = finish_plan_mode_execution_at(&locks, &root, "sess-1", &exec);
        assert!(matches!(outcome, PlanProposalCompletion::NotApplicable));
    }

    #[test]
    fn a_helper_execution_is_never_checked_for_a_proposal_even_if_it_looks_like_one() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let mut session = store::read_session(&root, "sess-1").unwrap();
        let helper_id = new_id();
        let mut record = ExecutionRecord::minimal(
            helper_id.clone(),
            "sess-1".into(),
            Some("lead-1".into()),
            SessionState::Finished,
            now_rfc3339(),
            Some(now_rfc3339()),
            1,
        );
        record.mode = Some("Plan".into());
        record.team = Some("Lead".into());
        session.executions.push(record);
        store::write_session(&root, &session).unwrap();

        let outcome = finish_plan_mode_execution_at(&locks, &root, "sess-1", &helper_id);
        assert!(matches!(outcome, PlanProposalCompletion::NotApplicable));
    }

    #[test]
    fn a_plan_mode_lead_reply_with_a_valid_proposal_is_persisted() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let exec = seed_lead_turn(&root, "sess-1", "Plan", "Lead", &fenced_proposal());

        let outcome = finish_plan_mode_execution_at(&locks, &root, "sess-1", &exec);
        match outcome {
            PlanProposalCompletion::Proposed {
                execution_id,
                auto_start: false,
            } => {
                let session = store::read_session(&root, "sess-1").unwrap();
                let proposed = session
                    .executions
                    .iter()
                    .find(|e| e.execution_id == execution_id)
                    .unwrap();
                assert!(proposed.proposed_graph.is_some());
            }
            other => panic!("expected Proposed, got {other:?}"),
        }
    }

    #[test]
    fn a_plan_mode_lead_reply_with_no_fence_is_refused_without_a_note() {
        // No fence means the model chose (or was never able) to propose a
        // graph -- `plain_proposal_refusal` returns `None` for this case
        // specifically because an ordinary Ask-shaped reply is not an error.
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let exec = seed_lead_turn(&root, "sess-1", "Plan", "Lead", "Just an ordinary answer, no plan needed.");

        let outcome = finish_plan_mode_execution_at(&locks, &root, "sess-1", &exec);
        match outcome {
            PlanProposalCompletion::Refused { outcome } => {
                assert!(matches!(
                    outcome,
                    crate::agentdesk::plan_proposal::ProposalOutcome::NoProposalFound
                ));
                assert!(plain_proposal_refusal(&outcome).is_none());
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn a_plan_mode_lead_reply_with_malformed_json_fails_visibly() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let exec = seed_lead_turn(
            &root,
            "sess-1",
            "Plan",
            "Lead",
            "Here's my plan.\n```graph-proposal\n{ not valid json\n```",
        );

        let outcome = finish_plan_mode_execution_at(&locks, &root, "sess-1", &exec);
        match outcome {
            PlanProposalCompletion::Refused { outcome } => {
                let text = plain_proposal_refusal(&outcome).expect("a malformed proposal must produce a visible note");
                assert!(!text.is_empty());
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn finish_plan_mode_proposal_appends_a_visible_note_for_a_refusal() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let exec = seed_lead_turn(
            &root,
            "sess-1",
            "Plan",
            "Lead",
            "Here's my plan.\n```graph-proposal\n{ not valid json\n```",
        );

        finish_plan_mode_proposal(&locks, &root, "sess-1", &exec);

        let session = store::read_session(&root, "sess-1").unwrap();
        assert!(
            session
                .messages
                .iter()
                .any(|m| m.kind == crate::agentdesk::model::MessageKind::System && m.execution_id.is_none()),
            "expected a system note explaining the refusal, transcript: {:?}",
            session.messages
        );
    }

    #[test]
    fn finish_plan_mode_proposal_persists_silently_for_a_valid_proposal() {
        let (_dir, root) = temp_root();
        let locks = crate::agentdesk::SessionLocks::new();
        seed_session(&root, "sess-1");
        let exec = seed_lead_turn(&root, "sess-1", "Plan", "Lead", &fenced_proposal());

        finish_plan_mode_proposal(&locks, &root, "sess-1", &exec);

        let session = store::read_session(&root, "sess-1").unwrap();
        assert!(
            session.executions.iter().any(|e| e.proposed_graph.is_some()),
            "expected a persisted proposal"
        );
        assert!(
            !session
                .messages
                .iter()
                .any(|m| m.kind == crate::agentdesk::model::MessageKind::System),
            "a successful proposal should not also append a refusal note"
        );
    }
}
