//! The unified result state (tasks.md package `agent-desk-review-and-landing`,
//! section 1): what a session's execution produced, described as *references*
//! into repository truth rather than a copy of it.
//!
//! architecture.md: "Agent Desk stores references to repository truth, not
//! copied diffs." This module never stores diff text, check output, or commit
//! content -- it names a worktree, a base/head pair, a list of changed paths,
//! and links to checks/commit/source/OpenSpec task. Every render of "what
//! changed" goes back through the existing diff/status/check machinery
//! (`git::types::working_status`, `commands::diff::get_file_diff`) against
//! those references at read time.
//!
//! [`ResultRecord`] lives on the session, one per execution that produced (or
//! attempted to produce) a result -- including a stopped, failed, or
//! conflicted one (task 1.3: "Persist partial results"). [`ResultState`] is
//! the state machine reviewers act on: `Reviewing` is the default the moment
//! an execution finishes, and nothing moves it to `Committed` except an
//! explicit user action (spec: "Finished does not mean committed").

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::model::{ExecutionId, SessionId, SessionLoadError};
use super::store::{read_json_file, write_atomic, SessionStoreRoot, WriteError};

/// A changed file, named but not diffed -- the diff itself is read from the
/// worktree at render time via the existing status/diff commands, never
/// stored here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResultChangedPath {
    pub path: String,
    pub old_path: Option<String>,
    /// `"A" | "M" | "D" | "R" | "!"`, mirroring [`crate::git::types::StatusCode`]'s
    /// serde tag directly rather than re-exporting that type here -- the result
    /// model deliberately does not depend on the status/diff module's internal
    /// shape, only on a string small enough that a UI can key off it.
    pub status: String,
}

/// One check's outcome, named and summarized -- never the raw terminal
/// output (task 2.4: "without raw terminal flood").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResultCheckOutcome {
    /// The command name as configured (e.g. "npm run typecheck"), not its
    /// full invocation with flags -- what the user would recognize.
    pub command_name: String,
    pub outcome: CheckRunOutcome,
    /// A short, plain-language summary line (e.g. "3 errors"), never the raw
    /// stdout/stderr stream.
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CheckRunOutcome {
    Passed,
    Failed,
    /// The check was configured but never ran (execution stopped first).
    Skipped,
}

/// A reference to the commit a kept result landed as, once one exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResultCommitRef {
    pub oid: String,
    /// The subject line only, for display in a result summary -- the full
    /// message (with trailers) is read from the commit object itself, not
    /// duplicated here.
    pub subject: String,
}

/// Where a session's result stands in the review/land lifecycle.
///
/// design.md: "Keep/Undo/Revise preserve current run-completion semantics.
/// Commit is intentional." Nothing but an explicit user action advances past
/// `Reviewing` -- see `agent_result_keep`/`agent_result_commit` in
/// `commands::agent_result`, the only writers of `Kept`/`Committed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ResultState {
    /// The execution finished (or stopped/failed/conflicted) and produced
    /// something to look at. The default state the moment a result record
    /// exists -- spec "Finished does not mean committed".
    Reviewing,
    /// The user asked the lead to revise; a new lead message/execution step
    /// was appended (task 2.5) and this result is superseded once that step
    /// produces its own record.
    RevisionRequested,
    /// The user chose Keep: the worktree's changes are considered good and
    /// ready to commit, but no commit has been made yet.
    Kept,
    /// An intentional commit was created from this result (task 3.3).
    Committed,
    /// The user chose Undo: the changes were discarded/reverted via the
    /// existing run-completion path (task 3.1).
    Discarded,
    /// The result is committed/discarded but its worktree still needs
    /// cleanup (task 5) and that cleanup has not run yet or was refused
    /// because of hand edits.
    CleanupNeeded,
    /// Cleanup was attempted and failed (e.g. Windows file lock) -- task 5.5
    /// retries this, and the UI offers a manual retry meanwhile.
    CleanupFailed,
}

impl ResultState {
    /// Whether this state still needs the user's attention in a review
    /// surface, as opposed to being a terminal/background state.
    pub fn needs_review(self) -> bool {
        matches!(self, ResultState::Reviewing | ResultState::RevisionRequested)
    }
}

/// Why an execution ended, when it did not simply "finish" cleanly. Persisted
/// alongside a result record so a stopped/failed/conflicted execution still
/// has something to review (task 1.3) rather than vanishing with no result at
/// all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ResultOutcomeKind {
    /// The lead reported the work complete.
    Finished,
    /// The user (or a peer Stop all) stopped the execution before it
    /// finished.
    Stopped,
    /// The provider/engine reported a failure.
    Failed,
    /// A helper's integration hit a conflict with another helper or the base
    /// (architecture.md section 10: "A helper conflict pauses only that
    /// integration and preserves both sides").
    Conflicted,
}

/// The unified result record: everything a review/landing surface needs to
/// know about what one execution produced, as references into repository
/// truth.
///
/// One record per execution that reached a reviewable outcome. A session with
/// a lead plus helpers has one record per execution ID (task 2.2: "graph node
/// Output/View diff open its helper-scoped result"), plus -- once the lead
/// finishes -- a combined record whose `execution_id` is the lead's own
/// (`parent_execution_id: None` on the lead's `ExecutionRecord` is how a
/// reader tells "this is the combined/top-level result" from "this is one
/// helper's").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResultRecord {
    /// Which execution this result belongs to. Joins to
    /// `AgentSession.executions` by `execution_id`.
    pub execution_id: ExecutionId,
    pub outcome: ResultOutcomeKind,
    pub state: ResultState,
    /// Absolute path of the worktree the execution ran in. `None` for a
    /// read-only intent (Ask/Explain/Review/Summarize; see
    /// `agentdesk::policy::for_intent`), which never provisions one.
    pub worktree_path: Option<String>,
    /// The branch checked out in that worktree, when one exists.
    pub branch: Option<String>,
    /// The commit the worktree started from.
    pub base_oid: Option<String>,
    /// The worktree's HEAD when the result was captured. Equal to `base_oid`
    /// when nothing was committed inside the worktree itself (the common
    /// case: changes sit uncommitted until Keep -> Commit).
    pub head_oid: Option<String>,
    pub changed_paths: Vec<ResultChangedPath>,
    pub checks: Vec<ResultCheckOutcome>,
    /// Set once `agent_result_commit` (task 3.3) lands a commit for this
    /// result.
    pub commit: Option<ResultCommitRef>,
    /// The OpenSpec change this result should be linked to via the `Spec:`
    /// trailer, when the session's source or an attached task names one.
    pub openspec_change_id: Option<String>,
    /// For a COMBINED graph result only (`execution_id` is the lead's own):
    /// every helper execution ID whose work is folded into this record's
    /// `worktree_path` (P1 "Finished is not a combined graph result" --
    /// "link helper-scoped results"). A reviewer can still open each
    /// helper's own scoped `ResultRecord` (joined by these IDs) to see what
    /// that one helper individually produced, alongside the combined view.
    /// Empty for a solo or per-helper record.
    #[serde(default)]
    pub linked_execution_ids: Vec<ExecutionId>,
    /// RFC 3339 UTC timestamp this record was created or last updated.
    pub updated_at: String,
}

impl ResultRecord {
    /// A fresh `Reviewing` record for `execution_id` with no changes/checks
    /// recorded yet -- the shape `bridge`/`commands::agent_result` fill in as
    /// an execution's outcome becomes known.
    pub fn new_reviewing(execution_id: ExecutionId, outcome: ResultOutcomeKind, now: &str) -> Self {
        Self {
            execution_id,
            outcome,
            state: ResultState::Reviewing,
            worktree_path: None,
            branch: None,
            base_oid: None,
            head_oid: None,
            changed_paths: Vec::new(),
            checks: Vec::new(),
            commit: None,
            openspec_change_id: None,
            linked_execution_ids: Vec::new(),
            updated_at: now.to_string(),
        }
    }

    /// Whether there is anything at all to land: no worktree (a read-only
    /// intent never provisioned one) or a worktree with zero changed paths
    /// both mean "nothing to keep/commit". Used by `commands::agent_result`
    /// to refuse Keep/Commit on a read-only result rather than silently
    /// no-op-ing (design.md, policy.rs's `can_write` enforcement surface).
    pub fn has_landable_changes(&self) -> bool {
        self.worktree_path.is_some() && !self.changed_paths.is_empty()
    }
}

/// One session's result records on disk: `<store-root>/results/<session-id>.json`.
///
/// A sibling directory to `sessions/`, not a field inside the session file
/// itself -- see `SessionStoreRoot::root_path`'s doc comment for why: results
/// are read/written on a different, more frequent rhythm (every check
/// completion, every Keep/Undo/Commit click) than the transcript, and this
/// keeps those writes from contending with `SessionLocks`-guarded transcript
/// mutations or growing the transcript file with data that is never rendered
/// from it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct ResultsFile {
    schema_version: u16,
    session_id: SessionId,
    records: Vec<ResultRecord>,
}

const RESULTS_SCHEMA_VERSION: u16 = 1;
const RESULTS_DIR: &str = "results";

fn results_dir(root: &SessionStoreRoot) -> PathBuf {
    root.root_path().join(RESULTS_DIR)
}

fn results_path(root: &SessionStoreRoot, session_id: &str) -> PathBuf {
    results_dir(root).join(format!("{session_id}.json"))
}

/// Ensure `results/` exists under the store root. Cheap and idempotent;
/// called before every write rather than once at startup so a store root
/// created before this module existed still works without a migration step.
fn ensure_results_dir(root: &SessionStoreRoot) -> std::io::Result<()> {
    fs::create_dir_all(results_dir(root))
}

/// All result records persisted for `session_id`, oldest first. An empty
/// `Vec` (not an error) for a session that has never had an execution reach
/// a reviewable outcome yet -- that is the ordinary state for a brand-new or
/// still-running session, not a fault.
pub fn read_results(root: &SessionStoreRoot, session_id: &str) -> Result<Vec<ResultRecord>, SessionLoadError> {
    let path = results_path(root, session_id);
    match read_json_file(&path) {
        Ok(raw) => {
            let file: ResultsFile = serde_json::from_value(raw).map_err(|e| SessionLoadError::Malformed {
                detail: e.to_string(),
            })?;
            if file.schema_version > RESULTS_SCHEMA_VERSION {
                return Err(SessionLoadError::UnsupportedSchemaVersion {
                    found: file.schema_version,
                    max_supported: RESULTS_SCHEMA_VERSION,
                });
            }
            Ok(file.records)
        }
        // No results file yet is the ordinary "nothing to review" state, not
        // a load failure -- every other error (malformed, unsupported
        // version, unreadable) still propagates so a genuinely damaged file
        // is reported rather than silently treated as empty.
        Err(SessionLoadError::NotFound) => Ok(Vec::new()),
        Err(other) => Err(other),
    }
}

/// Overwrite every result record for `session_id`. Callers read-modify-write
/// under the same [`super::SessionLocks`] session lock used for the
/// transcript (result records and the transcript are two files but one
/// logical unit per session), so this itself does no locking.
pub fn write_results(
    root: &SessionStoreRoot,
    session_id: &str,
    records: &[ResultRecord],
) -> Result<(), WriteError> {
    ensure_results_dir(root).map_err(|e| WriteError::CreateTemp {
        dir: results_dir(root),
        detail: e.to_string(),
    })?;
    let file = ResultsFile {
        schema_version: RESULTS_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        records: records.to_vec(),
    };
    write_atomic(&results_path(root, session_id), &file)
}

/// Insert or replace the record for `execution_id`, preserving the position
/// of an existing entry (append for a new one). The one mutation primitive
/// every writer in `commands::agent_result` uses, so "two calls upsert the
/// same execution's record" always converges to one entry rather than
/// duplicating it.
pub fn upsert_result(records: &mut Vec<ResultRecord>, record: ResultRecord) {
    match records.iter_mut().find(|r| r.execution_id == record.execution_id) {
        Some(existing) => *existing = record,
        None => records.push(record),
    }
}

/// Find a session's result record for one execution, if any.
pub fn find_result<'a>(records: &'a [ResultRecord], execution_id: &str) -> Option<&'a ResultRecord> {
    records.iter().find(|r| r.execution_id == execution_id)
}

/// P1 "link helper-scoped results": sets `linked_execution_ids` on the
/// COMBINED result record named `combined_execution_id` (the lead's own) to
/// `helper_execution_ids`, so a reviewer of the combined result can still
/// open each individual helper's own scoped record.
///
/// Takes `locks` and holds this session's own read-modify-write lock for the
/// full read-modify-write (matches `commands::agent_result::build_result_at`'s
/// own locking shape, which this is meant to be called right after -- as two
/// separate lock acquisitions, not one, since `build_result_at` already
/// released its own lock by the time it returns `Built`).
///
/// A no-op (not an error) if the combined record does not exist yet -- the
/// caller (`commands::agent_graph::finish_graph_with_combined_result`) only
/// calls this once `build_result_at` has just confirmed it built one.
pub fn link_helper_results(
    locks: &super::SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    combined_execution_id: &str,
    helper_execution_ids: &[ExecutionId],
) -> Result<(), String> {
    locks.with_session_lock(session_id, || {
        let mut records = match read_results(root, session_id) {
            Ok(r) => r,
            Err(e) => return Err(e.to_string()),
        };
        let Some(existing) = records.iter_mut().find(|r| r.execution_id == combined_execution_id) else {
            return Ok(());
        };
        existing.linked_execution_ids = helper_execution_ids.to_vec();
        write_results(root, session_id, &records).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    fn temp_root() -> (tempfile::TempDir, SessionStoreRoot) {
        let dir = tempfile::tempdir().unwrap();
        let root = SessionStoreRoot::at(dir.path().to_path_buf()).unwrap();
        (dir, root)
    }

    #[test]
    fn a_session_with_no_results_file_reads_as_empty_not_an_error() {
        let (_dir, root) = temp_root();
        let records = read_results(&root, "sess-1").unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn writing_then_reading_round_trips_every_record() {
        let (_dir, root) = temp_root();
        let mut records = Vec::new();
        upsert_result(
            &mut records,
            ResultRecord::new_reviewing("exec-1".into(), ResultOutcomeKind::Finished, "2026-01-01T00:00:00Z"),
        );
        write_results(&root, "sess-1", &records).unwrap();

        let back = read_results(&root, "sess-1").unwrap();
        assert_eq!(back, records);
    }

    #[test]
    fn upsert_replaces_the_existing_record_for_the_same_execution() {
        let mut records = Vec::new();
        upsert_result(
            &mut records,
            ResultRecord::new_reviewing("exec-1".into(), ResultOutcomeKind::Finished, "2026-01-01T00:00:00Z"),
        );
        let mut updated = ResultRecord::new_reviewing(
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            "2026-01-01T00:05:00Z",
        );
        updated.state = ResultState::Kept;
        upsert_result(&mut records, updated.clone());

        assert_eq!(records.len(), 1, "must replace, not duplicate");
        assert_eq!(records[0], updated);
    }

    #[test]
    fn upsert_appends_a_new_execution_without_touching_others() {
        let mut records = Vec::new();
        upsert_result(
            &mut records,
            ResultRecord::new_reviewing("exec-1".into(), ResultOutcomeKind::Finished, "2026-01-01T00:00:00Z"),
        );
        upsert_result(
            &mut records,
            ResultRecord::new_reviewing("exec-2".into(), ResultOutcomeKind::Finished, "2026-01-01T00:01:00Z"),
        );
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].execution_id, "exec-1");
        assert_eq!(records[1].execution_id, "exec-2");
    }

    #[test]
    fn find_result_locates_by_execution_id() {
        let mut records = Vec::new();
        upsert_result(
            &mut records,
            ResultRecord::new_reviewing("exec-1".into(), ResultOutcomeKind::Finished, "2026-01-01T00:00:00Z"),
        );
        assert!(find_result(&records, "exec-1").is_some());
        assert!(find_result(&records, "exec-missing").is_none());
    }

    #[test]
    fn a_results_file_from_a_future_schema_version_is_refused() {
        let (_dir, root) = temp_root();
        let path = results_path(&root, "sess-1");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            serde_json::json!({
                "schemaVersion": RESULTS_SCHEMA_VERSION + 1,
                "sessionId": "sess-1",
                "records": [],
            })
            .to_string(),
        )
        .unwrap();

        let result = read_results(&root, "sess-1");
        assert!(matches!(
            result,
            Err(SessionLoadError::UnsupportedSchemaVersion { .. })
        ));
    }

    /// Same "damaged file is reported, not silently swallowed as empty"
    /// guarantee the session store gives (spec: "One damaged session").
    #[test]
    fn a_malformed_results_file_is_reported_not_treated_as_empty() {
        let (_dir, root) = temp_root();
        let path = results_path(&root, "sess-1");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not valid json").unwrap();

        let result = read_results(&root, "sess-1");
        assert!(matches!(result, Err(SessionLoadError::Malformed { .. })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_result_needs_review() {
        let r = ResultRecord::new_reviewing(
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            "2026-01-01T00:00:00Z",
        );
        assert!(r.state.needs_review());
        assert_eq!(r.state, ResultState::Reviewing);
    }

    #[test]
    fn a_result_with_no_worktree_has_nothing_to_land() {
        // Read-only intents (Ask/Explain/Review/Summarize) never provision a
        // worktree -- `worktree_path` stays `None` -- so there is nothing a
        // Keep/Commit action could act on. This is the fixture
        // `commands::agent_result`'s refusal tests build on.
        let r = ResultRecord::new_reviewing(
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            "2026-01-01T00:00:00Z",
        );
        assert!(!r.has_landable_changes());
    }

    #[test]
    fn a_result_with_a_worktree_but_no_changed_paths_has_nothing_to_land() {
        let mut r = ResultRecord::new_reviewing(
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            "2026-01-01T00:00:00Z",
        );
        r.worktree_path = Some("C:/code/repo-worktrees/exec-1".into());
        assert!(!r.has_landable_changes());
    }

    #[test]
    fn a_result_with_changed_paths_has_something_to_land() {
        let mut r = ResultRecord::new_reviewing(
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            "2026-01-01T00:00:00Z",
        );
        r.worktree_path = Some("C:/code/repo-worktrees/exec-1".into());
        r.changed_paths.push(ResultChangedPath {
            path: "src/a.rs".into(),
            old_path: None,
            status: "M".into(),
        });
        assert!(r.has_landable_changes());
    }

    #[test]
    fn every_result_state_round_trips_through_json() {
        let states = [
            ResultState::Reviewing,
            ResultState::RevisionRequested,
            ResultState::Kept,
            ResultState::Committed,
            ResultState::Discarded,
            ResultState::CleanupNeeded,
            ResultState::CleanupFailed,
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let back: ResultState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }

    #[test]
    fn every_outcome_kind_round_trips_through_json() {
        let kinds = [
            ResultOutcomeKind::Finished,
            ResultOutcomeKind::Stopped,
            ResultOutcomeKind::Failed,
            ResultOutcomeKind::Conflicted,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let back: ResultOutcomeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn a_full_result_record_round_trips_through_json() {
        let mut r = ResultRecord::new_reviewing(
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            "2026-01-01T00:00:00Z",
        );
        r.worktree_path = Some("C:/code/repo-worktrees/exec-1".into());
        r.branch = Some("agent/exec-1".into());
        r.base_oid = Some("abc123".into());
        r.head_oid = Some("abc123".into());
        r.changed_paths.push(ResultChangedPath {
            path: "src/a.rs".into(),
            old_path: None,
            status: "M".into(),
        });
        r.checks.push(ResultCheckOutcome {
            command_name: "npm run typecheck".into(),
            outcome: CheckRunOutcome::Passed,
            summary: Some("no errors".into()),
        });
        r.commit = Some(ResultCommitRef {
            oid: "def456".into(),
            subject: "new: add the thing".into(),
        });
        r.openspec_change_id = Some("add-thing".into());

        let json = serde_json::to_string_pretty(&r).unwrap();
        let back: ResultRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn only_reviewing_and_revision_requested_need_review() {
        assert!(ResultState::Reviewing.needs_review());
        assert!(ResultState::RevisionRequested.needs_review());
        for state in [
            ResultState::Kept,
            ResultState::Committed,
            ResultState::Discarded,
            ResultState::CleanupNeeded,
            ResultState::CleanupFailed,
        ] {
            assert!(!state.needs_review(), "{state:?} should not need review");
        }
    }
}
