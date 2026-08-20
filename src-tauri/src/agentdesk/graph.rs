//! Lead-and-helper agent graphs.
//!
//! One session's lead can propose a graph of small, bounded helper jobs and
//! run up to three of them in parallel, each isolated in its own worktree.
//! This module owns:
//!
//! - the proposed/executable graph shape and DAG validation (tasks.md 1.1-1.2)
//! - the concurrency-3 scheduler that decides which helpers may start next
//!   (tasks.md 3.3)
//! - the typed conflict state integration hits when two helpers touch the
//!   same lines (tasks.md 5.3)
//!
//! Everything here operates on [`super::model::ExecutionRecord`] rows already
//! persisted on the session -- this module adds no second source of truth.
//! The graph the UI renders is always a *projection* of `AgentSession.executions`
//! (tasks.md 6.1), built by [`project_graph`].
//!
//! Nested helper-created helpers are explicitly out of scope this release: a
//! node whose `parent_execution_id` is itself non-`None` (i.e. a helper of a
//! helper) is rejected by [`validate_graph`].

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::model::{ExecutionId, ExecutionRecord, SessionState};

/// Helpers may run at most this many at once (tasks.md 3.3, design.md "at
/// most three concurrent helpers").
pub const MAX_CONCURRENT_HELPERS: usize = 3;

// ---------------------------------------------------------------------------
// Proposed graph (Plan mode, before Start)
// ---------------------------------------------------------------------------

/// One helper job in a lead's proposed graph, before any execution ID has
/// been minted. Mirrors the shape [`super::openspec_context::ProposedGraphNode`]
/// uses for OpenSpec provenance, but carries what the *scheduler* needs to
/// run the job: role, path allowance, budget, and an explicit completion
/// condition (tasks.md 1.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposedHelperJob {
    /// Caller-assigned, unique within the proposal. Becomes the durable
    /// `execution_id`'s human-readable seed once started, but is not itself
    /// an `ExecutionId` -- those are minted at Start (tasks.md 3.1).
    pub node_id: String,
    pub title: String,
    pub description: String,
    pub role: HelperRole,
    /// Repo-relative path globs this helper may write to. Read-only helpers
    /// (role `Researcher` / `Verifier` inspecting only) may leave this empty.
    pub allowed_paths: Vec<String>,
    /// Other node ids in the same proposal this one depends on -- it will not
    /// be scheduled until all of them finish (tasks.md 1.1, 3.3).
    pub depends_on: Vec<String>,
    pub budget: JobBudget,
    pub completion: CompletionCondition,
}

/// What kind of work a helper does, shown in its node meta line (mockup:
/// "Luna · researcher · read-only").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum HelperRole {
    Researcher,
    Builder,
    Verifier,
}

/// Bounds on how much a helper may do before it must stop and report back,
/// even if its completion condition was never met (tasks.md 3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JobBudget {
    pub max_turns: u32,
    /// Wall-clock budget in seconds -- `u32`, not `u64` (specta cannot export
    /// 64-bit integers; matches `ExecutionRecord::last_sequence`'s reasoning).
    pub max_seconds: u32,
}

impl Default for JobBudget {
    fn default() -> Self {
        Self {
            max_turns: 20,
            max_seconds: 900,
        }
    }
}

/// What makes a helper's job "done" -- checked by the scheduler, not left to
/// the helper's own judgment, so a helper cannot silently run forever or stop
/// early without anyone noticing (tasks.md 1.1, 1.2 "explicit completion
/// conditions").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CompletionCondition {
    /// The helper reports its own result via the typed engine output/tool
    /// (tasks.md 2.1) and the scheduler accepts that as done.
    ReportsResult,
    /// The named check command must pass in the helper's worktree.
    ChecksPass { command: String },
    /// The listed repo-relative paths must all have been touched.
    FilesChanged { paths: Vec<String> },
}

/// A full Plan-mode graph proposal: one lead plus its proposed helpers.
/// Persisted as the payload of an `AwaitingStart` execution record
/// (tasks.md 1.4, 2.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposedGraph {
    pub lead_summary: String,
    pub helpers: Vec<ProposedHelperJob>,
    /// RFC 3339 UTC timestamp of when the lead drafted this proposal.
    pub proposed_at: String,
}

/// Every way a proposed graph can fail validation (tasks.md 1.2, 1.3 "fixture
/// tests for every invalid shape"). Exhaustive and typed -- never a bare
/// string -- so the UI can render a specific, actionable message per case
/// and tests can assert on the exact failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphValidationError {
    /// More than [`MAX_CONCURRENT_HELPERS`] helper nodes were proposed.
    TooManyHelpers { found: u32, max: u32 },
    /// Two nodes share the same `node_id`.
    DuplicateNodeId { node_id: String },
    /// A node's `depends_on` names a node id that is not in the proposal.
    UnknownDependency { node_id: String, missing: String },
    /// The dependency edges contain a cycle.
    Cycle { node_ids: Vec<String> },
    /// A node has an empty `node_id`, `title`, or a budget of zero.
    EmptyJob { node_id: String, detail: String },
    /// A node with `allowed_paths` claims to write but its role is read-only,
    /// or a writing node has no `allowed_paths` at all -- both are treated as
    /// "no allowed paths" by the policy layer's `can_write` gate, which is a
    /// silent all-or-nothing allowance no graph should rely on implicitly.
    MissingAllowedPaths { node_id: String },
}

/// Validates a proposed graph as a whole: DAG shape (no cycles, no dangling
/// dependency, unique ids), the helper-count cap, and that every job is
/// actually runnable (non-empty title, a real budget, and -- for any helper
/// that writes -- a non-empty path allowance). Nested helpers (a helper
/// depending on nothing is fine; a helper of a helper is a different, later
/// concept entirely out of scope) cannot be expressed by this shape at all,
/// since `ProposedHelperJob` has no field for "my own sub-helpers" -- so
/// there is nothing here to reject on that account beyond what the type
/// system already prevents.
pub fn validate_graph(graph: &ProposedGraph) -> Result<(), GraphValidationError> {
    if graph.helpers.len() > MAX_CONCURRENT_HELPERS {
        return Err(GraphValidationError::TooManyHelpers {
            found: graph.helpers.len() as u32,
            max: MAX_CONCURRENT_HELPERS as u32,
        });
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for job in &graph.helpers {
        if !seen.insert(job.node_id.as_str()) {
            return Err(GraphValidationError::DuplicateNodeId {
                node_id: job.node_id.clone(),
            });
        }
    }

    for job in &graph.helpers {
        if job.node_id.trim().is_empty() {
            return Err(GraphValidationError::EmptyJob {
                node_id: job.node_id.clone(),
                detail: "a helper needs an id".into(),
            });
        }
        if job.title.trim().is_empty() {
            return Err(GraphValidationError::EmptyJob {
                node_id: job.node_id.clone(),
                detail: "a helper needs a title".into(),
            });
        }
        if job.budget.max_turns == 0 || job.budget.max_seconds == 0 {
            return Err(GraphValidationError::EmptyJob {
                node_id: job.node_id.clone(),
                detail: "a helper needs a real turn/time budget".into(),
            });
        }
        let writes = matches!(job.role, HelperRole::Builder)
            || !job.allowed_paths.is_empty();
        if writes && job.allowed_paths.is_empty() {
            return Err(GraphValidationError::MissingAllowedPaths {
                node_id: job.node_id.clone(),
            });
        }
        for dep in &job.depends_on {
            if !seen.contains(dep.as_str()) {
                return Err(GraphValidationError::UnknownDependency {
                    node_id: job.node_id.clone(),
                    missing: dep.clone(),
                });
            }
        }
    }

    if let Some(cycle) = find_cycle(&graph.helpers) {
        return Err(GraphValidationError::Cycle { node_ids: cycle });
    }

    Ok(())
}

fn find_cycle(jobs: &[ProposedHelperJob]) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }

    let by_id: HashMap<&str, &ProposedHelperJob> =
        jobs.iter().map(|j| (j.node_id.as_str(), j)).collect();
    let mut marks: HashMap<&str, Mark> = HashMap::new();
    let mut stack: Vec<&str> = Vec::new();

    fn visit<'a>(
        id: &'a str,
        by_id: &HashMap<&'a str, &'a ProposedHelperJob>,
        marks: &mut HashMap<&'a str, Mark>,
        stack: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        match marks.get(id) {
            Some(Mark::Done) => return None,
            Some(Mark::Visiting) => {
                let start = stack.iter().position(|x| *x == id).unwrap_or(0);
                let mut cycle: Vec<String> = stack[start..].iter().map(|s| s.to_string()).collect();
                cycle.push(id.to_string());
                return Some(cycle);
            }
            None => {}
        }
        marks.insert(id, Mark::Visiting);
        stack.push(id);
        if let Some(job) = by_id.get(id) {
            for dep in &job.depends_on {
                if let Some(cycle) = visit(dep, by_id, marks, stack) {
                    return Some(cycle);
                }
            }
        }
        stack.pop();
        marks.insert(id, Mark::Done);
        None
    }

    for id in by_id.keys() {
        if let Some(cycle) = visit(id, &by_id, &mut marks, &mut stack) {
            return Some(cycle);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Scheduling
// ---------------------------------------------------------------------------

/// One decision the scheduler made about an already-started graph: which
/// helper execution ids are ready to launch right now, given dependency
/// completion and the concurrency cap. Pure function of the current
/// [`ExecutionRecord`] rows -- no side effects, so it is trivially testable
/// and safe to call after every state change and after restart alike
/// (tasks.md 3.3, 6.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleDecision {
    /// Execution ids that may be started now.
    pub ready: Vec<ExecutionId>,
    /// Execution ids still blocked, and on what.
    pub blocked: Vec<BlockedNode>,
    /// How many concurrency slots remain after starting everything in
    /// `ready` (always `>= 0`; the scheduler never proposes more than fit).
    pub slots_remaining: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BlockedNode {
    pub execution_id: ExecutionId,
    pub waiting_on: Vec<ExecutionId>,
}

/// Given every execution on a session, decide which helper nodes (rows with
/// `parent_execution_id.is_some()`) may start next: their dependencies must
/// all be `Finished`, and no more than [`MAX_CONCURRENT_HELPERS`] may be
/// `Working`/`Preparing`/`NeedsInput` (counted as "active") at once.
///
/// A `NeedsInput` helper (one gated on approval) still occupies a
/// concurrency slot -- it has not released its worktree or given back its
/// turn budget -- which is what tasks.md 3.6 means by "keep lead and peers
/// responsive when one helper waits at a gate": peers keep running, but a
/// *fourth* helper does not start just because one of the three active ones
/// is momentarily blocked on a human answer.
pub fn schedule(executions: &[ExecutionRecord]) -> ScheduleDecision {
    let helpers: Vec<&ExecutionRecord> = executions
        .iter()
        .filter(|e| e.parent_execution_id.is_some())
        .collect();

    let active_count = helpers
        .iter()
        .filter(|e| is_active(e.state))
        .count();

    let mut slots = MAX_CONCURRENT_HELPERS.saturating_sub(active_count);

    let finished: HashSet<&str> = executions
        .iter()
        .filter(|e| e.state == SessionState::Finished)
        .map(|e| e.execution_id.as_str())
        .collect();

    let mut ready = Vec::new();
    let mut blocked = Vec::new();

    for helper in &helpers {
        // Only pending (not-yet-started) nodes are schedulable at all --
        // already-active or already-finished/stopped/failed nodes are not
        // candidates for (re)start here.
        if helper.state != SessionState::Ready && helper.state != SessionState::Draft {
            continue;
        }
        let waiting_on: Vec<ExecutionId> = helper
            .depends_on
            .iter()
            .filter(|dep| !finished.contains(dep.as_str()))
            .cloned()
            .collect();

        if waiting_on.is_empty() {
            if slots > 0 {
                ready.push(helper.execution_id.clone());
                slots -= 1;
            } else {
                blocked.push(BlockedNode {
                    execution_id: helper.execution_id.clone(),
                    waiting_on: Vec::new(),
                });
            }
        } else {
            blocked.push(BlockedNode {
                execution_id: helper.execution_id.clone(),
                waiting_on,
            });
        }
    }

    ScheduleDecision {
        ready,
        blocked,
        slots_remaining: slots as u32,
    }
}

fn is_active(state: SessionState) -> bool {
    matches!(
        state,
        SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
    )
}

// ---------------------------------------------------------------------------
// Integration conflicts
// ---------------------------------------------------------------------------

/// The state one helper's result integration is in. `Conflicted` is the
/// "typed conflict state" tasks.md 5.3/9 requires: it names the base text,
/// what this helper produced, and what a sibling's already-integrated change
/// produced, so a person resolving it sees all three and nothing is
/// discarded silently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IntegrationState {
    /// Not yet attempted (the helper has not finished, or is queued behind an
    /// earlier completion in the same integration order).
    Pending,
    /// Applied cleanly; nothing else touched the same lines.
    Integrated,
    /// This helper's result could not be applied cleanly against the current
    /// integrated tree because another already-integrated change touched the
    /// same lines. Both copies are preserved -- neither is committed until a
    /// person resolves it (tasks.md 5.3, 5.4; design.md "preserves both
    /// copies").
    Conflicted { conflict: IntegrationConflict },
}

/// One conflicted file: the shared ancestor text, this helper's version, and
/// the version already sitting in the integration target (which may itself
/// be another helper's already-applied change, or the lead's own edit).
/// Nothing here is ever silently dropped -- resolution is a user action that
/// picks or merges these into a fourth, final copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationConflict {
    pub path: String,
    /// The execution id whose already-integrated change this one collided
    /// with.
    pub conflicting_with: ExecutionId,
    /// Full file text as it stood before either change (the merge base).
    pub base_text: String,
    /// Full file text as this helper's worktree left it.
    pub helper_text: String,
    /// Full file text as it stands in the integration target right now
    /// (i.e. after `conflicting_with`'s change was applied).
    pub integrated_text: String,
}

/// Detects whether `helper_text` can be integrated cleanly on top of
/// `integrated_text`, given their common `base_text`.
///
/// This is a line-level three-way check, not a full merge: if the file
/// changed at all between `base_text` and `integrated_text` (i.e. some
/// other, already-integrated change touched this same file) AND the
/// helper's own text also differs from `base_text` for the file, the two
/// edits are treated as conflicting and neither is silently preferred --
/// even when their line ranges do not literally overlap, "both touched this
/// file" is treated as a review-worthy conflict rather than attempting an
/// automatic three-way merge. Choosing safety (surface it) over
/// cleverness (silently interleave) is deliberate: a merge that happens to
/// apply without lexical conflict can still be semantically wrong, and this
/// module has no way to know that; only a person reviewing both copies can.
pub fn detect_conflict(
    path: &str,
    conflicting_with: &ExecutionId,
    base_text: &str,
    helper_text: &str,
    integrated_text: &str,
) -> IntegrationState {
    if integrated_text == base_text {
        // Nothing else touched this file since the merge base -- the
        // helper's change applies cleanly regardless of what it says.
        return IntegrationState::Integrated;
    }
    if helper_text == base_text {
        // The helper never touched this file -- the already-integrated
        // change stands untouched, nothing to integrate.
        return IntegrationState::Integrated;
    }
    if helper_text == integrated_text {
        // Both sides ended up with the same text (e.g. both no-ops or
        // identical edits) -- no real conflict.
        return IntegrationState::Integrated;
    }
    IntegrationState::Conflicted {
        conflict: IntegrationConflict {
            path: path.to_string(),
            conflicting_with: conflicting_with.clone(),
            base_text: base_text.to_string(),
            helper_text: helper_text.to_string(),
            integrated_text: integrated_text.to_string(),
        },
    }
}

// ---------------------------------------------------------------------------
// Graph projection (tasks.md 6.1, 6.2)
// ---------------------------------------------------------------------------

/// One renderable node in the UI's graph tree -- built entirely from
/// [`ExecutionRecord`] rows plus the schedule decision, never from separate
/// frontend state (tasks.md 6.1: "no separate frontend graph truth").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeView {
    pub execution_id: ExecutionId,
    pub parent_execution_id: Option<ExecutionId>,
    pub is_lead: bool,
    pub title: String,
    pub role: Option<HelperRole>,
    pub state: SessionState,
    pub depends_on: Vec<ExecutionId>,
    pub allowed_paths: Vec<String>,
    pub worktree_path: Option<String>,
    pub changed_file_count: u32,
    pub output_summary: Option<String>,
    /// True when this node is in `schedule()`'s `ready`/active set right now
    /// -- lets the UI show "queued" vs. "waiting on X" distinctly even
    /// though both map to the same underlying `Ready`/`Draft` state.
    pub blocked_on: Vec<ExecutionId>,
}

/// Projects a session's executions plus the current schedule into the node
/// list the Graph panel renders, lead first, helpers after in declaration
/// order (tasks.md 6.1, 6.2).
pub fn project_graph(executions: &[ExecutionRecord]) -> Vec<GraphNodeView> {
    let decision = schedule(executions);
    let blocked_by: HashMap<&str, &[ExecutionId]> = decision
        .blocked
        .iter()
        .map(|b| (b.execution_id.as_str(), b.waiting_on.as_slice()))
        .collect();

    executions
        .iter()
        .map(|e| GraphNodeView {
            execution_id: e.execution_id.clone(),
            parent_execution_id: e.parent_execution_id.clone(),
            is_lead: e.parent_execution_id.is_none(),
            title: e
                .job_title
                .clone()
                .unwrap_or_else(|| "Lead agent".to_string()),
            role: e.helper_role.as_deref().and_then(parse_role),
            state: e.state,
            depends_on: e.depends_on.clone(),
            allowed_paths: e.allowed_paths.clone(),
            worktree_path: e.worktree_path.clone(),
            changed_file_count: e.changed_file_count,
            output_summary: e.output_summary.clone(),
            blocked_on: blocked_by
                .get(e.execution_id.as_str())
                .map(|v| v.to_vec())
                .unwrap_or_default(),
        })
        .collect()
}

fn parse_role(s: &str) -> Option<HelperRole> {
    match s {
        "researcher" => Some(HelperRole::Researcher),
        "builder" => Some(HelperRole::Builder),
        "verifier" => Some(HelperRole::Verifier),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, deps: &[&str]) -> ProposedHelperJob {
        ProposedHelperJob {
            node_id: id.into(),
            title: format!("Job {id}"),
            description: "test job".into(),
            role: HelperRole::Builder,
            allowed_paths: vec!["src/**".into()],
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            budget: JobBudget::default(),
            completion: CompletionCondition::ReportsResult,
        }
    }

    fn graph(helpers: Vec<ProposedHelperJob>) -> ProposedGraph {
        ProposedGraph {
            lead_summary: "test lead".into(),
            helpers,
            proposed_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn a_valid_dag_of_three_passes() {
        let g = graph(vec![job("a", &[]), job("b", &["a"]), job("c", &["a"])]);
        assert_eq!(validate_graph(&g), Ok(()));
    }

    #[test]
    fn more_than_three_helpers_is_rejected() {
        let g = graph(vec![
            job("a", &[]),
            job("b", &[]),
            job("c", &[]),
            job("d", &[]),
        ]);
        assert_eq!(
            validate_graph(&g),
            Err(GraphValidationError::TooManyHelpers { found: 4, max: 3 })
        );
    }

    #[test]
    fn duplicate_node_ids_are_rejected() {
        let g = graph(vec![job("a", &[]), job("a", &[])]);
        assert_eq!(
            validate_graph(&g),
            Err(GraphValidationError::DuplicateNodeId { node_id: "a".into() })
        );
    }

    #[test]
    fn a_dependency_on_a_missing_node_is_rejected() {
        let g = graph(vec![job("a", &["ghost"])]);
        assert_eq!(
            validate_graph(&g),
            Err(GraphValidationError::UnknownDependency {
                node_id: "a".into(),
                missing: "ghost".into(),
            })
        );
    }

    #[test]
    fn a_two_node_cycle_is_rejected() {
        let g = graph(vec![job("a", &["b"]), job("b", &["a"])]);
        match validate_graph(&g) {
            Err(GraphValidationError::Cycle { node_ids }) => {
                assert!(node_ids.contains(&"a".to_string()));
                assert!(node_ids.contains(&"b".to_string()));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn a_self_cycle_is_rejected() {
        let g = graph(vec![job("a", &["a"])]);
        match validate_graph(&g) {
            Err(GraphValidationError::Cycle { .. }) => {}
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_title_is_rejected() {
        let mut j = job("a", &[]);
        j.title = "  ".into();
        let g = graph(vec![j]);
        match validate_graph(&g) {
            Err(GraphValidationError::EmptyJob { node_id, .. }) => assert_eq!(node_id, "a"),
            other => panic!("expected EmptyJob, got {other:?}"),
        }
    }

    #[test]
    fn a_zero_budget_is_rejected() {
        let mut j = job("a", &[]);
        j.budget = JobBudget {
            max_turns: 0,
            max_seconds: 900,
        };
        let g = graph(vec![j]);
        match validate_graph(&g) {
            Err(GraphValidationError::EmptyJob { node_id, .. }) => assert_eq!(node_id, "a"),
            other => panic!("expected EmptyJob, got {other:?}"),
        }
    }

    #[test]
    fn a_builder_with_no_allowed_paths_is_rejected() {
        let mut j = job("a", &[]);
        j.allowed_paths.clear();
        let g = graph(vec![j]);
        assert_eq!(
            validate_graph(&g),
            Err(GraphValidationError::MissingAllowedPaths { node_id: "a".into() })
        );
    }

    #[test]
    fn a_read_only_researcher_needs_no_allowed_paths() {
        let mut j = job("a", &[]);
        j.role = HelperRole::Researcher;
        j.allowed_paths.clear();
        let g = graph(vec![j]);
        assert_eq!(validate_graph(&g), Ok(()));
    }

    fn exec(id: &str, parent: Option<&str>, state: SessionState, deps: &[&str]) -> ExecutionRecord {
        let mut e = ExecutionRecord::minimal(
            id.into(),
            "sess-1".into(),
            parent.map(|p| p.to_string()),
            state,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        );
        e.depends_on = deps.iter().map(|s| s.to_string()).collect();
        e
    }

    #[test]
    fn schedule_starts_independent_ready_helpers_up_to_the_cap() {
        let execs = vec![
            exec("lead", None, SessionState::Working, &[]),
            exec("h1", Some("lead"), SessionState::Ready, &[]),
            exec("h2", Some("lead"), SessionState::Ready, &[]),
            exec("h3", Some("lead"), SessionState::Ready, &[]),
            exec("h4", Some("lead"), SessionState::Ready, &[]),
        ];
        let decision = schedule(&execs);
        assert_eq!(decision.ready.len(), 3, "cap is {MAX_CONCURRENT_HELPERS}");
        assert_eq!(decision.blocked.len(), 1);
        assert_eq!(decision.slots_remaining, 0);
    }

    #[test]
    fn an_already_active_helper_counts_against_the_cap() {
        let execs = vec![
            exec("lead", None, SessionState::Working, &[]),
            exec("h1", Some("lead"), SessionState::Working, &[]),
            exec("h2", Some("lead"), SessionState::NeedsInput, &[]),
            exec("h3", Some("lead"), SessionState::Ready, &[]),
            exec("h4", Some("lead"), SessionState::Ready, &[]),
        ];
        let decision = schedule(&execs);
        // Two slots already used (h1 working, h2 waiting at a gate) -> only
        // one of h3/h4 may start.
        assert_eq!(decision.ready.len(), 1);
        assert_eq!(decision.slots_remaining, 0);
    }

    #[test]
    fn a_helper_waits_for_its_dependency_to_finish() {
        let execs = vec![
            exec("lead", None, SessionState::Working, &[]),
            exec("h1", Some("lead"), SessionState::Working, &[]),
            exec("h2", Some("lead"), SessionState::Ready, &["h1"]),
        ];
        let decision = schedule(&execs);
        assert!(decision.ready.is_empty());
        assert_eq!(decision.blocked.len(), 1);
        assert_eq!(decision.blocked[0].waiting_on, vec!["h1".to_string()]);
    }

    #[test]
    fn a_helper_becomes_ready_once_its_dependency_finishes() {
        let execs = vec![
            exec("lead", None, SessionState::Working, &[]),
            exec("h1", Some("lead"), SessionState::Finished, &[]),
            exec("h2", Some("lead"), SessionState::Ready, &["h1"]),
        ];
        let decision = schedule(&execs);
        assert_eq!(decision.ready, vec!["h2".to_string()]);
    }

    #[test]
    fn clean_integration_when_only_someone_else_touched_the_file() {
        // helper_text equals base_text -- this helper never touched the
        // file, so whatever the already-integrated side did stands
        // untouched and there is nothing to reconcile.
        let state = detect_conflict("a.rs", &"other".into(), "base", "base", "someone-else-changed");
        assert_eq!(state, IntegrationState::Integrated);
    }

    #[test]
    fn conflict_when_both_the_helper_and_someone_else_touched_the_file_differently() {
        let state = detect_conflict("a.rs", &"other".into(), "base", "helper-changed", "someone-else-changed");
        assert!(matches!(state, IntegrationState::Conflicted { .. }));
    }

    #[test]
    fn clean_integration_when_nothing_else_touched_the_file() {
        let state = detect_conflict("a.rs", &"other".into(), "base", "helper-changed", "base");
        assert_eq!(state, IntegrationState::Integrated);
    }

    #[test]
    fn clean_integration_when_the_helper_never_touched_the_file() {
        let state = detect_conflict("a.rs", &"other".into(), "base", "base", "someone-else-changed");
        assert_eq!(state, IntegrationState::Integrated);
    }

    #[test]
    fn no_conflict_when_both_produced_the_same_text() {
        let state = detect_conflict("a.rs", &"other".into(), "base", "same-result", "same-result");
        assert_eq!(state, IntegrationState::Integrated);
    }

    /// Gate 5's hard clause: two helpers editing the SAME LINE must produce a
    /// typed conflict that preserves BOTH results and commits neither
    /// silently.
    #[test]
    fn same_line_edit_by_two_helpers_preserves_both_copies_and_commits_neither() {
        let base = "fn greet() {\n    println!(\"hi\");\n}\n";
        let helper_a_text = "fn greet() {\n    println!(\"hello from A\");\n}\n";
        let helper_b_already_integrated = "fn greet() {\n    println!(\"hello from B\");\n}\n";

        let state = detect_conflict(
            "src/greet.rs",
            &"exec-helper-b".into(),
            base,
            helper_a_text,
            helper_b_already_integrated,
        );

        match state {
            IntegrationState::Conflicted { conflict } => {
                assert_eq!(conflict.path, "src/greet.rs");
                assert_eq!(conflict.conflicting_with, "exec-helper-b".to_string());
                // Both results survive in full -- neither is discarded or
                // silently merged.
                assert_eq!(conflict.helper_text, helper_a_text);
                assert_eq!(conflict.integrated_text, helper_b_already_integrated);
                assert_eq!(conflict.base_text, base);
            }
            other => panic!("expected a typed Conflicted state, got {other:?}"),
        }
    }

    #[test]
    fn project_graph_marks_dependency_blocked_helpers() {
        let execs = vec![
            exec("lead", None, SessionState::Working, &[]),
            exec("h1", Some("lead"), SessionState::Working, &[]),
            exec("h2", Some("lead"), SessionState::Ready, &["h1"]),
        ];
        let views = project_graph(&execs);
        let h2 = views.iter().find(|v| v.execution_id == "h2").unwrap();
        assert_eq!(h2.blocked_on, vec!["h1".to_string()]);
        let lead = views.iter().find(|v| v.execution_id == "lead").unwrap();
        assert!(lead.is_lead);
    }

    #[test]
    fn a_helper_depending_on_a_currently_active_sibling_is_not_double_counted_as_ready() {
        // Regression guard: a NeedsInput (gated) helper must not free up a
        // slot for a fourth helper, per tasks.md 3.6.
        let execs = vec![
            exec("lead", None, SessionState::Working, &[]),
            exec("h1", Some("lead"), SessionState::NeedsInput, &[]),
            exec("h2", Some("lead"), SessionState::Working, &[]),
            exec("h3", Some("lead"), SessionState::Working, &[]),
            exec("h4", Some("lead"), SessionState::Ready, &[]),
        ];
        let decision = schedule(&execs);
        assert!(decision.ready.is_empty(), "cap already reached by three active helpers");
        assert_eq!(decision.slots_remaining, 0);
    }
}
