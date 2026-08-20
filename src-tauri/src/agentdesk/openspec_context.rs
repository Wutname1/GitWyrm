//! OpenSpec as a first-class Agent Desk source: the context a lead agent
//! reads to "understand the source and the OpenSpec plan" (package
//! `agent-desk-openspec-workflows`), plus the compatibility mapping that lets
//! the current Spec Desk change/task selection resolve to a durable session.
//!
//! Everything here is a read-only projection over `crate::openspec::parse`
//! and `crate::openspec::history`. It never writes a file -- task/spec edits
//! stay routed through `crate::openspec::write` (see
//! `commands::agent_desk::agent_session_apply_openspec_task_completion` in
//! `commands/agent_desk.rs`, which is the only place this package's checkbox
//! writes happen). OpenSpec files remain authoritative
//! (`agent-desk-openspec-workflows/design.md`): a session's cached
//! [`crate::agentdesk::model::SourceSnapshot`] is provenance of what the user
//! clicked, never a second source of truth for task/spec state.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::openspec::{self, history, parse};

/// A file-backed source for an OpenSpec change or task can end up in one of
/// four honest states. Distinguishing them (rather than collapsing to a
/// single "not found") is what tasks 4.5/7 ask for: an archived change has a
/// real next action (open the archive), a deleted one does not, and a moved
/// one (folder renamed) is worth telling the user about rather than silently
/// treating as deleted.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenSpecChangeStatus {
    /// The change folder exists under `openspec/changes/<id>/` exactly as the
    /// session's source names it.
    Active { change: parse::SpecChange },
    /// The change is no longer active but was found under
    /// `openspec/changes/archive/<id>/` -- normal end state for finished
    /// work, not a fault. The next action is "open the archived change", not
    /// "recreate it".
    Archived { change: parse::SpecChange },
    /// Neither location has this id, but a change with the *same title* (from
    /// the session's cached snapshot) exists elsewhere active or archived --
    /// most likely the folder was renamed. Reported as its own case rather
    /// than `Deleted` so the UI can offer "this looks like it moved to
    /// `<newId>`" instead of a dead end.
    Moved { likely_new_id: String, archived: bool },
    /// Neither location has this id, and nothing with a matching title turned
    /// up either. The repository may not have this change any more, or the
    /// repository itself is not open (see `repo_reachable` on
    /// [`OpenSpecSourceStatus`], which callers should check first).
    Deleted,
}

/// Top-level status for an `OpenSpecChange`/`OpenSpecTask` session source,
/// combining "is the repository even open" with the change lookup above --
/// the file lookup is meaningless if the repository itself is not reachable,
/// so this is checked and reported first.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenSpecSourceStatus {
    pub repo_reachable: bool,
    pub change: OpenSpecChangeStatus,
}

/// Looks up `change_id` under `openspec_dir`, honestly distinguishing
/// active/archived/moved/deleted (tasks 4.5, 7.1-7.2 in build-order.md's
/// numbering; tasks.md 4.5 and section covering archived/deleted/moved).
///
/// `snapshot_title` is the session's cached [`SourceSnapshot::title`], used
/// only for the `Moved` heuristic below -- never to decide `Active` vs
/// `Archived`, which is always a direct filesystem check.
pub fn resolve_change_status(openspec_dir: &Path, change_id: &str, snapshot_title: &str) -> OpenSpecChangeStatus {
    let active_dir = openspec_dir.join("changes").join(change_id);
    if let Some(change) = parse::parse_change_dir(&active_dir) {
        return OpenSpecChangeStatus::Active { change };
    }

    let archived_dir = openspec_dir.join("changes").join("archive").join(change_id);
    if let Some(change) = parse::parse_change_dir(&archived_dir) {
        return OpenSpecChangeStatus::Archived { change };
    }

    // Not at either expected path under its own id. Look for a change (active
    // or archived) whose title matches what the session captured at launch --
    // a renamed folder keeps its proposal.md title unless that was edited
    // too, so this is a real (if imperfect) signal, not a guess from nothing.
    if !snapshot_title.trim().is_empty() {
        for change in parse::parse_changes_dir(openspec_dir) {
            if change.id != change_id && titles_match(&change.title, snapshot_title) {
                return OpenSpecChangeStatus::Moved {
                    likely_new_id: change.id,
                    archived: false,
                };
            }
        }
        for change in parse::archived_summaries(&openspec_dir.join("changes").join("archive")) {
            if change.id != change_id && titles_match(&change.title, snapshot_title) {
                return OpenSpecChangeStatus::Moved {
                    likely_new_id: change.id,
                    archived: true,
                };
            }
        }
    }

    OpenSpecChangeStatus::Deleted
}

fn titles_match(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && a.trim().eq_ignore_ascii_case(b.trim())
}

// -- Context builder (tasks.md section 2) --

/// The context a lead agent reads to "understand the source and the OpenSpec
/// plan" (this package's stated purpose). Built from proposal, design, every
/// delta, tasks, and progress -- tasks.md 2.1.
///
/// Every optional document records its own honest absence (tasks.md 2.2)
/// rather than the whole context failing to build: a change with no
/// design.md still produces a usable context, it just says so.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenSpecSourceContext {
    pub change_id: String,
    pub title: String,
    pub proposal: parse::Proposal,
    /// `None` when `design.md` does not exist for this change -- distinct
    /// from `Some(String::new())`, which would mean the file exists but is
    /// empty.
    pub design: Option<String>,
    pub deltas: Vec<parse::SpecDelta>,
    pub tasks: Vec<parse::SpecTask>,
    pub progress: parse::SpecProgress,
    /// Populated only when this context was built for one exact task
    /// (`OpenSpecTask` source) rather than the whole change
    /// (`OpenSpecChange` source). Carries the task's *current* parsed index
    /// and text, which can differ from the session's cached
    /// `SourceSnapshot`/`OpenSpecTask::task_text` if the file changed after
    /// launch -- see [`TargetTaskContext`] for how that divergence is
    /// surfaced (tasks.md 2.3, 2.4).
    pub target_task: Option<TargetTaskContext>,
    /// Commits touching this change's folder, newest first. Empty (not an
    /// error) for a change never committed.
    pub history: Vec<history::SpecHistoryEntry>,
    /// Set when this change also has a linked branch by naming convention
    /// (`<change_id>` as a branch name) -- best-effort, never invented: only
    /// populated when the caller actually found one (see
    /// `context_for_active_change`/`context_for_task`, which pass this
    /// through from the branch lookup already used elsewhere in the app).
    pub branch_link: Option<String>,
    /// Plain-language notes about anything that could not be parsed,
    /// forwarded from [`parse::SpecChange::notes`].
    pub notes: Vec<String>,
}

/// The exact task a session targets, plus whether the live file still agrees
/// with what the session captured at launch (tasks.md 2.4: "mark launch-vs-
/// live differences").
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetTaskContext {
    /// The task's current index in the freshly parsed file. `None` when no
    /// task at the session's original index still looks like the same task
    /// (tasks.md 2.3: "even when it is not the next open task"; this field is
    /// what "identity preserved" means concretely -- see
    /// [`locate_target_task`]).
    pub current_index: Option<u32>,
    pub current_text: Option<String>,
    pub current_done: Option<bool>,
    /// The index/text the session was launched with -- always present,
    /// unlike the `current_*` fields, because this is provenance the session
    /// itself carries regardless of what the live file says now.
    pub launched_index: u32,
    pub launched_text: String,
    /// True when the file changed under this task since launch: the text at
    /// `launched_index` no longer matches `launched_text`, or the task is
    /// gone entirely. Drives the "stale, refresh or accept" affordance
    /// (tasks.md 2.4, and design.md's staleness rule for plan graphs).
    pub diverged: bool,
}

/// Finds the task this session was launched against inside a freshly parsed
/// task list, preserving exact task identity (tasks.md 1.2, 2.3, spec
/// scenario "Non-next task") rather than falling back to "whatever is at that
/// index now" or "the next open task".
///
/// Matching order:
/// 1. Same index AND same text -- the common case, nothing changed.
/// 2. Same text at a different index -- tasks were inserted/removed above it;
///    identity follows the text, not the position, exactly like
///    `write::toggle_task_line`'s own guard against writing to a line that
///    moved.
/// 3. Neither matches -- the task this session names cannot be found. Report
///    the divergence rather than guessing a substitute (never silently
///    retarget to a different task, and never silently fall back to "the
///    next open task", which would defeat the entire "non-next task" point
///    of this source kind).
pub fn locate_target_task(tasks: &[parse::SpecTask], launched_index: u32, launched_text: &str) -> TargetTaskContext {
    let by_index = tasks.iter().find(|t| t.index == launched_index);
    if let Some(t) = by_index {
        if t.text == launched_text {
            return TargetTaskContext {
                current_index: Some(t.index),
                current_text: Some(t.text.clone()),
                current_done: Some(t.done),
                launched_index,
                launched_text: launched_text.to_string(),
                diverged: false,
            };
        }
    }

    if let Some(t) = tasks.iter().find(|t| t.text == launched_text) {
        return TargetTaskContext {
            current_index: Some(t.index),
            current_text: Some(t.text.clone()),
            current_done: Some(t.done),
            launched_index,
            launched_text: launched_text.to_string(),
            // The text still exists but moved lines -- worth flagging even
            // though the task itself is unambiguously the same one, since the
            // file changed underneath the session in a way the user may want
            // to see (something was inserted/reordered above it).
            diverged: true,
        };
    }

    TargetTaskContext {
        current_index: None,
        current_text: None,
        current_done: None,
        launched_index,
        launched_text: launched_text.to_string(),
        diverged: true,
    }
}

/// Builds the full context for an `OpenSpecChange` source: no single target
/// task, the whole change's proposal/design/deltas/tasks/progress.
pub fn context_for_change(
    repo_root: &Path,
    change: &parse::SpecChange,
    branch_link: Option<String>,
) -> Option<OpenSpecSourceContext> {
    let dir = openspec::openspec_dir(repo_root)?;
    let design = read_design(&dir, &change.id, change.has_design);
    let hist = history::change_history(repo_root, &change.id, 50).unwrap_or_default();

    Some(OpenSpecSourceContext {
        change_id: change.id.clone(),
        title: change.title.clone(),
        proposal: change.proposal.clone(),
        design,
        deltas: change.deltas.clone(),
        tasks: change.tasks.clone(),
        progress: change.progress,
        target_task: None,
        history: hist,
        branch_link,
        notes: change.notes.clone(),
    })
}

/// Builds the full context for an `OpenSpecTask` source: everything
/// `context_for_change` builds, plus the exact target task located by
/// identity (tasks.md 2.3), even when the file changed since launch.
pub fn context_for_task(
    repo_root: &Path,
    change: &parse::SpecChange,
    launched_index: u32,
    launched_text: &str,
    branch_link: Option<String>,
) -> Option<OpenSpecSourceContext> {
    let mut ctx = context_for_change(repo_root, change, branch_link)?;
    ctx.target_task = Some(locate_target_task(&change.tasks, launched_index, launched_text));
    Some(ctx)
}

fn read_design(openspec_dir: &Path, change_id: &str, has_design: bool) -> Option<String> {
    if !has_design {
        return None;
    }
    std::fs::read_to_string(openspec_dir.join("changes").join(change_id).join("design.md")).ok()
}

// -- Plan-mode graph draft schema (tasks.md section 3) --

/// One proposed node in a Plan-mode graph draft, tied back to the OpenSpec
/// ids that motivated it (tasks.md 3.1, design.md: "Plan graph nodes may
/// reference requirement IDs/scenarios and task indices. References are
/// provenance, not a second dependency language.").
///
/// This is a draft schema only -- scheduling, execution, and the AwaitingStart
/// state machine belong to the `agent-desk-agent-graphs` package
/// (build-order.md section 5); this package owns the shape of the reference,
/// not what runs it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposedGraphNode {
    /// Caller-assigned, unique within the draft. Not a durable execution ID
    /// -- those are minted only once a node actually starts running.
    pub node_id: String,
    pub label: String,
    /// What in the OpenSpec change caused this node to be proposed. Optional
    /// -- a node can be plain scaffolding (e.g. "wire up tests") with nothing
    /// in the spec text to point at.
    pub source_ref: Option<OpenSpecNodeRef>,
    /// Other node ids in the same draft this one depends on. Validated for
    /// cycles by [`validate_draft_acyclic`].
    pub depends_on: Vec<String>,
}

/// A reference from a graph node back to the OpenSpec text that motivated it.
/// Exhaustive so a node's provenance is always one specific, renderable kind
/// -- never a loose string the UI has to guess how to link.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenSpecNodeRef {
    /// One task line, by its parsed index -- matches
    /// `MessageTarget::OpenSpecTask`'s own `task_index` field so the same
    /// identity concept is used everywhere a task is referenced.
    Task { change_id: String, task_index: u32 },
    /// One requirement inside one delta file, by capability and requirement
    /// name (`SpecRequirement::name`) -- there is no numeric requirement id
    /// in the OpenSpec file format, so the name is the identity, matching how
    /// `SpecRequirement` itself is keyed.
    Requirement {
        change_id: String,
        capability: String,
        requirement_name: String,
    },
    /// One scenario within a requirement, by its name (`(name, body)` in
    /// `SpecRequirement::scenarios`).
    Scenario {
        change_id: String,
        capability: String,
        requirement_name: String,
        scenario_name: String,
    },
}

/// A full Plan-mode graph draft: every proposed node plus the change it was
/// drafted against and when. Persisted as an execution record in
/// `AwaitingStart` by the `agent-desk-agent-graphs` package (tasks.md 3.2);
/// this type is the payload that record carries.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposedGraphDraft {
    pub change_id: String,
    pub nodes: Vec<ProposedGraphNode>,
    /// RFC 3339 UTC timestamp of when the draft was built, used by
    /// [`is_draft_stale`] to compare against the change's current `updated`
    /// mtime.
    pub drafted_at: String,
    /// The change's `SpecChange::updated` mtime at draft time, so a later
    /// refresh can tell whether the source files changed since (tasks.md 3.3,
    /// design.md: "A changed task after graph draft marks the plan stale").
    pub source_updated_at: f64,
}

/// Whether `draft` is stale against the change's *current* mtime -- true
/// exactly when a file under the change's folder changed after the draft was
/// built. Tasks.md 3.3: "Detect task/spec changes after draft and block Start
/// until refreshed/accepted." This function only detects; blocking Start is
/// the `agent-desk-agent-graphs` package's job (its own AwaitingStart state
/// machine), which is why this returns a plain bool rather than an outcome
/// enum with a "blocked" variant that package does not own yet.
pub fn is_draft_stale(draft: &ProposedGraphDraft, current: &parse::SpecChange) -> bool {
    current.updated > draft.source_updated_at
}

/// Validates that `depends_on` edges in a draft form a DAG (no cycles,
/// no reference to a node id that is not in the draft). Tasks.md 3.1's
/// "requirement/task references... [are] provenance, not a second dependency
/// language" -- the dependency *graph* itself still has to be a real DAG, so
/// this is checked the same way the `agent-desk-agent-graphs` package will
/// check its own executable graph (build-order.md section 5.1), just against
/// the draft shape instead of live executions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphDraftValidation {
    Valid,
    UnknownDependency { node_id: String, missing: String },
    Cycle { node_ids: Vec<String> },
}

pub fn validate_draft_acyclic(nodes: &[ProposedGraphNode]) -> GraphDraftValidation {
    use std::collections::{HashMap, HashSet};

    let ids: HashSet<&str> = nodes.iter().map(|n| n.node_id.as_str()).collect();
    for node in nodes {
        for dep in &node.depends_on {
            if !ids.contains(dep.as_str()) {
                return GraphDraftValidation::UnknownDependency {
                    node_id: node.node_id.clone(),
                    missing: dep.clone(),
                };
            }
        }
    }

    let by_id: HashMap<&str, &ProposedGraphNode> = nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }
    let mut marks: HashMap<&str, Mark> = HashMap::new();
    let mut stack: Vec<&str> = Vec::new();

    fn visit<'a>(
        id: &'a str,
        by_id: &HashMap<&'a str, &'a ProposedGraphNode>,
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
        if let Some(node) = by_id.get(id) {
            for dep in &node.depends_on {
                if let Some(cycle) = visit(dep.as_str(), by_id, marks, stack) {
                    return Some(cycle);
                }
            }
        }
        stack.pop();
        marks.insert(id, Mark::Done);
        None
    }

    for node in nodes {
        if let Some(cycle) = visit(node.node_id.as_str(), &by_id, &mut marks, &mut stack) {
            return GraphDraftValidation::Cycle { node_ids: cycle };
        }
    }

    GraphDraftValidation::Valid
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_change(root: &Path, id: &str, tasks_md: &str) {
        let dir = root.join("openspec").join("changes").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("proposal.md"), format!("# Change: {id}\n\n## Why\n\nBecause.\n")).unwrap();
        fs::write(dir.join("tasks.md"), tasks_md).unwrap();
    }

    fn write_archived(root: &Path, id: &str, title: &str) {
        let dir = root.join("openspec").join("changes").join("archive").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("proposal.md"), format!("# {title}\n\n## Why\n\nBecause.\n")).unwrap();
        fs::write(dir.join("tasks.md"), "## 1. Group\n\n- [x] 1.1 Done thing\n").unwrap();
    }

    #[test]
    fn resolve_change_status_finds_an_active_change() {
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "add-thing", "## 1. Group\n\n- [ ] 1.1 Do it\n");
        let openspec_dir = dir.path().join("openspec");
        let status = resolve_change_status(&openspec_dir, "add-thing", "Add thing");
        assert!(matches!(status, OpenSpecChangeStatus::Active { .. }));
    }

    #[test]
    fn resolve_change_status_finds_an_archived_change() {
        let dir = tempfile::tempdir().unwrap();
        write_archived(dir.path(), "add-thing", "Change: Add thing");
        let openspec_dir = dir.path().join("openspec");
        let status = resolve_change_status(&openspec_dir, "add-thing", "Add thing");
        assert!(matches!(status, OpenSpecChangeStatus::Archived { .. }));
    }

    #[test]
    fn resolve_change_status_detects_a_likely_rename_by_title() {
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "add-thing-v2", "## 1. Group\n\n- [ ] 1.1 Do it\n");
        // proposal.md's title inside add-thing-v2 defaults to the id unless a
        // `# Change:` heading says otherwise -- give it the old title.
        fs::write(
            dir.path().join("openspec/changes/add-thing-v2/proposal.md"),
            "# Change: Add thing\n\n## Why\n\nBecause.\n",
        )
        .unwrap();
        let openspec_dir = dir.path().join("openspec");
        let status = resolve_change_status(&openspec_dir, "add-thing", "Add thing");
        match status {
            OpenSpecChangeStatus::Moved { likely_new_id, archived } => {
                assert_eq!(likely_new_id, "add-thing-v2");
                assert!(!archived);
            }
            other => panic!("expected Moved, got {other:?}"),
        }
    }

    #[test]
    fn resolve_change_status_reports_deleted_when_nothing_matches() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("openspec").join("changes")).unwrap();
        let openspec_dir = dir.path().join("openspec");
        let status = resolve_change_status(&openspec_dir, "ghost-change", "Ghost");
        assert!(matches!(status, OpenSpecChangeStatus::Deleted));
    }

    #[test]
    fn resolve_change_status_reports_deleted_with_no_snapshot_title_and_no_match() {
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "unrelated", "## 1. Group\n\n- [ ] 1.1 Do it\n");
        let openspec_dir = dir.path().join("openspec");
        let status = resolve_change_status(&openspec_dir, "ghost-change", "");
        assert!(matches!(status, OpenSpecChangeStatus::Deleted));
    }

    #[test]
    fn locate_target_task_keeps_identity_when_nothing_changed() {
        let tasks = vec![
            parse::SpecTask { index: 0, group: "1".into(), text: "1.1 First".into(), done: false, line: 3 },
            parse::SpecTask { index: 1, group: "1".into(), text: "1.2 Second".into(), done: false, line: 4 },
        ];
        let ctx = locate_target_task(&tasks, 1, "1.2 Second");
        assert_eq!(ctx.current_index, Some(1));
        assert!(!ctx.diverged);
    }

    /// The core Gate 4 claim: starting task 7 while task 3 is still open must
    /// keep naming task 7, not silently retarget to task 3 (the "next open
    /// task"). This is exercised again end-to-end in
    /// `commands::agent_desk::tests`, but the identity rule itself lives here.
    #[test]
    fn locate_target_task_preserves_a_non_next_task_when_earlier_tasks_are_still_open() {
        let tasks = vec![
            parse::SpecTask { index: 0, group: "1".into(), text: "1.1 First (open)".into(), done: false, line: 3 },
            parse::SpecTask { index: 1, group: "1".into(), text: "1.2 Second (open)".into(), done: false, line: 4 },
            parse::SpecTask { index: 2, group: "1".into(), text: "1.3 Third (open)".into(), done: false, line: 5 },
            parse::SpecTask { index: 6, group: "2".into(), text: "2.4 Target task".into(), done: false, line: 12 },
        ];
        let ctx = locate_target_task(&tasks, 6, "2.4 Target task");
        assert_eq!(ctx.current_index, Some(6));
        assert_eq!(ctx.current_text.as_deref(), Some("2.4 Target task"));
        assert!(!ctx.diverged);
        assert_eq!(ctx.launched_index, 6);
    }

    #[test]
    fn locate_target_task_follows_text_when_the_index_shifts() {
        // Two tasks were inserted above index 1, pushing the target task to
        // index 3 -- identity must follow the text, not the stale index.
        let tasks = vec![
            parse::SpecTask { index: 0, group: "1".into(), text: "1.1 First".into(), done: false, line: 3 },
            parse::SpecTask { index: 1, group: "1".into(), text: "1.2 Inserted A".into(), done: false, line: 4 },
            parse::SpecTask { index: 2, group: "1".into(), text: "1.3 Inserted B".into(), done: false, line: 5 },
            parse::SpecTask { index: 3, group: "1".into(), text: "1.4 Target".into(), done: false, line: 6 },
        ];
        let ctx = locate_target_task(&tasks, 1, "1.4 Target");
        assert_eq!(ctx.current_index, Some(3));
        assert!(ctx.diverged, "the line moved, so this must be flagged even though identity held");
    }

    #[test]
    fn locate_target_task_reports_divergence_when_the_task_is_gone() {
        let tasks = vec![parse::SpecTask {
            index: 0,
            group: "1".into(),
            text: "1.1 Something else".into(),
            done: false,
            line: 3,
        }];
        let ctx = locate_target_task(&tasks, 5, "9.9 Removed task");
        assert_eq!(ctx.current_index, None);
        assert!(ctx.diverged);
        assert_eq!(ctx.launched_index, 5);
        assert_eq!(ctx.launched_text, "9.9 Removed task");
    }

    #[test]
    fn locate_target_task_detects_duplicate_display_numbers_by_full_text() {
        // Two tasks can share a display number (author renumbering mistake);
        // full text (including that number) is still a unique-enough key in
        // practice, and this must not conflate them.
        let tasks = vec![
            parse::SpecTask { index: 0, group: "1".into(), text: "1.1 Wire the command".into(), done: false, line: 3 },
            parse::SpecTask { index: 1, group: "1".into(), text: "1.1 Wire the writer".into(), done: true, line: 4 },
        ];
        let ctx = locate_target_task(&tasks, 1, "1.1 Wire the writer");
        assert_eq!(ctx.current_index, Some(1));
        assert_eq!(ctx.current_done, Some(true));
        assert!(!ctx.diverged);
    }

    #[test]
    fn context_for_change_records_honest_absence_of_design() {
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "add-thing", "## 1. Group\n\n- [ ] 1.1 Do it\n");
        let openspec_dir = dir.path().join("openspec");
        let change = parse::parse_change_dir(&openspec_dir.join("changes").join("add-thing")).unwrap();
        assert!(!change.has_design);
        let ctx = context_for_change(dir.path(), &change, None).unwrap();
        assert_eq!(ctx.design, None, "no design.md must be None, not an empty string standing in for absence");
    }

    #[test]
    fn context_for_change_reads_design_when_present() {
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "add-thing", "## 1. Group\n\n- [ ] 1.1 Do it\n");
        fs::write(dir.path().join("openspec/changes/add-thing/design.md"), "# Design\n\nDetails.\n").unwrap();
        let openspec_dir = dir.path().join("openspec");
        let change = parse::parse_change_dir(&openspec_dir.join("changes").join("add-thing")).unwrap();
        let ctx = context_for_change(dir.path(), &change, None).unwrap();
        assert_eq!(ctx.design.as_deref(), Some("# Design\n\nDetails.\n"));
    }

    #[test]
    fn context_for_task_carries_the_exact_target_task() {
        let dir = tempfile::tempdir().unwrap();
        write_change(
            dir.path(),
            "add-thing",
            "## 1. Group\n\n- [ ] 1.1 First\n- [ ] 1.2 Second\n- [ ] 1.3 Third\n",
        );
        let openspec_dir = dir.path().join("openspec");
        let change = parse::parse_change_dir(&openspec_dir.join("changes").join("add-thing")).unwrap();
        let ctx = context_for_task(dir.path(), &change, 2, "1.3 Third", None).unwrap();
        let target = ctx.target_task.expect("target task present");
        assert_eq!(target.current_index, Some(2));
        assert!(!target.diverged);
    }

    #[test]
    fn is_draft_stale_true_only_after_a_later_change() {
        let draft = ProposedGraphDraft {
            change_id: "add-thing".into(),
            nodes: vec![],
            drafted_at: "2026-01-01T00:00:00Z".into(),
            source_updated_at: 1000.0,
        };
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "add-thing", "## 1. Group\n\n- [ ] 1.1 Do it\n");
        let openspec_dir = dir.path().join("openspec");
        let mut change = parse::parse_change_dir(&openspec_dir.join("changes").join("add-thing")).unwrap();

        change.updated = 999.0;
        assert!(!is_draft_stale(&draft, &change), "an older mtime than the draft must not be stale");

        change.updated = 1000.0;
        assert!(!is_draft_stale(&draft, &change), "an equal mtime must not be stale");

        change.updated = 1001.0;
        assert!(is_draft_stale(&draft, &change), "a newer mtime must be stale");
    }

    #[test]
    fn validate_draft_acyclic_accepts_a_simple_chain() {
        let nodes = vec![
            ProposedGraphNode {
                node_id: "a".into(),
                label: "A".into(),
                source_ref: None,
                depends_on: vec![],
            },
            ProposedGraphNode {
                node_id: "b".into(),
                label: "B".into(),
                source_ref: None,
                depends_on: vec!["a".into()],
            },
        ];
        assert_eq!(validate_draft_acyclic(&nodes), GraphDraftValidation::Valid);
    }

    #[test]
    fn validate_draft_acyclic_rejects_an_unknown_dependency() {
        let nodes = vec![ProposedGraphNode {
            node_id: "a".into(),
            label: "A".into(),
            source_ref: None,
            depends_on: vec!["ghost".into()],
        }];
        assert_eq!(
            validate_draft_acyclic(&nodes),
            GraphDraftValidation::UnknownDependency { node_id: "a".into(), missing: "ghost".into() }
        );
    }

    #[test]
    fn validate_draft_acyclic_rejects_a_direct_cycle() {
        let nodes = vec![
            ProposedGraphNode {
                node_id: "a".into(),
                label: "A".into(),
                source_ref: None,
                depends_on: vec!["b".into()],
            },
            ProposedGraphNode {
                node_id: "b".into(),
                label: "B".into(),
                source_ref: None,
                depends_on: vec!["a".into()],
            },
        ];
        assert!(matches!(validate_draft_acyclic(&nodes), GraphDraftValidation::Cycle { .. }));
    }

    #[test]
    fn validate_draft_acyclic_rejects_a_longer_cycle() {
        let nodes = vec![
            ProposedGraphNode { node_id: "a".into(), label: "A".into(), source_ref: None, depends_on: vec!["b".into()] },
            ProposedGraphNode { node_id: "b".into(), label: "B".into(), source_ref: None, depends_on: vec!["c".into()] },
            ProposedGraphNode { node_id: "c".into(), label: "C".into(), source_ref: None, depends_on: vec!["a".into()] },
        ];
        assert!(matches!(validate_draft_acyclic(&nodes), GraphDraftValidation::Cycle { .. }));
    }

    #[test]
    fn graph_node_ref_round_trips_every_variant() {
        let refs = vec![
            OpenSpecNodeRef::Task { change_id: "add-thing".into(), task_index: 2 },
            OpenSpecNodeRef::Requirement {
                change_id: "add-thing".into(),
                capability: "agent-desk".into(),
                requirement_name: "OpenSpec sources remain file-backed".into(),
            },
            OpenSpecNodeRef::Scenario {
                change_id: "add-thing".into(),
                capability: "agent-desk".into(),
                requirement_name: "Exact task identity is preserved".into(),
                scenario_name: "Non-next task".into(),
            },
        ];
        for r in refs {
            let json = serde_json::to_string(&r).unwrap();
            let back: OpenSpecNodeRef = serde_json::from_str(&json).unwrap();
            assert_eq!(format!("{r:?}"), format!("{back:?}"));
        }
    }
}
