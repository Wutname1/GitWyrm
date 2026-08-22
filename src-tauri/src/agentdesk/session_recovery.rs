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

use super::model::{AgentSessionHeader, ExecutionRecord, SessionState};

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
/// frontend owns this exact copy itself (`ConversationPane.tsx`, right next
/// to the equally-frontend-owned "Getting ready…"/"Working…" strings for
/// `Preparing`/`Working`), the same way every other `SessionState` is a bare
/// tag with no backend-supplied display text. This constant exists so the
/// wording has one canonical home in the backend for anyone changing
/// `reconcile_header`'s behavior to check against, and is asserted against
/// literally in this module's own tests
/// (`the_interrupted_reason_is_plain_language_with_no_jargon`) so a drift
/// between the two copies -- Rust doc comment vs. `ConversationPane.tsx`'s
/// JSX -- has a place to be caught on the backend side, even though nothing
/// automatically keeps the TypeScript string in sync.
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
        if execution.conflict.is_some() {
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
}
