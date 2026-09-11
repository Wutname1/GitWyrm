//! Reconcile a session's persisted state against the process that is
//! actually alive right now.
//!
//! [`super::model::SessionState::Preparing`], `Working`, and `NeedsInput` are
//! only truthful while the process that started that execution is still
//! running: nothing else ever advances a session out of those states. Every
//! in-process registry that could make one of them true again --
//! [`super::execution_registry::ExecutionRegistry`] (keyed by
//! `(session_id, execution_id)`) and `airun::SessionRegistry` -- starts empty
//! on every launch (`app.manage(...::new())` in `lib.rs`), so a session read
//! with one of those three states is, by construction, describing a process
//! that no longer exists: a crash, a force-quit, a power loss, or an app
//! update all leave exactly this shape on disk with nothing left to finish
//! the job.
//!
//! This module has no Tauri dependency, matching [`super::reconcile`]'s
//! shape: callers (`commands/agent_desk.rs`) resolve "is this session's
//! execution actually live in this process" themselves (via
//! `ExecutionRegistry::live_executions_for_session`) and pass the answer in
//! as a plain `bool`, so [`reconcile_header`] and [`reconcile_executions`]
//! stay pure functions directly testable without a running app.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::model::{AgentSession, AgentSessionHeader, ExecutionId, ExecutionRecord, SessionState};

/// States that only mean anything while their owning process is alive. A
/// session or execution record found in one of these, when nothing in this
/// process backs it, is stale rather than actually in progress.
pub fn is_live_process_state(state: SessionState) -> bool {
    matches!(
        state,
        SessionState::Preparing | SessionState::Working | SessionState::NeedsInput
    )
}

/// Plain-language reason a non-expert can read for why a session ended up in
/// [`SessionState::Interrupted`]. Not sent over the wire today -- the
/// frontend owns this exact copy itself, the same way every other
/// `SessionState` is a bare tag with no backend-supplied display text.
///
/// **Where the frontend copy lives: `src/lib/agentDeskResult.ts`**, in
/// `runStoppedBadlyLabel`. This comment used to say `ConversationPane.tsx`,
/// which is where it was *before* being pulled into shared code because the
/// same sentence had been hand-written three times. That component now only
/// calls `runStoppedBadlyLabel(state)`, so anyone following the old pointer
/// found no string, and would reasonably conclude this constant was dead --
/// defeating the one job the comment exists to do.
///
/// The constant exists so the wording has one canonical home in the backend
/// for anyone changing `reconcile_header`'s behavior, and is asserted
/// literally in this module's tests
/// (`the_interrupted_reason_is_plain_language_with_no_jargon`) so drift has a
/// place to be caught on the backend side. Nothing automatically keeps the
/// TypeScript string in sync -- the two are checked by a person reading both.
pub const INTERRUPTED_REASON: &str = "This chat stopped when the app closed.";

/// What happened to one header when reconciliation ran. `Unchanged` is the
/// common case (nothing here needs fixing) and callers should treat it as
/// "no write needed," not as a fault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HeaderReconciliation {
    Unchanged,
    /// The header claimed a live-process state with nothing in this process
    /// backing it, so it was moved to `Interrupted`.
    Interrupted,
}

/// Reconcile one session header in place. Returns what changed, so a caller
/// that only has headers in hand (the sidebar list) knows whether a write is
/// needed without re-deriving the check itself.
///
/// `execution_is_live` answers "does this process currently have a live run
/// attached to this exact session right now" -- callers derive this from
/// `!ExecutionRegistry::live_executions_for_session(&header.session_id).is_empty()`,
/// which this function has no way to check itself (it has no dependency on
/// that registry's type, by design -- see module doc).
pub fn reconcile_header(header: &mut AgentSessionHeader, execution_is_live: bool) -> HeaderReconciliation {
    if !is_live_process_state(header.state) || execution_is_live {
        return HeaderReconciliation::Unchanged;
    }
    header.state = SessionState::Interrupted;
    HeaderReconciliation::Interrupted
}

/// Reconcile every `ExecutionRecord` in `executions` in place. A full session
/// carries potentially several execution records (lead plus helpers, tasks.md
/// graph work) and the graph panel renders every one of them, so a fix that
/// only touched the header would leave helper nodes showing "Working"
/// forever even after the lead's own state was corrected.
///
/// `is_live` is called once per execution record still claiming a
/// live-process state; it should answer "is this specific `execution_id`
/// backed by something alive in this process right now" -- typically
/// `ExecutionRegistry::is_live(&session_id, execution_id)` directly, since the
/// registry IS execution-granular. A caller with only a coarser,
/// session-granular liveness answer may still pass a closure that ignores its
/// argument and returns that one answer for every execution in the session.
/// Returns the number of records actually changed, so the caller can skip a
/// write when nothing moved.
///
/// A helper with `conflict.is_some()` is left alone even though its state is
/// `NeedsInput` (a live-process state by [`is_live_process_state`]'s general
/// rule): `agent_graph::integrate_helper_result` sets that combination
/// directly, with no live process behind it at all -- it is durably waiting
/// on a person to pick `KeepHelper`/`KeepIntegrated`/`UseMerged`
/// (`agent_graph::resolve_conflict_at`), not on the CLI subprocess that
/// finished and deregistered from [`super::ExecutionRegistry`] well before
/// the conflict was ever recorded (see `advance_graph_after_helper_completion`
/// in `commands/agent_graph.rs`: `executions.complete()` runs before
/// `integrate_helper_result` even starts looking for conflicts). Without this
/// carve-out, every conflict left unresolved across an app restart would
/// silently flip to `Interrupted` on the very next load, and the user's
/// pending choice -- and the two preserved texts it was about to pick between
/// -- would vanish from the graph panel with no way back.
pub fn reconcile_executions(
    executions: &mut [ExecutionRecord],
    mut is_live: impl FnMut(&str) -> bool,
) -> u32 {
    let mut changed = 0u32;
    for execution in executions.iter_mut() {
        if !is_live_process_state(execution.state) {
            continue;
        }
        // A lead holding a `proposed_graph` waits on a person exactly as a
        // conflict does: the plan was proposed, the turn that produced it
        // ended, and the process deregistered -- so it looks orphaned while
        // being nothing of the sort. `recover_orphaned_executions` has always
        // skipped both; this path skipped only the conflict, so opening a
        // chat with a plan awaiting Start silently voided it.
        //
        // It voided it invisibly, which is the worst shape: the graph panel
        // finds that lead by `proposed_graph` alone, so Start/Revise/Use solo
        // still render -- but `Interrupted` is terminal (`graph::is_terminal`),
        // so Start does nothing. A control that is visibly offered and cannot
        // work is the Rule #1 failure, and losing a plan the person was about
        // to approve is the "nothing lands automatically" promise breaking in
        // the direction that costs work.
        if execution.conflict.is_some() || execution.proposed_graph.is_some() {
            continue;
        }
        if is_live(&execution.execution_id) {
            continue;
        }
        // Leave `ended_at` as-is: it was `None` while genuinely running (see
        // `bridge::apply_run_event`, which only sets it on `Finished` /
        // `Stopped` / `Failed`), and it stays `None` here too -- the process
        // never reached an end state, it simply stopped existing. Setting a
        // synthetic end time would claim a precision about when the process
        // died that reconciliation has no way to know.
        execution.state = SessionState::Interrupted;
        changed += 1;
    }
    changed
}

// ---------------------------------------------------------------------------
// Startup recovery of executions that died with the app
// ---------------------------------------------------------------------------

/// Plain-language `output_summary` stamped on every execution the startup
/// sweep marks `Failed`. Kept as a constant so the wording lives in one place
/// and the no-jargon test below can pin it.
pub const CLOSED_WHILE_RUNNING_DETAIL: &str = "GitWyrm was closed while this was running.";

/// What the startup sweep found on disk for one orphaned execution. Decides
/// both the wording of the note and whether a result record is worth
/// building: only `WorktreePresent` has anything a person could Keep.
#[derive(Debug, Clone, PartialEq)]
pub enum OrphanWork {
    /// The record never had a worktree (a lead running against the session's
    /// own checkout, or a read-only helper).
    NoWorktree,
    /// A worktree was recorded but the folder is gone from disk.
    WorktreeMissing { path: String },
    /// The worktree folder still exists; `changed_file_count` is how many
    /// files differ from its base right now.
    WorktreePresent { path: String, changed_file_count: usize },
}

/// One execution the startup sweep moved to `Failed`, plus what the caller
/// still has to do for it outside the session lock: build a result (if
/// [`RecoveredOrphan::result_worktree_path`] is `Some`) and append `note` to
/// the transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveredOrphan {
    pub execution_id: ExecutionId,
    pub is_helper: bool,
    pub work: OrphanWork,
    /// Transcript note in plain language, already composed.
    pub note: String,
}

impl RecoveredOrphan {
    /// The worktree a result should be built against, or `None` when there is
    /// nothing on disk to review.
    pub fn result_worktree_path(&self) -> Option<&str> {
        match &self.work {
            OrphanWork::WorktreePresent { path, .. } => Some(path),
            _ => None,
        }
    }
}

fn orphan_note(job_title: Option<&str>, is_helper: bool, work: &OrphanWork) -> String {
    let who = match (job_title, is_helper) {
        (Some(title), _) if !title.trim().is_empty() => format!("The helper \"{}\"", title.trim()),
        (_, true) => "A helper".to_string(),
        (_, false) => "The lead".to_string(),
    };
    let what_remains = match work {
        OrphanWork::NoWorktree => {
            "It was not working in a separate folder, so there is nothing extra to keep or discard.".to_string()
        }
        OrphanWork::WorktreeMissing { .. } => {
            "Its work folder is no longer on disk, so there is nothing to keep.".to_string()
        }
        OrphanWork::WorktreePresent {
            changed_file_count: 0,
            ..
        } => "Its work folder is still here but has no changes in it.".to_string(),
        OrphanWork::WorktreePresent {
            changed_file_count: 1,
            ..
        } => "It left 1 changed file behind. Open its result to keep or discard that work.".to_string(),
        OrphanWork::WorktreePresent {
            changed_file_count, ..
        } => format!("It left {changed_file_count} changed files behind. Open its result to keep or discard that work."),
    };
    format!("{who} stopped because GitWyrm was closed while it was running. {what_remains}")
}

/// Startup counterpart to [`reconcile_executions`]. That function runs lazily
/// when a session is opened and only flips the state to `Interrupted`; it
/// never looks at the worktree a dead helper left behind, so a helper that
/// died before producing a result kept its folder on disk forever with no
/// way to Keep or discard the work. This runs once at launch, before any
/// session can be opened, and does the full cleanup: every execution still
/// claiming a live-process state that nothing in this process backs is
/// marked `Failed` (a terminal state, so the lazy path leaves it alone
/// afterwards), its `ended_at` is set to now, the header's
/// `active_execution_id` is cleared if it pointed at it, and the header
/// itself moves to `Failed` once no execution in the session is running any
/// more.
///
/// `is_live` answers per execution id, exactly as for
/// [`reconcile_executions`]. `inspect_worktree` is handed a recorded
/// worktree path and answers `None` if the folder is gone or `Some(changed
/// file count)` if it is still there; it is a closure so this stays free of
/// git and filesystem dependencies and directly testable.
///
/// Two kinds of `NeedsInput` are skipped on purpose because they wait on a
/// person, not a process: a helper with a recorded `conflict` (same reason
/// as in [`reconcile_executions`]) and a lead with a `proposed_graph` that
/// is waiting for Start. Failing either would throw away the pending choice.
///
/// Returns what was recovered so the caller can build results and append
/// notes outside the session lock. Empty means nothing was written.
pub fn recover_orphaned_executions(
    session: &mut AgentSession,
    now: &str,
    mut is_live: impl FnMut(&str) -> bool,
    mut inspect_worktree: impl FnMut(&str) -> Option<usize>,
) -> Vec<RecoveredOrphan> {
    let mut recovered = Vec::new();
    for execution in session.executions.iter_mut() {
        if !is_live_process_state(execution.state) {
            continue;
        }
        if execution.conflict.is_some() || execution.proposed_graph.is_some() {
            continue;
        }
        if is_live(&execution.execution_id) {
            continue;
        }

        // A lead in the middle of a graph has no worktree of its own but may
        // have an integration worktree holding every helper's merged work;
        // that is the folder worth offering to keep.
        let recorded_path = execution
            .worktree_path
            .clone()
            .or_else(|| execution.integration_worktree_path.clone());
        let work = match recorded_path {
            None => OrphanWork::NoWorktree,
            Some(path) => match inspect_worktree(&path) {
                None => OrphanWork::WorktreeMissing { path },
                Some(changed_file_count) => OrphanWork::WorktreePresent {
                    path,
                    changed_file_count,
                },
            },
        };

        execution.state = SessionState::Failed;
        execution.ended_at = Some(now.to_string());
        if execution.output_summary.is_none() {
            execution.output_summary = Some(CLOSED_WHILE_RUNNING_DETAIL.to_string());
        }
        if session.header.active_execution_id.as_deref() == Some(execution.execution_id.as_str()) {
            session.header.active_execution_id = None;
        }

        let is_helper = execution.parent_execution_id.is_some();
        recovered.push(RecoveredOrphan {
            execution_id: execution.execution_id.clone(),
            is_helper,
            note: orphan_note(execution.job_title.as_deref(), is_helper, &work),
            work,
        });
    }

    if recovered.is_empty() {
        return recovered;
    }
    session.header.updated_at = now.to_string();
    let anything_still_running = session.executions.iter().any(|e| is_live_process_state(e.state));
    if is_live_process_state(session.header.state) && !anything_still_running {
        session.header.state = SessionState::Failed;
    }
    recovered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::{SessionIntent, SessionSource, CURRENT_SCHEMA_VERSION};

    fn header(state: SessionState) -> AgentSessionHeader {
        AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: "sess-1".into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/proj".into(),
            repo_name: "proj".into(),
            title: "A session".into(),
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
        }
    }

    fn execution(execution_id: &str, state: SessionState) -> ExecutionRecord {
        ExecutionRecord::minimal(
            execution_id.into(),
            "sess-1".into(),
            None,
            state,
            "2026-01-01T00:00:00Z".into(),
            None,
            3,
        )
    }

    #[test]
    fn a_preparing_header_with_no_live_backing_becomes_interrupted() {
        let mut h = header(SessionState::Preparing);
        let outcome = reconcile_header(&mut h, false);
        assert_eq!(outcome, HeaderReconciliation::Interrupted);
        assert_eq!(h.state, SessionState::Interrupted);
    }

    #[test]
    fn a_working_header_with_no_live_backing_becomes_interrupted() {
        let mut h = header(SessionState::Working);
        let outcome = reconcile_header(&mut h, false);
        assert_eq!(outcome, HeaderReconciliation::Interrupted);
        assert_eq!(h.state, SessionState::Interrupted);
    }

    #[test]
    fn a_needs_input_header_with_no_live_backing_becomes_interrupted() {
        let mut h = header(SessionState::NeedsInput);
        let outcome = reconcile_header(&mut h, false);
        assert_eq!(outcome, HeaderReconciliation::Interrupted);
        assert_eq!(h.state, SessionState::Interrupted);
    }

    #[test]
    fn a_live_process_state_backed_by_a_real_link_is_left_alone() {
        let mut h = header(SessionState::Working);
        let outcome = reconcile_header(&mut h, true);
        assert_eq!(outcome, HeaderReconciliation::Unchanged);
        assert_eq!(h.state, SessionState::Working);
    }

    #[test]
    fn finished_failed_and_stopped_headers_are_never_touched() {
        for state in [SessionState::Finished, SessionState::Failed, SessionState::Stopped] {
            let mut h = header(state);
            let outcome = reconcile_header(&mut h, false);
            assert_eq!(outcome, HeaderReconciliation::Unchanged);
            assert_eq!(h.state, state, "a terminal state must never be rewritten");
        }
    }

    #[test]
    fn draft_ready_and_missing_source_headers_are_never_touched() {
        for state in [SessionState::Draft, SessionState::Ready, SessionState::MissingSource] {
            let mut h = header(state);
            let outcome = reconcile_header(&mut h, false);
            assert_eq!(outcome, HeaderReconciliation::Unchanged);
            assert_eq!(h.state, state);
        }
    }

    #[test]
    fn an_already_interrupted_header_is_idempotent() {
        let mut h = header(SessionState::Interrupted);
        let outcome = reconcile_header(&mut h, false);
        assert_eq!(outcome, HeaderReconciliation::Unchanged);
        assert_eq!(h.state, SessionState::Interrupted);
    }

    #[test]
    fn every_stuck_execution_record_is_reconciled_not_just_the_lead() {
        let mut executions = vec![
            execution("lead", SessionState::Working),
            execution("helper-1", SessionState::NeedsInput),
            execution("helper-2", SessionState::Finished),
        ];
        let changed = reconcile_executions(&mut executions, |_| false);
        assert_eq!(changed, 2);
        assert_eq!(executions[0].state, SessionState::Interrupted);
        assert_eq!(executions[1].state, SessionState::Interrupted);
        assert_eq!(executions[2].state, SessionState::Finished, "already-finished helper untouched");
    }

    #[test]
    fn a_helper_backed_by_a_live_execution_is_left_running() {
        let mut executions = vec![
            execution("lead", SessionState::Working),
            execution("helper-1", SessionState::Working),
        ];
        let changed = reconcile_executions(&mut executions, |id| id == "helper-1");
        assert_eq!(changed, 1);
        assert_eq!(executions[0].state, SessionState::Interrupted, "lead not live, must flip");
        assert_eq!(executions[1].state, SessionState::Working, "helper is live, must stay");
    }

    #[test]
    fn reconciling_executions_with_nothing_stuck_reports_zero_changes() {
        let mut executions = vec![execution("lead", SessionState::Finished)];
        let changed = reconcile_executions(&mut executions, |_| false);
        assert_eq!(changed, 0);
        assert_eq!(executions[0].state, SessionState::Finished);
    }

    /// A conflicted helper survives a session reload without becoming
    /// `Interrupted`, even though it is `NeedsInput` (ordinarily a
    /// live-process state) and nothing in this process backs it -- the
    /// secondary bug this module's own doc comment on `reconcile_executions`
    /// explains: a conflict is a durable wait for a person, not for a
    /// process, and `advance_graph_after_helper_completion` always
    /// deregisters the helper from `ExecutionRegistry` before a conflict is
    /// even detected, so `is_live` legitimately returns false here.
    #[test]
    fn a_conflicted_helper_survives_reconciliation_without_becoming_interrupted() {
        let mut exec = execution("helper-a", SessionState::NeedsInput);
        exec.conflict = Some(crate::agentdesk::graph::IntegrationConflict {
            path: "src/greet.rs".into(),
            conflicting_with: "lead".into(),
            base_text: "base".into(),
            helper_text: "helper version".into(),
            integrated_text: "lead version".into(),
        });
        let mut executions = vec![exec];

        let changed = reconcile_executions(&mut executions, |_| false);

        assert_eq!(changed, 0, "a conflicted node must not be counted as reconciled");
        assert_eq!(
            executions[0].state,
            SessionState::NeedsInput,
            "a conflicted node must stay NeedsInput, never flip to Interrupted"
        );
        assert!(
            executions[0].conflict.is_some(),
            "the conflict itself, and the two preserved texts inside it, must survive the reload"
        );
    }

    /// A plain (non-conflict) `NeedsInput` execution -- e.g. a live approval
    /// gate -- must still be reconciled normally when nothing backs it. The
    /// carve-out above is specific to conflicts, not to `NeedsInput` as a
    /// whole.
    #[test]
    fn a_plain_needs_input_execution_with_no_conflict_is_still_reconciled() {
        let mut executions = vec![execution("lead", SessionState::NeedsInput)];
        let changed = reconcile_executions(&mut executions, |_| false);
        assert_eq!(changed, 1);
        assert_eq!(executions[0].state, SessionState::Interrupted);
    }

    /// Pins the exact wording -- a non-expert reason, no jargon like "orphaned
    /// execution" or "stale state." `ConversationPane.tsx` owns the actual
    /// frontend-facing copy (this constant is not sent over the wire, see its
    /// own doc comment), so this is the one place a Rust-side change to the
    /// wording gets caught, even though nothing keeps the TypeScript string
    /// automatically in sync.
    #[test]
    fn the_interrupted_reason_is_plain_language_with_no_jargon() {
        assert_eq!(INTERRUPTED_REASON, "This chat stopped when the app closed.");
        for jargon in ["orphan", "stale", "execution", "process"] {
            assert!(
                !INTERRUPTED_REASON.to_lowercase().contains(jargon),
                "reason text must stay plain-language, found {jargon:?} in {INTERRUPTED_REASON:?}"
            );
        }
    }

    // -- startup orphan recovery --

    const NOW: &str = "2026-02-02T00:00:00Z";

    fn session_with(executions: Vec<ExecutionRecord>) -> AgentSession {
        let mut s = AgentSession::new(header(SessionState::Working));
        s.executions = executions;
        s
    }

    fn helper(execution_id: &str, state: SessionState, worktree_path: Option<&str>) -> ExecutionRecord {
        let mut e = execution(execution_id, state);
        e.parent_execution_id = Some("lead".into());
        e.worktree_path = worktree_path.map(str::to_string);
        e
    }

    fn never_live(_: &str) -> bool {
        false
    }

    #[test]
    fn a_working_helper_whose_worktree_still_has_changes_fails_and_gets_a_result() {
        let mut s = session_with(vec![
            execution("lead", SessionState::Finished),
            helper("helper-1", SessionState::Working, Some("C:/wt/helper-1")),
        ]);
        s.header.active_execution_id = Some("lead".into());

        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |path| {
            assert_eq!(path, "C:/wt/helper-1");
            Some(2)
        });

        assert_eq!(recovered.len(), 1);
        let orphan = &recovered[0];
        assert_eq!(orphan.execution_id, "helper-1");
        assert!(orphan.is_helper);
        assert_eq!(orphan.result_worktree_path(), Some("C:/wt/helper-1"));
        assert!(orphan.note.contains("2 changed files"), "note was {:?}", orphan.note);
        assert!(orphan.note.contains("keep or discard"), "note was {:?}", orphan.note);

        let h = &s.executions[1];
        assert_eq!(h.state, SessionState::Failed);
        assert_eq!(h.ended_at.as_deref(), Some(NOW));
        assert_eq!(h.output_summary.as_deref(), Some(CLOSED_WHILE_RUNNING_DETAIL));
        assert_eq!(s.executions[0].state, SessionState::Finished, "finished lead untouched");
        assert_eq!(
            s.header.active_execution_id.as_deref(),
            Some("lead"),
            "active id pointed at the lead, not the helper, so it stays"
        );
    }

    #[test]
    fn a_working_helper_with_no_worktree_fails_with_a_note_and_no_result() {
        let mut s = session_with(vec![helper("helper-1", SessionState::Working, None)]);

        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |_| {
            panic!("no worktree recorded, nothing to inspect")
        });

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].work, OrphanWork::NoWorktree);
        assert_eq!(recovered[0].result_worktree_path(), None);
        assert!(recovered[0].note.contains("nothing extra to keep"), "note was {:?}", recovered[0].note);
        assert_eq!(s.executions[0].state, SessionState::Failed);
    }

    #[test]
    fn a_helper_whose_worktree_folder_is_gone_says_so_and_gets_no_result() {
        let mut s = session_with(vec![helper("helper-1", SessionState::Preparing, Some("C:/wt/gone"))]);

        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |_| None);

        assert_eq!(recovered.len(), 1);
        assert_eq!(
            recovered[0].work,
            OrphanWork::WorktreeMissing {
                path: "C:/wt/gone".into()
            }
        );
        assert_eq!(recovered[0].result_worktree_path(), None);
        assert!(recovered[0].note.contains("no longer on disk"), "note was {:?}", recovered[0].note);
        assert_eq!(s.executions[0].state, SessionState::Failed);
    }

    #[test]
    fn a_finished_execution_is_never_touched() {
        let mut s = session_with(vec![
            execution("lead", SessionState::Finished),
            helper("helper-1", SessionState::Failed, Some("C:/wt/helper-1")),
        ]);
        s.header.state = SessionState::Finished;
        let before = s.clone();

        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |_| Some(5));

        assert!(recovered.is_empty());
        assert_eq!(s, before, "nothing may change when nothing is stuck");
    }

    #[test]
    fn an_execution_that_is_live_in_the_registry_is_left_running() {
        let mut s = session_with(vec![
            execution("lead", SessionState::Working),
            helper("helper-1", SessionState::Working, Some("C:/wt/helper-1")),
        ]);
        s.header.active_execution_id = Some("lead".into());
        let live = ["lead".to_string(), "helper-1".to_string()];

        let recovered = recover_orphaned_executions(&mut s, NOW, |id| live.iter().any(|l| l == id), |_| Some(1));

        assert!(recovered.is_empty());
        assert_eq!(s.executions[0].state, SessionState::Working);
        assert_eq!(s.executions[1].state, SessionState::Working);
        assert_eq!(s.header.state, SessionState::Working);
        assert_eq!(s.header.active_execution_id.as_deref(), Some("lead"));
    }

    #[test]
    fn a_dead_lead_clears_the_active_id_and_fails_the_header() {
        let mut s = session_with(vec![
            execution("lead", SessionState::Working),
            helper("helper-1", SessionState::Working, None),
        ]);
        s.header.active_execution_id = Some("lead".into());

        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |_| None);

        assert_eq!(recovered.len(), 2);
        assert!(!recovered[0].is_helper);
        assert!(recovered[0].note.starts_with("The lead"), "note was {:?}", recovered[0].note);
        assert_eq!(s.header.active_execution_id, None);
        assert_eq!(s.header.state, SessionState::Failed);
        assert_eq!(s.header.updated_at, NOW);
    }

    #[test]
    fn a_dead_lead_mid_graph_offers_its_integration_worktree() {
        let mut lead = execution("lead", SessionState::Working);
        lead.integration_worktree_path = Some("C:/wt/integration".into());
        let mut s = session_with(vec![lead]);

        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |path| {
            assert_eq!(path, "C:/wt/integration");
            Some(3)
        });

        assert_eq!(recovered[0].result_worktree_path(), Some("C:/wt/integration"));
    }

    #[test]
    fn the_header_stays_working_while_a_live_execution_remains() {
        let mut s = session_with(vec![
            execution("lead", SessionState::Working),
            helper("helper-1", SessionState::Working, None),
        ]);

        let recovered = recover_orphaned_executions(&mut s, NOW, |id| id == "lead", |_| None);

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].execution_id, "helper-1");
        assert_eq!(s.header.state, SessionState::Working, "the lead is still alive");
    }

    #[test]
    fn a_conflicted_helper_and_a_proposal_waiting_on_start_are_skipped() {
        let mut conflicted = helper("helper-1", SessionState::NeedsInput, Some("C:/wt/helper-1"));
        conflicted.conflict = Some(crate::agentdesk::graph::IntegrationConflict {
            path: "src/greet.rs".into(),
            conflicting_with: "lead".into(),
            base_text: "base".into(),
            helper_text: "helper".into(),
            integrated_text: "lead".into(),
        });
        let mut proposal = execution("lead", SessionState::NeedsInput);
        proposal.proposed_graph = Some(crate::agentdesk::graph::ProposedGraph {
            lead_summary: "Split the work".into(),
            helpers: Vec::new(),
            proposed_at: NOW.into(),
        });
        let mut s = session_with(vec![proposal, conflicted]);
        s.header.state = SessionState::NeedsInput;

        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |_| Some(1));

        assert!(recovered.is_empty());
        assert_eq!(s.executions[0].state, SessionState::NeedsInput);
        assert_eq!(s.executions[1].state, SessionState::NeedsInput);
        assert_eq!(s.header.state, SessionState::NeedsInput);
    }

    /// The counterpart the test above never had. Both recovery paths decide
    /// the same question -- "is this waiting on a person or on a process that
    /// is gone?" -- and only one of them was pinned, which is exactly where
    /// they drifted: the lazy path skipped a conflict but not a proposal, so
    /// merely opening a chat whose plan was awaiting Start voided that plan.
    ///
    /// It voided it invisibly. The graph panel finds the awaiting lead by
    /// `proposed_graph` alone, so Start still appears, while `Interrupted` is
    /// terminal and Start does nothing.
    #[test]
    fn opening_a_chat_does_not_void_a_plan_waiting_on_start() {
        let mut conflicted = helper("helper-1", SessionState::NeedsInput, Some("C:/wt/helper-1"));
        conflicted.conflict = Some(crate::agentdesk::graph::IntegrationConflict {
            path: "src/greet.rs".into(),
            conflicting_with: "lead".into(),
            base_text: "base".into(),
            helper_text: "helper".into(),
            integrated_text: "lead".into(),
        });
        let mut proposal = execution("lead", SessionState::NeedsInput);
        proposal.proposed_graph = Some(crate::agentdesk::graph::ProposedGraph {
            lead_summary: "Split the work".into(),
            helpers: Vec::new(),
            proposed_at: NOW.into(),
        });
        let mut executions = vec![proposal, conflicted];

        let changed = reconcile_executions(&mut executions, never_live);

        assert_eq!(changed, 0, "neither is orphaned -- both wait on a person");
        assert_eq!(executions[0].state, SessionState::NeedsInput);
        assert_eq!(executions[1].state, SessionState::NeedsInput);
    }

    /// The two paths must agree about what counts as waiting on a person, or
    /// one of them loses work the other protects. Checked by running both
    /// over the same records rather than by reading the two functions and
    /// hoping -- which is how the divergence above survived 46 reviews.
    #[test]
    fn both_recovery_paths_protect_the_same_human_waits() {
        for make in [
            |e: &mut ExecutionRecord| {
                e.conflict = Some(crate::agentdesk::graph::IntegrationConflict {
                    path: "src/greet.rs".into(),
                    conflicting_with: "lead".into(),
                    base_text: "base".into(),
                    helper_text: "helper".into(),
                    integrated_text: "lead".into(),
                });
            },
            |e: &mut ExecutionRecord| {
                e.proposed_graph = Some(crate::agentdesk::graph::ProposedGraph {
                    lead_summary: "Split the work".into(),
                    helpers: Vec::new(),
                    proposed_at: NOW.into(),
                });
            },
        ] {
            let mut lazy = execution("lead", SessionState::NeedsInput);
            make(&mut lazy);
            let mut lazy_list = vec![lazy];
            assert_eq!(
                reconcile_executions(&mut lazy_list, never_live),
                0,
                "the lazy path changed a record that waits on a person"
            );

            let mut swept = execution("lead", SessionState::NeedsInput);
            make(&mut swept);
            let mut s = session_with(vec![swept]);
            s.header.state = SessionState::NeedsInput;
            assert!(
                recover_orphaned_executions(&mut s, NOW, never_live, |_| Some(1)).is_empty(),
                "the startup sweep recovered a record that waits on a person"
            );
        }
    }

    #[test]
    fn a_titled_helper_is_named_in_its_note() {
        let mut h = helper("helper-1", SessionState::Working, None);
        h.job_title = Some("Rename the config loader".into());
        let mut s = session_with(vec![h]);

        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |_| None);

        assert!(
            recovered[0].note.starts_with("The helper \"Rename the config loader\" stopped"),
            "note was {:?}",
            recovered[0].note
        );
    }

    #[test]
    fn recovery_wording_is_plain_language_with_no_jargon() {
        let mut s = session_with(vec![
            execution("lead", SessionState::Working),
            helper("helper-1", SessionState::Working, Some("C:/wt/a")),
            helper("helper-2", SessionState::Working, Some("C:/wt/b")),
            helper("helper-3", SessionState::Working, None),
        ]);
        let mut calls = 0;
        let recovered = recover_orphaned_executions(&mut s, NOW, never_live, |_| {
            calls += 1;
            if calls == 1 {
                Some(4)
            } else {
                None
            }
        });
        let mut texts: Vec<String> = recovered.into_iter().map(|o| o.note).collect();
        texts.push(CLOSED_WHILE_RUNNING_DETAIL.to_string());
        for text in texts {
            for jargon in ["orphan", "stale", "execution", "process", "worktree", "registry"] {
                assert!(
                    !text.to_lowercase().contains(jargon),
                    "found {jargon:?} in {text:?}"
                );
            }
        }
    }
}
