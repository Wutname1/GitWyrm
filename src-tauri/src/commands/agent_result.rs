//! Commands for the Agent Desk review/landing package: building the unified
//! result state, reviewing it against GitWyrm's existing diff/status data,
//! and the Keep/Undo/Commit/PR-handoff/cleanup actions that follow.
//!
//! Everything here is deliberately thin over existing systems
//! (docs/agent-desk/architecture.md section 1's reuse boundary): computing
//! "what changed" reads the worktree's live status
//! (`git::types::working_status`, the same function `commands::status::get_status`
//! uses); committing goes through `git::commit_write::create` with the same
//! signing behavior every other commit path gets; removing a worktree goes
//! through `git::worktree::remove`; nothing here re-implements any of them.
//!
//! **No command in this file ever pushes or posts to a host.** Task 4.3/4.5:
//! PR creation drafts editable text and opens the host's own compare/new-PR
//! page in the user's browser -- the actual publish/push stays the existing,
//! separate, user-initiated `git_push` action GitWyrm already has.
//!
//! This used to say "grep for `git_push` or `HostProvider` in this file",
//! which named the check without ever running it -- the product's strongest
//! safety claim resting on whoever remembered to look.
//! `no_command_here_pushes_or_posts_on_the_users_behalf` in this file's tests
//! now reads this source and fails on a call that transmits anything, so the
//! promise breaks loudly rather than quietly. The two symbols above appear in
//! this comment on purpose; the test ignores comment lines.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::agentdesk::model::{AgentSession, ExecutionId, SessionId, SessionLoadError};
use crate::agentdesk::result::{
    self, ResultChangedPath, ResultCheckOutcome, ResultCommitRef, ResultOutcomeKind, ResultRecord,
    ResultState,
};
use crate::agentdesk::store::SessionStoreRoot;
use crate::agentdesk::{policy, SessionLocks};
use crate::error::AppError;
use crate::git::commit_write::{self, CommitIdentity};
use crate::git::trailers;
use crate::git::types::StatusCode;
use crate::git::worktree::{self, DirtyChoice, RemoveOutcome};

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn resolve_root(app: &AppHandle) -> Result<SessionStoreRoot, AppError> {
    SessionStoreRoot::resolve(app).map_err(|e| AppError::Other(e.to_string()))
}

fn status_code_label(code: StatusCode) -> &'static str {
    match code {
        StatusCode::Added => "A",
        StatusCode::Modified => "M",
        StatusCode::Deleted => "D",
        StatusCode::Renamed => "R",
        StatusCode::Conflicted => "!",
    }
}

// -- 1.2: build result records from existing completion state --

/// Read the changed-path list for a worktree by opening it directly with
/// git2 and reusing the same working-tree status walk
/// `commands::status::get_status` uses (`git::types` is the shared type,
/// this just calls the underlying scan directly rather than through
/// `RepoManager`, since a helper's worktree is frequently not a repo the
/// main window has opened).
///
/// Never reads file contents or diff text -- only path/status, matching the
/// result model's "references, not copies" rule.
/// Paths present in a worktree now that this result never recorded.
///
/// Compared as a set of (path, status) so a different `statuses()` ordering
/// -- which git2 does not promise is stable -- can never read as a change.
/// `None` means the worktree holds exactly what the agent left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnexpectedPaths {
    modified: u32,
    untracked: u32,
}

fn count_unexpected_paths(recorded: &[ResultChangedPath], live: &[ResultChangedPath]) -> Option<UnexpectedPaths> {
    use std::collections::BTreeSet;
    let known: BTreeSet<(&str, &str)> = recorded
        .iter()
        .map(|p| (p.path.as_str(), p.status.as_str()))
        .collect();

    let mut out = UnexpectedPaths { modified: 0, untracked: 0 };
    for entry in live {
        if known.contains(&(entry.path.as_str(), entry.status.as_str())) {
            continue;
        }
        // "A" is this module's own code for an added/untracked path
        // (`StatusCode::Added`); everything else counts as a modification.
        // Untracked is called out separately because it is the one with no
        // way back -- a file never written to history cannot be recovered.
        if entry.status == "A" {
            out.untracked += 1;
        } else {
            out.modified += 1;
        }
    }
    if out.modified == 0 && out.untracked == 0 {
        None
    } else {
        Some(out)
    }
}

fn changed_paths_for_worktree(worktree_path: &Path) -> Result<Vec<ResultChangedPath>, AppError> {
    let repo = git2::Repository::open(worktree_path)
        .map_err(|e| AppError::Other(format!("could not open worktree: {e}")))?;

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| AppError::Other(format!("could not read worktree status: {e}")))?;

    let mut out = Vec::new();
    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        let path = path.to_string();
        let status = entry.status();
        let code = if status.is_conflicted() {
            StatusCode::Conflicted
        } else if status.is_wt_new() || status.is_index_new() {
            StatusCode::Added
        } else if status.is_wt_deleted() || status.is_index_deleted() {
            StatusCode::Deleted
        } else if status.is_wt_renamed() || status.is_index_renamed() {
            StatusCode::Renamed
        } else {
            StatusCode::Modified
        };
        out.push(ResultChangedPath {
            path: path.to_string(),
            old_path: None,
            status: status_code_label(code).to_string(),
        });
    }
    Ok(out)
}

/// Reads back every `RunStep::Check` this execution reported into the
/// transcript, in order, as [`ResultCheckOutcome`]s -- R3.7's "changed files,
/// checks, worktree path, base/head revisions" for the automatic result
/// build. `SessionMessage.rendered_content` already carries the full step
/// losslessly (see `agentdesk::bridge::map_run_step`'s doc comment), so this
/// is a pure re-read of what the transcript already has, not a new source of
/// truth -- exactly the "references into repository truth" stance this
/// module's own doc comment describes for `changed_paths`/`checks`.
pub(crate) fn checks_for_execution(session: &AgentSession, execution_id: &str) -> Vec<ResultCheckOutcome> {
    session
        .messages
        .iter()
        .filter(|m| m.execution_id.as_deref() == Some(execution_id))
        .filter_map(|m| {
            let rendered = m.rendered_content.as_deref()?;
            let step: crate::airun::driver::RunStep = serde_json::from_str(rendered).ok()?;
            match step {
                // A check that reported no name is dropped rather than
                // recorded. The name is what a completion condition is
                // matched against, and a nameless one cannot answer "did it
                // make `cargo test` pass?" either way -- keeping it would
                // record evidence that names nothing. `detail` on the next
                // line is already dropped when blank, for the same reason.
                crate::airun::driver::RunStep::Check { name, .. } if name.trim().is_empty() => None,
                crate::airun::driver::RunStep::Check { name, passed, detail } => Some(ResultCheckOutcome {
                    command_name: name,
                    outcome: if passed {
                        crate::agentdesk::result::CheckRunOutcome::Passed
                    } else {
                        crate::agentdesk::result::CheckRunOutcome::Failed
                    },
                    summary: if detail.trim().is_empty() { None } else { Some(detail) },
                    // Straight out of the agent's own transcript: `passed` is
                    // what it told us, and nothing here re-ran the command.
                    source: crate::agentdesk::result::CheckEvidenceSource::AgentReported,
                }),
                _ => None,
            }
        })
        .collect()
}

/// What building/refreshing a result record found.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BuildResultOutcome {
    Built { record: ResultRecord },
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
    WriteFailed { detail: String },
    /// The execution named is not one this session knows about.
    ExecutionNotFound,
}

/// Build (or refresh) the unified result record for one execution: reads the
/// worktree's current changed-path list from disk and upserts it into the
/// session's result sidecar. Idempotent -- calling this again after more
/// edits land just refreshes `changed_paths`/`head_oid`, never duplicating
/// the record (task 1.2).
///
/// `worktree_path`/`branch`/`base_oid` are supplied by the caller (the
/// execution-start/completion path that already knows them from
/// `git::worktree::add`) rather than re-derived here -- a read-only intent
/// passes `None` for `worktree_path`, which is exactly what
/// `ResultRecord::has_landable_changes` keys off.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_result_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: ExecutionId,
    outcome: ResultOutcomeKind,
    worktree_path: Option<String>,
    branch: Option<String>,
    base_oid: Option<String>,
    checks: Vec<ResultCheckOutcome>,
    openspec_change_id: Option<String>,
) -> BuildResultOutcome {
    use SessionLoadError as E;

    // Confirm the session/execution exist before doing any filesystem work.
    let exists = locks.with_session_lock(session_id, || {
        crate::agentdesk::store::read_session(root, session_id)
    });
    match &exists {
        Ok(session) => {
            if !session.executions.iter().any(|e| e.execution_id == execution_id) {
                return BuildResultOutcome::ExecutionNotFound;
            }
        }
        Err(E::NotFound) => return BuildResultOutcome::SessionNotFound,
        Err(E::Io { detail }) => {
            return BuildResultOutcome::SessionUnavailable {
                detail: detail.clone(),
            }
        }
        Err(reason) => {
            return BuildResultOutcome::SessionDamaged {
                reason: reason.to_string(),
            }
        }
    }

    // A worktree that could not be read is recorded as unread, not as empty.
    // `unwrap_or_default()` used to make those two the same value, and every
    // reader downstream took the empty list at face value -- see
    // `ResultRecord::changed_paths_unreadable` for what that cost.
    //
    // No worktree at all is a different thing again, and genuinely empty: a
    // read-only intent never provisions one, so there is nothing that could
    // have changed. That stays `None`.
    let (changed_paths, changed_paths_unreadable) = match &worktree_path {
        Some(p) => match changed_paths_for_worktree(Path::new(p)) {
            Ok(paths) => (paths, None),
            Err(e) => {
                log::warn!("agent desk: could not read the worktree for a result: {e}");
                (Vec::new(), Some(e.to_string()))
            }
        },
        None => (Vec::new(), None),
    };
    let head_oid = worktree_path.as_deref().and_then(|p| {
        let repo = git2::Repository::open(p).ok()?;
        let head = repo.head().ok()?;
        let commit = head.peel_to_commit().ok()?;
        Some(commit.id().to_string())
    });

    let now = now_rfc3339();
    let mut record = ResultRecord::new_reviewing(execution_id, outcome, &now);
    record.worktree_path = worktree_path;
    record.branch = branch;
    record.base_oid = base_oid;
    record.head_oid = head_oid;
    record.changed_paths = changed_paths;
    record.changed_paths_unreadable = changed_paths_unreadable;
    record.checks = checks;
    record.openspec_change_id = openspec_change_id;

    let write = locks.with_session_lock(session_id, || {
        let mut records = match result::read_results(root, session_id) {
            Ok(r) => r,
            Err(E::NotFound) => Vec::new(),
            Err(e) => return Err(e.to_string()),
        };
        // Preserve state/commit set by a later Keep/Undo/Commit if this is
        // only a refresh of an already-decided result -- a re-run of the
        // change scan must never regress `Kept`/`Committed` back to
        // `Reviewing`.
        if let Some(existing) = result::find_result(&records, &record.execution_id) {
            if !matches!(existing.state, ResultState::Reviewing | ResultState::RevisionRequested) {
                record.state = existing.state;
                record.commit = existing.commit.clone();
            }
        }
        result::upsert_result(&mut records, record.clone());
        result::write_results(root, session_id, &records).map_err(|e| e.to_string())
    });

    match write {
        Ok(()) => BuildResultOutcome::Built { record },
        Err(detail) => BuildResultOutcome::WriteFailed { detail },
    }
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn agent_result_build(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    session_id: SessionId,
    execution_id: ExecutionId,
    outcome: ResultOutcomeKind,
    worktree_path: Option<String>,
    branch: Option<String>,
    base_oid: Option<String>,
    checks: Vec<ResultCheckOutcome>,
    openspec_change_id: Option<String>,
) -> Result<BuildResultOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        build_result_at(
            &locks_arc,
            &root,
            &session_id,
            execution_id,
            outcome,
            worktree_path,
            branch,
            base_oid,
            checks,
            openspec_change_id,
        )
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

// -- 2.1/2.2: list results for review --

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ListResultsOutcome {
    Found { records: Vec<ResultRecord> },
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
}

fn list_results_at(root: &SessionStoreRoot, session_id: &str) -> ListResultsOutcome {
    use SessionLoadError as E;
    match result::read_results(root, session_id) {
        Ok(records) => ListResultsOutcome::Found { records },
        Err(E::NotFound) => ListResultsOutcome::Found { records: Vec::new() },
        Err(E::Io { detail }) => ListResultsOutcome::SessionUnavailable { detail },
        Err(reason) => ListResultsOutcome::SessionDamaged {
            reason: reason.to_string(),
        },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_list(
    app: AppHandle,
    session_id: SessionId,
) -> Result<ListResultsOutcome, AppError> {
    let root = resolve_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || list_results_at(&root, &session_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

// -- 3.1/3.2: Keep / Undo, routed through existing worktree/dirty-count
//    behavior rather than a second implementation --

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeepResultOutcome {
    Kept { record: ResultRecord },
    /// Nothing to keep: no worktree, or a worktree with zero changes (a
    /// read-only intent's result, or a helper that made no edits).
    NothingToKeep,
    /// The changed-file list was never read, so what would be kept is
    /// unknown. Distinct from `NothingToKeep`, which is a measurement --
    /// this one is the absence of one, and answering it with "nothing to
    /// keep" would state as fact something GitWyrm never checked.
    WorktreeUnreadable { detail: String },
    ResultNotFound,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
    WriteFailed { detail: String },
}

fn keep_result_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: &str,
) -> KeepResultOutcome {
    use SessionLoadError as E;
    locks.with_session_lock(session_id, || {
        let mut records = match result::read_results(root, session_id) {
            Ok(r) => r,
            Err(E::NotFound) => return KeepResultOutcome::SessionNotFound,
            Err(E::Io { detail }) => return KeepResultOutcome::SessionUnavailable { detail },
            Err(reason) => {
                return KeepResultOutcome::SessionDamaged {
                    reason: reason.to_string(),
                }
            }
        };
        let Some(existing) = records.iter().position(|r| r.execution_id == execution_id) else {
            return KeepResultOutcome::ResultNotFound;
        };
        // Asked before the emptiness check, because an unreadable worktree
        // has an empty list for a reason that is not emptiness.
        if let Some(detail) = records[existing].changed_paths_unreadable.clone() {
            return KeepResultOutcome::WorktreeUnreadable { detail };
        }
        if !records[existing].has_landable_changes() {
            return KeepResultOutcome::NothingToKeep;
        }
        records[existing].state = ResultState::Kept;
        records[existing].updated_at = now_rfc3339();
        let record = records[existing].clone();
        match result::write_results(root, session_id, &records) {
            Ok(()) => KeepResultOutcome::Kept { record },
            Err(e) => KeepResultOutcome::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_keep(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    session_id: SessionId,
    execution_id: ExecutionId,
) -> Result<KeepResultOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        keep_result_at(&locks_arc, &root, &session_id, &execution_id)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// What Undo did to a result's worktree. Mirrors
/// `commands::airun::RunDiscardPlan`/`git::worktree::RemoveOutcome`'s shape:
/// hand-edited work is never silently thrown away (task 3.2).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UndoResultOutcome {
    /// The worktree had only what the agent wrote; it was discarded.
    Discarded { record: ResultRecord },
    /// The user (or someone) edited the worktree by hand since the agent
    /// finished. Nothing was deleted -- the result stays `Reviewing` and the
    /// caller should offer Open instead (task 3.2/5.2).
    RefusedHandEdited {
        record: ResultRecord,
        modified: u32,
        untracked: u32,
    },
    NothingToUndo,
    ResultNotFound,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
    WriteFailed { detail: String },
    /// Refused: the result's state moved between the peek (which decided
    /// this worktree was clean and safe to discard) and the locked write --
    /// most commonly a concurrent `agent_result_commit` landed a real commit
    /// in between. Discarding now would stamp `Discarded` over a `Committed`
    /// record and orphan that commit from every session record (the defect
    /// this fix closes). Nothing was written; the caller should re-read the
    /// current state instead of retrying blindly.
    StateChanged { record: ResultRecord },
}

fn undo_result_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: &str,
) -> UndoResultOutcome {
    use SessionLoadError as E;

    // Read the record (and its worktree path) first, outside the lock the
    // dirty-count filesystem scan doesn't need to hold.
    let peek = locks.with_session_lock(session_id, || result::read_results(root, session_id));
    let records = match peek {
        Ok(r) => r,
        Err(E::NotFound) => return UndoResultOutcome::SessionNotFound,
        Err(E::Io { detail }) => return UndoResultOutcome::SessionUnavailable { detail },
        Err(reason) => {
            return UndoResultOutcome::SessionDamaged {
                reason: reason.to_string(),
            }
        }
    };
    let Some(record) = records.iter().find(|r| r.execution_id == execution_id).cloned() else {
        return UndoResultOutcome::ResultNotFound;
    };
    let peeked_state = record.state;
    let Some(worktree_path) = &record.worktree_path else {
        return UndoResultOutcome::NothingToUndo;
    };

    // Hand-edit detection.
    //
    // This used to ask `dirty_count` "is anything uncommitted here?", which
    // is the wrong question: an agent's own output IS uncommitted (see
    // `changed_paths_for_worktree` -- the result's file list is built from
    // exactly the same `repo.statuses()` walk, and Keep only flips state
    // without committing). So Undo refused on every result that changed
    // anything, telling the person their own agent's output was "hand-edited"
    // -- which made the safety net the whole review design leans on
    // unreachable.
    //
    // The question that actually needs answering is "is anything here that
    // the agent did not leave?", so the live worktree is compared against the
    // set this result recorded when it was built. An unreadable worktree is
    // still treated as touched: the same safe-direction-to-be-wrong-in stance
    // `commands::airun::ai_run_discard_plan` takes.
    //
    // KNOWN LIMIT, recorded rather than hidden: this compares paths and their
    // status, not content. Editing a file the agent already changed leaves
    // the set identical and is not detected. Closing that needs the agent's
    // work committed inside its own worktree so a content diff has something
    // to compare against -- see `docs/agent-desk/audit-2026-09-03.md`.
    let unexpected = match changed_paths_for_worktree(Path::new(worktree_path)) {
        Ok(live) => count_unexpected_paths(&record.changed_paths, &live),
        Err(_) => Some(UnexpectedPaths { modified: 1, untracked: 0 }),
    };
    if let Some(unexpected) = unexpected {
        return UndoResultOutcome::RefusedHandEdited {
            record,
            modified: unexpected.modified,
            untracked: unexpected.untracked,
        };
    }

    locks.with_session_lock(session_id, || {
        let mut records = match result::read_results(root, session_id) {
            Ok(r) => r,
            Err(E::NotFound) => return UndoResultOutcome::SessionNotFound,
            Err(E::Io { detail }) => return UndoResultOutcome::SessionUnavailable { detail },
            Err(reason) => {
                return UndoResultOutcome::SessionDamaged {
                    reason: reason.to_string(),
                }
            }
        };
        let Some(entry) = records.iter_mut().find(|r| r.execution_id == execution_id) else {
            return UndoResultOutcome::ResultNotFound;
        };
        // Re-check under THIS lock that the state is still what the peek
        // observed -- the peek's dirty-count scan ran unlocked, so a
        // concurrent `agent_result_commit` could have landed a real commit
        // (setting state to `Committed`) in the window between the peek and
        // this write. Discarding now would silently overwrite that outcome
        // and clear `changed_paths` on a record a commit already accounts
        // for, so refuse instead (mirrors `record_execution_if_not_running`
        // in `commands::agent_desk`). Also refuse outright if the state was
        // ALREADY `Committed` at peek time (a stale UI still offering Undo
        // after a commit it does not know about yet) -- a real commit
        // exists in git history either way, and Undo must never stamp
        // `Discarded` over that regardless of which side of the peek the
        // commit landed on.
        if entry.state != peeked_state || entry.state == ResultState::Committed {
            return UndoResultOutcome::StateChanged {
                record: entry.clone(),
            };
        }
        entry.state = ResultState::Discarded;
        entry.changed_paths.clear();
        entry.updated_at = now_rfc3339();
        let record = entry.clone();
        match result::write_results(root, session_id, &records) {
            Ok(()) => UndoResultOutcome::Discarded { record },
            Err(e) => UndoResultOutcome::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_undo(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    session_id: SessionId,
    execution_id: ExecutionId,
) -> Result<UndoResultOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        undo_result_at(&locks_arc, &root, &session_id, &execution_id)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

// -- 2.5: "Review requested changes" appends a new lead step rather than
//    mutating the finished result in place --

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RequestRevisionOutcome {
    Requested { record: ResultRecord },
    ResultNotFound,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
    WriteFailed { detail: String },
}

/// Mark a result `RevisionRequested`. The caller is responsible for actually
/// appending the follow-up user message via the existing
/// `agent_session_append_user_message` command and starting a new execution
/// through `agent_session_start_execution` -- this command only flips the
/// result's own state so a review surface stops offering Keep/Commit on a
/// result the user has already said needs more work.
fn request_revision_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: &str,
) -> RequestRevisionOutcome {
    use SessionLoadError as E;
    locks.with_session_lock(session_id, || {
        let mut records = match result::read_results(root, session_id) {
            Ok(r) => r,
            Err(E::NotFound) => return RequestRevisionOutcome::SessionNotFound,
            Err(E::Io { detail }) => return RequestRevisionOutcome::SessionUnavailable { detail },
            Err(reason) => {
                return RequestRevisionOutcome::SessionDamaged {
                    reason: reason.to_string(),
                }
            }
        };
        let Some(entry) = records.iter_mut().find(|r| r.execution_id == execution_id) else {
            return RequestRevisionOutcome::ResultNotFound;
        };
        entry.state = ResultState::RevisionRequested;
        entry.updated_at = now_rfc3339();
        let record = entry.clone();
        match result::write_results(root, session_id, &records) {
            Ok(()) => RequestRevisionOutcome::Requested { record },
            Err(e) => RequestRevisionOutcome::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_request_revision(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    session_id: SessionId,
    execution_id: ExecutionId,
) -> Result<RequestRevisionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        request_revision_at(&locks_arc, &root, &session_id, &execution_id)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

// -- 3.3/3.4: intentional commit with source/OpenSpec trailers --

/// A draft commit message for a kept result: subject plus trailers,
/// generated the same way `commands::airun::ai_run_completion` does
/// (`airun::complete::commit_message`) so a run started from Agent Desk and
/// one started from the old task-run console read identically in history.
/// **Read-only** -- building this never writes a commit; see
/// `agent_result_commit` for the explicit action that does (task 3.4:
/// "Never auto-commit merely because the lead says finished").
#[tauri::command]
#[specta::specta]
pub async fn agent_result_draft_commit_message(
    app: AppHandle,
    session_id: SessionId,
    execution_id: ExecutionId,
    task_text: String,
    provider: String,
) -> Result<Option<String>, AppError> {
    let root = resolve_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let records = result::read_results(&root, &session_id).unwrap_or_default();
        let record = result::find_result(&records, &execution_id)?;
        let change_id = record.openspec_change_id.clone().unwrap_or_default();
        Some(crate::airun::complete::commit_message(
            &task_text, &change_id, &provider,
        ))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommitResultOutcome {
    Committed { record: ResultRecord, oid: String },
    /// Refused: the intent this session's result came from cannot write
    /// (`agentdesk::policy::for_intent`'s `can_write`) -- a Review/Summarize
    /// result has nothing to commit by design, not by accident.
    ReadOnlyIntent,
    NothingToCommit,
    ResultNotFound,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
    WriteFailed { detail: String },
    /// The worktree could not be opened, or committing in it failed --
    /// `detail` carries the underlying git error (may include a signing
    /// failure message from `git::commit_write`, which already tells the
    /// user what to do).
    GitFailed { detail: String },
    MessageRequired,
    /// Refused: the result's state moved between the peek (which required
    /// `Kept`) and the locked write that would have stamped `Committed` --
    /// most commonly a concurrent Undo discarded this same result while the
    /// git commit above was running. The commit was already created in git
    /// history (it cannot be un-created here), but the session record is
    /// left as-is rather than overwriting whatever the other caller wrote,
    /// so the caller should surface `oid` to the user as a commit that now
    /// needs manual reconciliation with the record.
    RecordStateChanged { record: ResultRecord, oid: String },
}

/// Create an intentional commit for a kept result. Never called
/// automatically -- the caller is a user clicking Commit after reviewing the
/// drafted message (task 3.3/3.4). Requires `ResultState::Kept`: committing
/// straight from `Reviewing` would skip the Keep step whose whole purpose is
/// "the user looked at this and it is good."
///
/// Routes through `git::commit_write::create`, the same signing-aware path
/// `commands::commit::create_commit` uses, so an Agent Desk commit is signed
/// exactly when any other commit in this repository would be -- there is no
/// second, unsigned commit path here.
fn commit_result_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: &str,
    intent: crate::agentdesk::model::SessionIntent,
    message: &str,
) -> CommitResultOutcome {
    use SessionLoadError as E;

    if !policy::for_intent(intent).can_write {
        return CommitResultOutcome::ReadOnlyIntent;
    }
    if message.trim().is_empty() {
        return CommitResultOutcome::MessageRequired;
    }

    let peek = locks.with_session_lock(session_id, || result::read_results(root, session_id));
    let records = match peek {
        Ok(r) => r,
        Err(E::NotFound) => return CommitResultOutcome::SessionNotFound,
        Err(E::Io { detail }) => return CommitResultOutcome::SessionUnavailable { detail },
        Err(reason) => {
            return CommitResultOutcome::SessionDamaged {
                reason: reason.to_string(),
            }
        }
    };
    let Some(record) = records.iter().find(|r| r.execution_id == execution_id).cloned() else {
        return CommitResultOutcome::ResultNotFound;
    };
    if record.state != ResultState::Kept {
        return CommitResultOutcome::NothingToCommit;
    }
    let Some(worktree_path) = &record.worktree_path else {
        return CommitResultOutcome::NothingToCommit;
    };

    let commit_result: Result<git2::Oid, String> = (|| {
        let repo = git2::Repository::open(worktree_path).map_err(|e| e.to_string())?;
        let repo_path = commit_write::workdir_of(&repo);
        let signature = repo.signature().map_err(|_| {
            "Git does not know your name and email yet. Add them in Settings > General, then commit again.".to_string()
        })?;

        let mut index = repo.index().map_err(|e| e.to_string())?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).map_err(|e| e.to_string())?;
        index.write().map_err(|e| e.to_string())?;
        let tree_oid = index.write_tree().map_err(|e| e.to_string())?;
        let tree = repo.find_tree(tree_oid).map_err(|e| e.to_string())?;

        let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        if let Some(p) = &head_commit {
            if p.tree_id() == tree_oid {
                return Err("nothing staged to commit".to_string());
            }
        }
        let parents: Vec<&git2::Commit> = head_commit.iter().collect();

        let identity = CommitIdentity {
            author: signature.clone(),
            committer: signature,
        };
        commit_write::create(&repo, &repo_path, Some("HEAD"), &identity, message, &tree, &parents)
            .map_err(|e| e.to_string())
    })();

    let oid = match commit_result {
        Ok(oid) => oid,
        Err(detail) => return CommitResultOutcome::GitFailed { detail },
    };

    let subject = message.lines().next().unwrap_or_default().to_string();
    commit_record_if_still_kept(locks, root, session_id, execution_id, oid, &subject)
}

/// What happened when `commit_record_if_still_kept` tried to stamp a
/// record `Committed`. Mirrors `commands::agent_graph::CommitGraphOutcome`'s
/// shape for the same reason: an internal, non-`Type` enum whose only job is
/// to let the caller distinguish "wrote successfully" from "refused because
/// the state moved" from "write itself failed" -- `CommitResultOutcome`
/// carries the public/`Type` version of the same three cases.
enum CommitRecordOutcome {
    Committed(Vec<ResultRecord>),
    StateChanged(ResultRecord),
    Err(String),
}

/// Atomically re-checks "is this record still `Kept`" and, if so, stamps it
/// `Committed` with the given commit reference -- both inside ONE
/// `with_session_lock` acquisition, so no concurrent `undo_result_at` call
/// for the same execution can slip a `Discarded` write in between the check
/// and this write (mirrors `record_execution_if_not_running` in
/// `commands::agent_desk`, and `undo_result_at`'s matching re-check for the
/// opposite direction). Split out from `commit_result_at` so this specific
/// re-check-and-write step can be raced directly by a test with two real
/// threads -- `commit_result_at` as a whole cannot be raced deterministically
/// because most of it is a real, variable-latency git commit.
fn commit_record_if_still_kept(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    execution_id: &str,
    oid: git2::Oid,
    subject: &str,
) -> CommitResultOutcome {
    let write = locks.with_session_lock(session_id, || {
        let mut records = match result::read_results(root, session_id) {
            Ok(r) => r,
            Err(e) => return CommitRecordOutcome::Err(e.to_string()),
        };
        let Some(entry) = records.iter_mut().find(|r| r.execution_id == execution_id) else {
            return CommitRecordOutcome::Err("result vanished during commit".to_string());
        };
        if entry.state != ResultState::Kept {
            return CommitRecordOutcome::StateChanged(entry.clone());
        }
        entry.state = ResultState::Committed;
        entry.commit = Some(ResultCommitRef {
            oid: oid.to_string(),
            subject: subject.to_string(),
        });
        entry.updated_at = now_rfc3339();
        if let Err(e) = result::write_results(root, session_id, &records) {
            return CommitRecordOutcome::Err(e.to_string());
        }
        CommitRecordOutcome::Committed(records)
    });

    match write {
        CommitRecordOutcome::Committed(records) => {
            let record = result::find_result(&records, execution_id)
                .cloned()
                .unwrap_or_else(|| ResultRecord::new_reviewing(execution_id.to_string(), ResultOutcomeKind::Finished, &now_rfc3339()));
            CommitResultOutcome::Committed {
                record,
                oid: oid.to_string(),
            }
        }
        CommitRecordOutcome::StateChanged(record) => CommitResultOutcome::RecordStateChanged {
            record,
            oid: oid.to_string(),
        },
        CommitRecordOutcome::Err(detail) => CommitResultOutcome::WriteFailed { detail },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_commit(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    session_id: SessionId,
    execution_id: ExecutionId,
    intent: crate::agentdesk::model::SessionIntent,
    message: String,
) -> Result<CommitResultOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        commit_result_at(&locks_arc, &root, &session_id, &execution_id, intent, &message)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Confirms a message carries the `Spec:`/`Assisted-by:` trailers this
/// module composes, for callers that build their own message text (e.g. an
/// edited draft) rather than using `agent_result_draft_commit_message`
/// verbatim.
#[tauri::command]
#[specta::specta]
pub async fn agent_result_message_trailers(
    message: String,
) -> Result<(Option<String>, Option<String>), AppError> {
    Ok((trailers::spec_id(&message), trailers::assisted_by(&message)))
}

// -- 2.2: open a result's diff in GitWyrm's existing diff view --
//
// Agent Desk (`AGENT_DESK_LABEL`, see `commands::spec_desk`) is a standalone
// window with no embedded diff viewer -- `src/lib/agentDeskTargets.ts`
// documents this: every `diff`/`file` message target resolves to
// `unavailable` there today because "no navigation bridge between the two
// exists yet." This command IS that bridge for a result's changed files: it
// focuses the MAIN window (label `"main"`, matching `lib.rs`'s
// `app.get_webview_window("main")` usage) and emits an event the main
// window's `App.tsx` listens for, carrying enough to open the worktree's
// working-tree diff for one path in the existing `DiffView`/`uiStore`
// machinery -- no new diff renderer, no copied diff text crossing the IPC
// boundary.
pub const OPEN_RESULT_DIFF_EVENT: &str = "agent-result://open-diff";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenResultDiffTarget {
    pub worktree_path: String,
    /// `None` opens the changed-file list itself (the caller lands on
    /// whatever the main window shows for "no file selected yet" in that
    /// worktree); `Some(path)` jumps straight to one file's diff.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenResultDiffOutcome {
    /// The main window was found and told to open the diff.
    Opened,
    /// No main window exists yet (a very early startup race). The caller
    /// should not treat this as a hard failure -- there is nothing this
    /// command can do about a window that has not been created.
    MainWindowNotOpen,
}

/// Focuses the main window and asks it to open one result's worktree diff.
/// Never opens a second main window, never creates any window itself --
/// mirrors `commands::spec_desk::open_spec_desk`'s "focus what already
/// exists" shape, aimed the other direction (Agent Desk -> main).
#[tauri::command]
#[specta::specta]
pub async fn agent_result_open_diff(
    app: AppHandle,
    worktree_path: String,
    path: Option<String>,
) -> Result<OpenResultDiffOutcome, AppError> {
    use tauri::{Emitter, Manager};
    let Some(main) = app.get_webview_window("main") else {
        return Ok(OpenResultDiffOutcome::MainWindowNotOpen);
    };
    let _ = main.unminimize();
    let _ = main.show();
    let _ = main.set_focus();
    let _ = app.emit_to(
        "main",
        OPEN_RESULT_DIFF_EVENT,
        &OpenResultDiffTarget { worktree_path, path },
    );
    Ok(OpenResultDiffOutcome::Opened)
}

// -- 4: PR creation/handoff, separate and explicit, never pushing --

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftPullRequest {
    pub title: String,
    pub body: String,
    /// The host's own "open a new PR" page, built from `web_base`/branch
    /// helpers already in `git::remote_url` -- the same URL a user would
    /// reach by clicking "Compare & pull request" on the host's site. Opening
    /// it is the entire "create PR" action here: GitWyrm drafts the text,
    /// the user reviews and submits it on the host, and no GitWyrm code ever
    /// calls a host write API or a push. `None` when the remote is not a
    /// host this app recognizes.
    pub compare_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DraftPullRequestOutcome {
    Drafted { draft: DraftPullRequest },
    ResultNotFound,
    NoCommit,
    NoRemote,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
}

/// Build editable PR title/body text and a compare-URL for a committed
/// result. Requires `ResultState::Committed` -- drafting a PR for work that
/// was never committed would invite publishing something the user never
/// explicitly landed (task 3.4/4's shared "explicit action" stance).
///
/// This command **only reads** the repository (remote URL, branch name) and
/// **never** shells out to git or a host API -- see this file's module doc
/// for the full "never pushes" guarantee.
fn draft_pull_request_at(root: &SessionStoreRoot, session_id: &str, execution_id: &str) -> DraftPullRequestOutcome {
    use SessionLoadError as E;
    let records = match result::read_results(root, session_id) {
        Ok(r) => r,
        Err(E::NotFound) => return DraftPullRequestOutcome::SessionNotFound,
        Err(E::Io { detail }) => return DraftPullRequestOutcome::SessionUnavailable { detail },
        Err(reason) => {
            return DraftPullRequestOutcome::SessionDamaged {
                reason: reason.to_string(),
            }
        }
    };
    let Some(record) = records.iter().find(|r| r.execution_id == execution_id) else {
        return DraftPullRequestOutcome::ResultNotFound;
    };
    let Some(commit) = &record.commit else {
        return DraftPullRequestOutcome::NoCommit;
    };
    let Some(worktree_path) = &record.worktree_path else {
        return DraftPullRequestOutcome::NoCommit;
    };

    let compare_url = git2::Repository::open(worktree_path).ok().and_then(|repo| {
        let branch = record.branch.clone().or_else(|| {
            repo.head()
                .ok()
                .and_then(|h| h.shorthand().ok().map(str::to_owned))
        })?;
        let remote = repo.find_remote("origin").ok()?;
        let url = remote.url().unwrap_or("").to_string();
        if url.is_empty() {
            return None;
        }
        let parsed = crate::git::remote_url::parse(&url)?;
        let base = parsed.web_base();
        if base.is_empty() {
            return None;
        }
        // GitHub/GitLab/Bitbucket/Azure all accept a compare-style path off
        // the repo's web base; this mirrors what "Compare & pull request"
        // links to, without calling any host API to construct it.
        Some(format!("{base}/compare/{branch}?expand=1"))
    });

    let title = commit.subject.clone();
    let body = record
        .openspec_change_id
        .as_deref()
        .map(|id| format!("Spec: {id}"))
        .unwrap_or_default();

    DraftPullRequestOutcome::Drafted {
        draft: DraftPullRequest {
            title,
            body,
            compare_url,
        },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_draft_pull_request(
    app: AppHandle,
    session_id: SessionId,
    execution_id: ExecutionId,
) -> Result<DraftPullRequestOutcome, AppError> {
    let root = resolve_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || draft_pull_request_at(&root, &session_id, &execution_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Opens the host's compare/new-PR page in the user's default browser. The
/// **only** action in this module that reaches outside the app, and it opens
/// a URL a browser GET request loads -- nothing here pushes a branch, calls
/// a host write API, or posts anything. Reuses `tauri_plugin_opener`, the
/// same mechanism `commands::external::reveal_in_file_manager` uses for
/// "open in file manager"/"open in editor".
#[tauri::command]
#[specta::specta]
pub async fn agent_result_open_pull_request_page(app: AppHandle, url: String) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    tauri::async_runtime::spawn_blocking(move || {
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| AppError::Other(e.to_string()))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

// -- 5: cleanup and recovery --

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CleanupWorktreeOutcome {
    Removed { record: ResultRecord },
    /// Hand edits or other uncommitted work remain -- the worktree is kept
    /// and the caller should offer Open rather than deleting (task 5.2).
    KeptHandEdited { record: ResultRecord, modified: u32, untracked: u32 },
    /// Not safe to clean up yet: the result is not `Committed` or
    /// `Discarded` (task 5.1: "only after safe integration or confirmed
    /// discard").
    NotIntegratedOrDiscarded,
    NothingToClean,
    RefusedLocked { path: String },
    PartiallyRemoved { path: String },
    ResultNotFound,
    SessionNotFound,
    SessionDamaged { reason: String },
    SessionUnavailable { detail: String },
    WriteFailed { detail: String },
}

/// Remove a result's worktree once its work is safely landed (`Committed`)
/// or explicitly thrown away (`Discarded`). Routes through
/// `git::worktree::remove`, the exact function the ordinary worktree panel
/// uses, with `DirtyChoice::Refuse` -- cleanup NEVER escalates to `Discard`
/// on its own; a dirty worktree here always means "something unaccounted for
/// is in it" (task 5.1's "safe integration" already covers the expected
/// changes) and is reported back as `KeptHandEdited` rather than removed.
fn cleanup_worktree_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    repo: &git2::Repository,
    main_repo_path: &str,
    session_id: &str,
    execution_id: &str,
) -> CleanupWorktreeOutcome {
    use SessionLoadError as E;
    let peek = locks.with_session_lock(session_id, || result::read_results(root, session_id));
    let records = match peek {
        Ok(r) => r,
        Err(E::NotFound) => return CleanupWorktreeOutcome::SessionNotFound,
        Err(E::Io { detail }) => return CleanupWorktreeOutcome::SessionUnavailable { detail },
        Err(reason) => {
            return CleanupWorktreeOutcome::SessionDamaged {
                reason: reason.to_string(),
            }
        }
    };
    let Some(record) = records.iter().find(|r| r.execution_id == execution_id).cloned() else {
        return CleanupWorktreeOutcome::ResultNotFound;
    };
    if !matches!(record.state, ResultState::Committed | ResultState::Discarded) {
        return CleanupWorktreeOutcome::NotIntegratedOrDiscarded;
    }
    let Some(worktree_path) = &record.worktree_path else {
        return CleanupWorktreeOutcome::NothingToClean;
    };

    // Name git's worktree entry for this path (its admin folder name may
    // differ from the trailing path component, though in practice
    // `git::worktree::add` names them the same).
    let name = worktree::list(repo, None)
        .into_iter()
        .find(|w| worktree::paths_equal(Path::new(&w.path), Path::new(worktree_path)))
        .map(|w| w.name);
    let Some(name) = name else {
        // Already gone (or never existed as a worktree, e.g. a bare repo
        // path). Nothing left to clean up -- report success rather than an
        // error the user cannot act on.
        return CleanupWorktreeOutcome::Removed { record };
    };

    let outcome = worktree::remove(repo, main_repo_path, &name, DirtyChoice::Refuse);
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            return CleanupWorktreeOutcome::WriteFailed {
                detail: e.to_string(),
            }
        }
    };

    match outcome {
        RemoveOutcome::Removed { .. } => CleanupWorktreeOutcome::Removed { record },
        RemoveOutcome::RefusedDirty { modified, untracked } => CleanupWorktreeOutcome::KeptHandEdited {
            record,
            modified,
            untracked,
        },
        RemoveOutcome::RefusedLocked { path } => CleanupWorktreeOutcome::RefusedLocked { path },
        RemoveOutcome::PartiallyRemoved { path } => CleanupWorktreeOutcome::PartiallyRemoved { path },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_cleanup_worktree(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    manager: tauri::State<'_, crate::state::RepoManager>,
    repo_id: String,
    session_id: SessionId,
    execution_id: ExecutionId,
) -> Result<CleanupWorktreeOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks_arc = locks.inner().clone();
    let open = manager.get(&repo_id)?;
    let main_repo_path = open.path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        let outcome = cleanup_worktree_at(&locks_arc, &root, &repo, &main_repo_path, &session_id, &execution_id);
        // Windows file-lock retry (task 5.5), matching
        // `commands::worktree::remove_worktree`'s own one-retry pattern: a
        // lock right after this call is often a handle already on its way
        // out, not a real holder.
        if let CleanupWorktreeOutcome::RefusedLocked { .. } = outcome {
            std::thread::sleep(std::time::Duration::from_millis(250));
            return cleanup_worktree_at(&locks_arc, &root, &repo, &main_repo_path, &session_id, &execution_id);
        }
        outcome
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Reconciles result records at startup: worktrees whose folder is gone are
/// flagged rather than silently dropped (task 5.3: "without deleting
/// automatically"). Returns the execution IDs whose worktree could not be
/// found, so the caller can surface a recovery affordance instead of a
/// blank/broken result.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrphanedResult {
    pub execution_id: ExecutionId,
    pub worktree_path: String,
}

fn find_orphaned_worktrees_at(records: &[ResultRecord]) -> Vec<OrphanedResult> {
    records
        .iter()
        .filter(|r| matches!(r.state, ResultState::Kept | ResultState::CleanupNeeded))
        .filter_map(|r| {
            let path = r.worktree_path.as_ref()?;
            if Path::new(path).exists() {
                None
            } else {
                Some(OrphanedResult {
                    execution_id: r.execution_id.clone(),
                    worktree_path: path.clone(),
                })
            }
        })
        .collect()
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_find_orphaned(
    app: AppHandle,
    session_id: SessionId,
) -> Result<Vec<OrphanedResult>, AppError> {
    let root = resolve_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        // One session, asked for by name: a caller that named it is owed the
        // reason it got nothing back, rather than an empty list that reads as
        // "no orphans here". The whole-store sweep below has to keep going
        // past a damaged session; this one has nothing else to go on.
        let records = result::read_results(&root, &session_id).map_err(|e| {
            AppError::Other(format!(
                "GitWyrm could not read this chat's record of what its agents left behind: {e}"
            ))
        })?;
        Ok(find_orphaned_worktrees_at(&records))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// One agent copy still on disk, named to the chat it belongs to.
///
/// Agent runs work in a full checkout so they cannot disturb what the person
/// has open. Those copies were removable from the result panel but nowhere
/// said they existed, so discovery happened in the file manager or on a full
/// disk -- which for a product whose promise is a Git client with nothing to
/// hide is a contradiction rather than a missing feature.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentCopyOnDisk {
    /// The chat this copy belongs to, when one still does.
    ///
    /// `None` for an orphan: a copy whose chat was deleted. Deleting a chat
    /// removes the sidecar that named this path, so before these were listed
    /// from git the folder became permanently invisible here -- on the one
    /// screen whose job is saying what GitWyrm is holding.
    pub session_id: Option<SessionId>,
    pub repo_id: String,
    /// The chat's title, or `None` for an orphan. The UI names it rather than
    /// inventing a placeholder title.
    pub session_title: Option<String>,
    pub execution_id: Option<String>,
    pub worktree_path: String,
    /// Total size of the copy in bytes. `None` when the folder could not be
    /// measured -- reported as unknown rather than as zero, since a zero
    /// would read as "this costs nothing" when the truth is "we could not
    /// look".
    ///
    /// `f64`, not `u64`: Specta refuses to export 64-bit integers because it
    /// cannot know whether the serializer handles BigInt, and `cargo check`
    /// stays green while `export_bindings` dies. A double holds every integer
    /// up to 2^53 exactly, which is 9 petabytes -- past any worktree.
    pub size_bytes: Option<f64>,
    /// The result's state, so the UI can say why a copy is still held.
    /// `None` for an orphan, whose result record is gone with its chat.
    pub state: Option<ResultState>,
}

/// Adds up a folder's files, following no symlinks and giving up rather than
/// guessing.
///
/// Returns `None` on any read failure, because a partial total presented as a
/// total is worse than no number: someone deciding whether to clear 6 GB must
/// not be shown 200 MB because a subfolder was unreadable.
fn dir_size_bytes(path: &Path) -> Option<u64> {
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).ok()? {
            let entry = entry.ok()?;
            // `symlink_metadata` so a link is counted as the link, never
            // followed out of the folder being measured.
            let meta = entry.metadata().ok()?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Some(total)
}

/// Every agent copy still on disk, largest first.
///
/// The counterpart to [`agent_result_find_orphaned_all`]: that one reports
/// results whose folder is GONE, this one reports the folders that are still
/// there. Same fan-out shape -- the cheap index for session identity, then
/// one sidecar read per session.
/// A comparable key for a worktree path.
///
/// The same folder reaches this function as a stored string from a chat record
/// and as git's own path, which differ in separator and case on Windows.
/// Mirrors `git::worktree::paths_equal`'s own fallback normalisation -- that
/// function compares two paths, this one has to bucket many.
fn normalize_worktree_key(path: &str) -> String {
    // `char::from_u32(92)` is a backslash. Written this way because a literal
    // one does not survive the tooling that edits this file -- the same trap
    // qa-log #84 records, which produced a silently Windows-only bug once.
    let backslash = char::from_u32(92).unwrap_or('/');
    match std::path::Path::new(path).canonicalize() {
        Ok(p) => p.to_string_lossy().to_lowercase().replace(backslash, "/"),
        Err(_) => path.to_lowercase().replace(backslash, "/"),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn agent_result_copies_on_disk(
    app: AppHandle,
    manager: tauri::State<'_, crate::state::RepoManager>,
) -> Result<Vec<AgentCopyOnDisk>, AppError> {
    let root = resolve_root(&app)?;
    // Every open repository, so the git side can be asked directly.
    let repos = manager.open_repos();

    tauri::async_runtime::spawn_blocking(move || {
        // What the chat records know: path -> (session, execution, title, state).
        //
        // This used to be the ONLY source, which is why deleting a chat made
        // its copy permanently invisible here: the sidecar naming the path
        // went with the chat. Now it is the join, not the enumeration.
        let loaded = crate::agentdesk::store::load_or_rebuild_index(&root);
        let mut known: std::collections::HashMap<String, AgentCopyOnDisk> = std::collections::HashMap::new();
        for header in loaded.headers {
            for r in result::read_results(&root, &header.session_id).unwrap_or_default() {
                let Some(path) = r.worktree_path.clone() else {
                    continue;
                };
                known.insert(
                    normalize_worktree_key(&path),
                    AgentCopyOnDisk {
                        session_id: Some(header.session_id.clone()),
                        repo_id: header.repo_id.clone(),
                        session_title: Some(header.title.clone()),
                        execution_id: Some(r.execution_id.clone()),
                        size_bytes: None,
                        worktree_path: path,
                        state: Some(r.state),
                    },
                );
            }
        }

        // Enumerate from git. Provisioning marks every agent worktree
        // (`git::worktree::mark_as_run_worktree`) and `worktree::list` already
        // reports that mark, so git knows which checkouts are ours whether or
        // not a chat still points at them.
        let mut out: Vec<AgentCopyOnDisk> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (repo_id, open) in &repos {
            let Ok(repo) = open.repo.lock() else { continue };
            for wt in crate::git::worktree::list(&repo, None)
                .into_iter()
                .filter(|w| w.is_run_worktree)
            {
                if !Path::new(&wt.path).exists() {
                    continue;
                }
                let key = normalize_worktree_key(&wt.path);
                if !seen.insert(key.clone()) {
                    continue;
                }
                let mut row = known.remove(&key).unwrap_or(AgentCopyOnDisk {
                    // An orphan: git has the folder, no chat claims it.
                    session_id: None,
                    repo_id: repo_id.clone(),
                    session_title: None,
                    execution_id: None,
                    size_bytes: None,
                    worktree_path: wt.path.clone(),
                    state: None,
                });
                row.size_bytes = dir_size_bytes(Path::new(&wt.path)).map(|b| b as f64);
                out.push(row);
            }
        }

        // Copies a chat still names that git did not list -- a repository not
        // open right now, most often. Kept rather than dropped: this screen
        // reported them before and losing them would be a regression.
        for (_key, mut row) in known {
            if !Path::new(&row.worktree_path).exists() {
                continue;
            }
            row.size_bytes = dir_size_bytes(Path::new(&row.worktree_path)).map(|b| b as f64);
            out.push(row);
        }

        // Largest first: the copy worth clearing is the one taking the room.
        // An unmeasurable copy sorts last rather than first, so a folder we
        // could not read never displaces a real 6 GB one at the top.
        out.sort_by(|a, b| {
            b.size_bytes
                .unwrap_or(0.0)
                .partial_cmp(&a.size_bytes.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// One orphaned result, named to the session it belongs to -- what
/// [`agent_result_find_orphaned`] cannot say on its own, since it already
/// takes a single `session_id` and answers only for that one session.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrphanedResultInSession {
    pub session_id: SessionId,
    pub repo_id: String,
    pub session_title: String,
    pub orphan: OrphanedResult,
}

/// One session's results for the whole-store sweep: its records, or an empty
/// list when they cannot be read.
///
/// Skipping a damaged session is right for a fan-out -- one bad sidecar must
/// never hide every other session's real orphans. Skipping it *silently* is
/// not. That session drops out of every future sweep, and the only reading
/// the caller has of an empty list is "no orphans", which is a claim about
/// the world rather than about what could be read.
///
/// So the skip stays and a warning goes to the log, for the same reason
/// `inspect_worktree_on_disk` keeps an unopenable folder visible: the person
/// gets a note rather than a record that quietly ceased to exist. A missing
/// sidecar is NOT that case -- `read_results` returns an empty list for a
/// session that has simply produced no results yet, which is ordinary and
/// says nothing.
fn orphan_records_for_sweep(root: &SessionStoreRoot, session_id: &str) -> Vec<ResultRecord> {
    match result::read_results(root, session_id) {
        Ok(records) => records,
        Err(e) => {
            log::warn!(
                "agent desk: session {session_id} skipped in the orphan scan, its record of results could not be read: {e}"
            );
            Vec::new()
        }
    }
}

/// P1-C wiring 3 ("orphan-result detection is registered but not called at
/// startup"): [`agent_result_find_orphaned`] existed, was registered, and
/// worked correctly for one session, but nothing ever called it -- a run
/// that crashed with a `Kept`/`CleanupNeeded` result pointing at a worktree
/// folder that is now gone would sit that way silently forever, since
/// nothing about opening the app, or opening that one session, re-checks
/// every result's worktree path against disk.
///
/// This is the whole-store counterpart the frontend can call once, at Agent
/// Desk startup, without first knowing which session(s) might have an
/// orphan -- it scans every session's index entry (not a full session read
/// each: `result::read_results` is its own sidecar file per session,
/// [`store::load_or_rebuild_index`] is what gives the session id/repo id/
/// title cheaply) and reuses [`find_orphaned_worktrees_at`] per session, the
/// exact same detection `agent_result_find_orphaned` already used -- this
/// command is a fan-out over sessions, not a second implementation of what
/// "orphaned" means.
///
/// A session whose result sidecar cannot be read (`unwrap_or_default`, same
/// stance as the single-session command above) is skipped rather than
/// failing the whole scan -- one damaged session's results must never hide
/// every other session's real orphans from the startup reconciliation this
/// exists to drive.
#[tauri::command]
#[specta::specta]
pub async fn agent_result_find_orphaned_all(app: AppHandle) -> Result<Vec<OrphanedResultInSession>, AppError> {
    let root = resolve_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let loaded = crate::agentdesk::store::load_or_rebuild_index(&root);
        loaded
            .headers
            .into_iter()
            .filter(|h| !h.archived)
            .flat_map(|header| {
                let records = orphan_records_for_sweep(&root, &header.session_id);
                find_orphaned_worktrees_at(&records)
                    .into_iter()
                    .map(move |orphan| OrphanedResultInSession {
                        session_id: header.session_id.clone(),
                        repo_id: header.repo_id.clone(),
                        session_title: header.title.clone(),
                        orphan,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

// -- agent-graphs 6.4: recover executions that died with the app --

/// Answers `session_recovery::recover_orphaned_executions`'s
/// `inspect_worktree` question from disk: `None` when the folder is gone,
/// otherwise how many files its status walk reports as changed. A folder
/// that exists but cannot be opened as a repository counts as present with
/// no changes, so the person still gets a note about it rather than a
/// silently skipped record.
fn inspect_worktree_on_disk(path: &str) -> Option<usize> {
    if !Path::new(path).is_dir() {
        return None;
    }
    Some(changed_paths_for_worktree(Path::new(path)).map(|p| p.len()).unwrap_or(0))
}

/// Everything the result build needs from a recovered execution, captured
/// while the session was still held under its lock so the build itself can
/// run without it (matching `airun::build_result_for_completed_execution`,
/// which reads the same provenance off the `ExecutionRecord`).
struct RecoveredBuild {
    orphan: crate::agentdesk::session_recovery::RecoveredOrphan,
    branch: Option<String>,
    base_oid: Option<String>,
    checks: Vec<ResultCheckOutcome>,
    openspec_change_id: Option<String>,
}

/// One session's startup recovery: marks every execution nothing in this
/// process backs as `Failed`, builds a `Failed` result for each one whose
/// worktree is still on disk (so the review panel offers Keep/discard for
/// the work it left behind), and appends a plain note per execution to the
/// transcript. Returns what was recovered; empty when nothing was stuck.
///
/// The session write happens in one critical section with the decision, the
/// same shape as `agent_desk::get_session_at`. Result builds and notes run
/// afterwards, each taking the lock on its own, because `build_result_at`
/// and `append_system_note` already acquire it and the lock is not
/// re-entrant.
///
/// `is_live` is passed in rather than read from `ExecutionRegistry` here so
/// tests can simulate a live execution without an app handle.
pub(crate) fn recover_orphaned_executions_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
    is_live: impl FnMut(&str) -> bool,
) -> Vec<crate::agentdesk::session_recovery::RecoveredOrphan> {
    let now = now_rfc3339();
    let builds: Vec<RecoveredBuild> = locks.with_session_lock(session_id, || {
        let mut session = match crate::agentdesk::store::read_session(root, session_id) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("agent desk startup recovery: could not read session {session_id}: {e}");
                return Vec::new();
            }
        };
        let recovered = crate::agentdesk::session_recovery::recover_orphaned_executions(
            &mut session,
            &now,
            is_live,
            inspect_worktree_on_disk,
        );
        if recovered.is_empty() {
            return Vec::new();
        }
        if let Err(e) = crate::agentdesk::store::write_session(root, &session) {
            // Nothing else runs for this session: a note or result without the
            // state change behind it would describe a failure the session file
            // still denies. The next launch simply finds the same records.
            log::warn!("agent desk startup recovery: could not write session {session_id}: {e}");
            return Vec::new();
        }
        let openspec_change_id = crate::commands::agent_desk::openspec_change_id_of(&session.header.source);
        recovered
            .into_iter()
            .map(|orphan| {
                let record = session.executions.iter().find(|e| e.execution_id == orphan.execution_id);
                RecoveredBuild {
                    branch: record.and_then(|r| r.branch.clone()),
                    base_oid: record.and_then(|r| r.base_oid.clone()),
                    checks: checks_for_execution(&session, &orphan.execution_id),
                    openspec_change_id: openspec_change_id.clone(),
                    orphan,
                }
            })
            .collect()
    });

    let mut recovered = Vec::with_capacity(builds.len());
    for build in builds {
        if let Some(worktree_path) = build.orphan.result_worktree_path() {
            let outcome = build_result_at(
                locks,
                root,
                session_id,
                build.orphan.execution_id.clone(),
                ResultOutcomeKind::Failed,
                Some(worktree_path.to_string()),
                build.branch,
                build.base_oid,
                build.checks,
                build.openspec_change_id,
            );
            if !matches!(outcome, BuildResultOutcome::Built { .. }) {
                log::warn!(
                    "agent desk startup recovery: result for {} in session {session_id} not built: {outcome:?}",
                    build.orphan.execution_id
                );
            }
        }
        crate::commands::agent_graph::append_system_note(locks, root, session_id, &build.orphan.note);
        recovered.push(build.orphan);
    }
    recovered
}

/// Runs once from `lib.rs` setup, before the webview exists. It has to be
/// before, not alongside: the first `agent_session_get` a mounted sidebar
/// issues would otherwise reach `get_session_at`'s lazy reconciliation
/// first, flip the same records to `Interrupted`, and this sweep would then
/// find nothing to recover while the helper's worktree stayed on disk.
///
/// A session is read in full when its header is still in a live-process
/// state, **or** when it ever started an agent graph.
///
/// The header alone used to be the whole filter, justified by "a helper only
/// runs while its lead does". That is not true. `stop_execution_at` with
/// `StopScope::One` stops exactly one execution and then sets the header to
/// `Stopped` regardless, so stopping a lead while three helpers are working
/// leaves a terminal header above running helpers -- and
/// `advance_graph_after_helper_completion` finishes the header the same way.
/// Kill the app there and the sweep skipped the session entirely: those
/// helpers' worktrees, holding real uncommitted work, were never inspected,
/// never got a result, and never got a note. The lazy path does not save
/// them either -- by design it never looks at a worktree -- so the work sat
/// on disk with no route to Keep or discard. That is precisely the case this
/// sweep exists for.
///
/// `graph_started_at` is the cheap index-side signal for "this session can
/// have helpers": it is set once, when a graph is actually accepted and
/// started (`agent_graph.rs`), and never cleared. Sessions that never ran a
/// graph and whose header is terminal are still skipped, so the common case
/// costs one index load and nothing more.
/// Whether the startup sweep needs to open one session in full.
///
/// Split out from the loop so the selection rule is testable on its own: the
/// loop needs an `AppHandle`, a store root and a lock registry, so every
/// existing test called `recover_orphaned_executions_at` directly and this
/// filter -- the part that decides whether recovery happens at all -- was
/// never exercised by anything. That is where it went wrong.
pub(crate) fn startup_sweep_should_read(
    header_state: crate::agentdesk::model::SessionState,
    ever_started_a_graph: bool,
) -> bool {
    crate::agentdesk::session_recovery::is_live_process_state(header_state) || ever_started_a_graph
}

pub(crate) fn recover_orphaned_executions_on_startup(app: &AppHandle) {
    use tauri::Manager;

    let root = match SessionStoreRoot::resolve(app) {
        Ok(root) => root,
        Err(e) => {
            log::warn!("agent desk startup recovery skipped, store unavailable: {e}");
            return;
        }
    };
    let locks = app.state::<std::sync::Arc<SessionLocks>>();
    let executions = app.state::<crate::agentdesk::ExecutionRegistry>();

    let loaded = crate::agentdesk::store::load_or_rebuild_index(&root);
    let mut recovered_total = 0usize;
    let mut sessions_touched = 0usize;
    for header in loaded
        .headers
        .iter()
        .filter(|h| startup_sweep_should_read(h.state, h.graph_started_at.is_some()))
    {
        let session_id = header.session_id.clone();
        let recovered = recover_orphaned_executions_at(&locks, &root, &session_id, |execution_id| {
            executions.is_live(&session_id, &execution_id.to_string())
        });
        if !recovered.is_empty() {
            sessions_touched += 1;
            recovered_total += recovered.len();
        }
    }

    if recovered_total > 0 {
        // Header states moved, and the index is a projection of headers that
        // `write_session` does not maintain (see `agent_desk::refresh_index`).
        let (headers, _diagnostics) = crate::agentdesk::store::rebuild_index_from_sessions(&root);
        if let Err(e) = crate::agentdesk::store::write_index(&root, &headers) {
            log::warn!("agent desk startup recovery: index not refreshed: {e}");
        }
        log::info!(
            "agent desk startup recovery: {recovered_total} run(s) across {sessions_touched} session(s) were still marked as running from a previous launch and were closed out"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::{
        AgentSession, AgentSessionHeader, ExecutionRecord, SessionIntent, SessionSource,
        SessionState, CURRENT_SCHEMA_VERSION,
    };
    use crate::agentdesk::result::{CheckEvidenceSource, CheckRunOutcome, ResultCheckOutcome};

    /// Nothing in this file pushes or posts on the user's behalf.
    ///
    /// The module doc has promised this since results shipped, and it even
    /// names the check -- "Grep for `git_push` or `HostProvider` in this
    /// file" -- without ever running it. That is the product's strongest
    /// safety claim ("the agent never silently pushes, posts a review, merges
    /// a pull request, or changes an external service") resting on prose.
    ///
    /// Reads this file's own source, because what is being asserted is the
    /// ABSENCE of a call, and absence is not reachable from a value.
    ///
    /// Deliberately ignores comment lines: the doc comment names both symbols
    /// on purpose, and a test that could not tell an explanation from a call
    /// would either fail today or force the explanation out of the file that
    /// needs it most.
    #[test]
    fn no_command_here_pushes_or_posts_on_the_users_behalf() {
        // Assembled from halves so this test's own needles cannot match the
        // lines that define them. Spelling them literally made the check fail
        // on itself, which is the shape where a guard quietly starts testing
        // its own source instead of the code.
        let needles = [
            format!("git{}push", "_"),
            format!("Host{}", "Provider"),
            format!("push{}branch", "_"),
        ];

        // Every file the feature is made of, not just this one.
        //
        // This read only `agent_result.rs`, which is where the promise is
        // written down -- and a push added one file over would have been
        // invisible to it. The promise is about Agent Desk, so the check is
        // too: the whole `agentdesk/` module and every `agent_*` command
        // beside it.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut sources: Vec<(std::path::PathBuf, String)> = Vec::new();
        let mut stack = vec![root.join("agentdesk")];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        sources.push((path, text));
                    }
                }
            }
        }
        let Ok(commands) = std::fs::read_dir(root.join("commands")) else {
            panic!("the commands directory must be readable for this check to mean anything");
        };
        for entry in commands.flatten() {
            let path = entry.path();
            let is_agent_command = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("agent_") && n.ends_with(".rs"));
            if is_agent_command {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    sources.push((path, text));
                }
            }
        }
        assert!(
            sources.len() > 5,
            "found only {} files to check, which means the walk is not finding the feature",
            sources.len()
        );

        let mut offenders = Vec::new();
        for (path, text) in &sources {
            for (i, raw) in text.lines().enumerate() {
                let line = raw.trim();
                // The doc comments naming these symbols are the explanation,
                // not a call.
                if line.starts_with("//") || line.starts_with("*") {
                    continue;
                }
                // And this test's own needle definitions, which are the
                // three lines that split a symbol across a `format!` join.
                //
                // Narrow on purpose. The first version skipped every line
                // containing `format!("`, which is precisely how a real push
                // would be written -- an injected `format!("git_push {n}")`
                // sailed through. A skip wide enough to hide the thing being
                // looked for is not an exemption, it is a hole.
                let is_needle_definition = line.starts_with("format!(\"")
                    && line.contains("\", \"");
                if is_needle_definition {
                    continue;
                }
                if needles.iter().any(|n| line.contains(n.as_str())) {
                    offenders.push(format!("{}:{}: {line}", path.display(), i + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "Agent Desk must never push or post on the user's behalf; drafting opens the host's own page and the user submits it. Found: {offenders:#?}"
        );
    }

    /// A check that reported no name is not recorded as evidence.
    ///
    /// The name is what a completion condition is matched against. A nameless
    /// check cannot answer "did it make `cargo test` pass?" either way, so
    /// keeping it would record evidence that names nothing -- and under the
    /// old matching rule an empty name satisfied every condition there is.
    #[test]
    fn a_check_that_reported_no_name_is_not_recorded() {
        let mut session = AgentSession::new(header("sess-1"));
        for name in ["", "   ", "cargo test"] {
            session.messages.push(crate::agentdesk::model::SessionMessage {
                message_id: format!("m-{name}"),
                segment_id: "seg-1".into(),
                role: crate::agentdesk::model::MessageRole::Assistant,
                timestamp: "2026-01-01T00:00:00Z".into(),
                plain_content: String::new(),
                rendered_content: Some(
                    serde_json::to_string(&crate::airun::driver::RunStep::Check {
                        name: name.to_string(),
                        passed: true,
                        detail: String::new(),
                    })
                    .unwrap(),
                ),
                provider: None,
                model: None,
                kind: crate::agentdesk::model::MessageKind::Tool,
                execution_id: Some("exec-1".into()),
                sequence: Some(1),
                import: None,
                targets: Vec::new(),
            });
        }

        let checks = checks_for_execution(&session, "exec-1");
        assert_eq!(
            checks.iter().map(|c| c.command_name.as_str()).collect::<Vec<_>>(),
            vec!["cargo test"],
            "only the named check should survive"
        );
    }

    // -- startup orphan recovery, end to end against the real store and git --

    fn seed_session_with_helper(
        root: &SessionStoreRoot,
        session_id: &str,
        helper_state: SessionState,
        helper_worktree: Option<&str>,
    ) {
        let mut session = AgentSession::new(header(session_id));
        session.header.active_execution_id = Some("lead".into());
        session.executions.push(ExecutionRecord::minimal(
            "lead".into(),
            session_id.into(),
            None,
            SessionState::Finished,
            "2026-01-01T00:00:00Z".into(),
            Some("2026-01-01T00:10:00Z".into()),
            0,
        ));
        let mut helper = ExecutionRecord::minimal(
            "helper-1".into(),
            session_id.into(),
            Some("lead".into()),
            helper_state,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        );
        helper.worktree_path = helper_worktree.map(str::to_string);
        helper.branch = Some("agent/helper-1".into());
        session.executions.push(helper);
        crate::agentdesk::store::write_session(root, &session).unwrap();
    }

    fn system_notes(root: &SessionStoreRoot, session_id: &str) -> Vec<String> {
        crate::agentdesk::store::read_session(root, session_id)
            .unwrap()
            .messages
            .iter()
            .filter(|m| m.role == crate::agentdesk::model::MessageRole::System)
            .map(|m| m.plain_content.clone())
            .collect()
    }

    /// The selection rule nothing was testing. Every other startup test
    /// calls `recover_orphaned_executions_at` directly, so the filter that
    /// decides whether a session is opened at all was never exercised -- and
    /// it was wrong: it trusted "a helper only runs while its lead does",
    /// which `StopScope::One` breaks by stopping one execution and marking
    /// the header `Stopped` while siblings keep working.
    #[test]
    fn the_startup_sweep_still_opens_a_finished_session_that_ran_a_graph() {
        use crate::agentdesk::model::SessionState;

        // The case that was skipped: helpers may be stuck underneath.
        for terminal in [SessionState::Stopped, SessionState::Finished, SessionState::Failed] {
            assert!(
                startup_sweep_should_read(terminal, true),
                "{terminal:?} with a graph must still be opened -- its helpers can outlive it"
            );
        }

        // Unchanged: a session that never ran a graph and is finished has
        // nothing this sweep would touch, so it stays cheap.
        for terminal in [SessionState::Stopped, SessionState::Finished, SessionState::Failed] {
            assert!(!startup_sweep_should_read(terminal, false));
        }

        // And a live header is read whether or not a graph ever ran.
        for live in [SessionState::Working, SessionState::Preparing, SessionState::NeedsInput] {
            assert!(startup_sweep_should_read(live, false));
            assert!(startup_sweep_should_read(live, true));
        }
    }

    #[test]
    fn startup_recovery_fails_a_working_helper_and_builds_a_result_for_its_dirty_worktree() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        let worktree = worktree_with_a_change();
        let worktree_path = worktree.path().to_string_lossy().into_owned();
        seed_session_with_helper(&root, "sess-1", SessionState::Working, Some(&worktree_path));

        let recovered = recover_orphaned_executions_at(&locks, &root, "sess-1", |_| false);

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].execution_id, "helper-1");

        let session = crate::agentdesk::store::read_session(&root, "sess-1").unwrap();
        let helper = session.executions.iter().find(|e| e.execution_id == "helper-1").unwrap();
        assert_eq!(helper.state, SessionState::Failed);
        assert!(helper.ended_at.is_some());
        assert_eq!(session.executions[0].state, SessionState::Finished, "finished lead untouched");

        let records = result::read_results(&root, "sess-1").unwrap();
        let record = result::find_result(&records, "helper-1").expect("a result must exist for the dirty worktree");
        assert_eq!(record.outcome, ResultOutcomeKind::Failed);
        assert_eq!(record.worktree_path.as_deref(), Some(worktree_path.as_str()));
        assert_eq!(record.branch.as_deref(), Some("agent/helper-1"));
        assert!(record.has_landable_changes(), "the uncommitted b.txt must show up as a change to keep");
        assert!(record.changed_paths.iter().any(|p| p.path == "b.txt"));

        let notes = system_notes(&root, "sess-1");
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("GitWyrm was closed"), "note was {:?}", notes[0]);
        assert!(notes[0].contains("1 changed file"), "note was {:?}", notes[0]);
    }

    #[test]
    fn startup_recovery_fails_a_working_helper_without_a_worktree_and_builds_no_result() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        seed_session_with_helper(&root, "sess-1", SessionState::Working, None);

        let recovered = recover_orphaned_executions_at(&locks, &root, "sess-1", |_| false);

        assert_eq!(recovered.len(), 1);
        let session = crate::agentdesk::store::read_session(&root, "sess-1").unwrap();
        assert_eq!(session.executions[1].state, SessionState::Failed);

        let records = result::read_results(&root, "sess-1").unwrap_or_default();
        assert!(records.is_empty(), "no worktree means nothing to review, so no result");

        let notes = system_notes(&root, "sess-1");
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("nothing extra to keep"), "note was {:?}", notes[0]);
    }

    #[test]
    fn startup_recovery_notes_a_worktree_that_is_gone_from_disk() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        let gone = tempfile::tempdir().unwrap();
        let gone_path = gone.path().to_string_lossy().into_owned();
        drop(gone);
        seed_session_with_helper(&root, "sess-1", SessionState::Working, Some(&gone_path));

        let recovered = recover_orphaned_executions_at(&locks, &root, "sess-1", |_| false);

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].result_worktree_path(), None);
        assert!(result::read_results(&root, "sess-1").unwrap_or_default().is_empty());
        let notes = system_notes(&root, "sess-1");
        assert!(notes[0].contains("no longer on disk"), "note was {:?}", notes[0]);
    }

    #[test]
    fn startup_recovery_leaves_a_finished_session_alone() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        seed_session_with_helper(&root, "sess-1", SessionState::Finished, Some("C:/wt/whatever"));
        let before = crate::agentdesk::store::read_session(&root, "sess-1").unwrap();

        let recovered = recover_orphaned_executions_at(&locks, &root, "sess-1", |_| false);

        assert!(recovered.is_empty());
        let after = crate::agentdesk::store::read_session(&root, "sess-1").unwrap();
        assert_eq!(after, before, "no write may happen when nothing is stuck");
        assert!(result::read_results(&root, "sess-1").unwrap_or_default().is_empty());
        assert!(system_notes(&root, "sess-1").is_empty());
    }

    #[test]
    fn startup_recovery_leaves_a_live_execution_alone() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        let worktree = worktree_with_a_change();
        let worktree_path = worktree.path().to_string_lossy().into_owned();
        seed_session_with_helper(&root, "sess-1", SessionState::Working, Some(&worktree_path));
        let before = crate::agentdesk::store::read_session(&root, "sess-1").unwrap();

        let live = ["helper-1".to_string()];
        let recovered = recover_orphaned_executions_at(&locks, &root, "sess-1", |id| live.iter().any(|l| l == id));

        assert!(recovered.is_empty());
        let after = crate::agentdesk::store::read_session(&root, "sess-1").unwrap();
        assert_eq!(after, before);
        assert_eq!(after.executions[1].state, SessionState::Working);
        assert!(result::read_results(&root, "sess-1").unwrap_or_default().is_empty());
        assert!(system_notes(&root, "sess-1").is_empty());
    }

    #[test]
    fn startup_recovery_is_idempotent_across_launches() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        let worktree = worktree_with_a_change();
        let worktree_path = worktree.path().to_string_lossy().into_owned();
        seed_session_with_helper(&root, "sess-1", SessionState::Working, Some(&worktree_path));

        let first = recover_orphaned_executions_at(&locks, &root, "sess-1", |_| false);
        let second = recover_orphaned_executions_at(&locks, &root, "sess-1", |_| false);

        assert_eq!(first.len(), 1);
        assert!(second.is_empty(), "a second launch must find nothing left to recover");
        assert_eq!(system_notes(&root, "sess-1").len(), 1, "no duplicate note");
        assert_eq!(result::read_results(&root, "sess-1").unwrap().len(), 1, "no duplicate result");
    }

    fn temp_root() -> (tempfile::TempDir, SessionStoreRoot) {
        let dir = tempfile::tempdir().unwrap();
        let root = SessionStoreRoot::at(dir.path().to_path_buf()).unwrap();
        (dir, root)
    }

    fn header(session_id: &str) -> AgentSessionHeader {
        AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: session_id.into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/proj".into(),
            repo_name: "proj".into(),
            title: "A session".into(),
            source: SessionSource::Manual { repo_id: "repo-1".into() },
            intent: SessionIntent::Fix,
            state: SessionState::Working,
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

    fn seed_session(root: &SessionStoreRoot, session_id: &str, execution_id: &str) {
        let mut session = AgentSession::new(header(session_id));
        session.executions.push(ExecutionRecord::minimal(
            execution_id.into(),
            session_id.into(),
            None,
            SessionState::Working,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        ));
        crate::agentdesk::store::write_session(root, &session).unwrap();
    }

    /// Build a real git repo with one commit and one uncommitted change, to
    /// exercise `changed_paths_for_worktree` against real git2 status.
    fn worktree_with_a_change() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        std::fs::write(dir.path().join("a.txt"), "one").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[]).unwrap();

        std::fs::write(dir.path().join("b.txt"), "two").unwrap();
        dir
    }

    #[test]
    fn build_result_refuses_an_unknown_execution() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        let outcome = build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-missing".into(),
            ResultOutcomeKind::Finished,
            None,
            None,
            None,
            Vec::new(),
            None,
        );
        assert!(matches!(outcome, BuildResultOutcome::ExecutionNotFound));
    }

    #[test]
    fn build_result_with_no_worktree_has_nothing_to_land() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        let outcome = build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            None,
            None,
            None,
            Vec::new(),
            None,
        );
        match outcome {
            BuildResultOutcome::Built { record } => assert!(!record.has_landable_changes()),
            other => panic!("expected Built, got {other:?}"),
        }
    }

    #[test]
    fn build_result_reads_real_changed_paths_from_the_worktree() {
        let (_dir, root) = temp_root();
        let wt = worktree_with_a_change();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        let outcome = build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(wt.path().to_string_lossy().into_owned()),
            Some("agent/exec-1".into()),
            None,
            vec![ResultCheckOutcome {
                command_name: "npm run typecheck".into(),
                outcome: CheckRunOutcome::Passed,
                summary: None,
                source: CheckEvidenceSource::AgentReported,
            }],
            Some("add-thing".into()),
        );
        match outcome {
            BuildResultOutcome::Built { record } => {
                assert!(record.has_landable_changes());
                assert_eq!(record.changed_paths.len(), 1);
                assert_eq!(record.changed_paths[0].path, "b.txt");
                assert_eq!(record.changed_paths[0].status, "A");
                assert_eq!(record.checks.len(), 1);
            }
            other => panic!("expected Built, got {other:?}"),
        }
    }

    #[test]
    fn keep_refuses_when_there_is_nothing_to_keep() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            None,
            None,
            None,
            Vec::new(),
            None,
        );
        let outcome = keep_result_at(&locks, &root, "sess-1", "exec-1");
        assert!(matches!(outcome, KeepResultOutcome::NothingToKeep));
    }

    #[test]
    fn keep_succeeds_when_the_worktree_has_changes() {
        let (_dir, root) = temp_root();
        let wt = worktree_with_a_change();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(wt.path().to_string_lossy().into_owned()),
            None,
            None,
            Vec::new(),
            None,
        );
        let outcome = keep_result_at(&locks, &root, "sess-1", "exec-1");
        match outcome {
            KeepResultOutcome::Kept { record } => assert_eq!(record.state, ResultState::Kept),
            other => panic!("expected Kept, got {other:?}"),
        }
    }

    #[test]
    fn a_read_only_intent_refuses_to_commit() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        let outcome = commit_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1",
            SessionIntent::Review,
            "improved: something",
        );
        assert!(matches!(outcome, CommitResultOutcome::ReadOnlyIntent));
    }

    #[test]
    fn a_read_only_intent_never_produces_landable_changes_end_to_end() {
        // The end-to-end guarantee task 4.5/the acceptance checklist asks
        // for: for every read-only SessionIntent, a result built from it
        // never reports something to land, and a commit attempt is refused
        // before it ever touches git -- exercised as one combined assertion
        // per intent rather than two separate tests, since the guarantee is
        // that both checks agree.
        use crate::agentdesk::policy;
        for intent in [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Review,
            SessionIntent::Summarize,
        ] {
            assert!(!policy::for_intent(intent).can_write, "{intent:?} must not be able to write");
            let (_dir, root) = temp_root();
            let locks = SessionLocks::new();
            seed_session(&root, "sess-1", "exec-1");
            build_result_at(
                &locks,
                &root,
                "sess-1",
                "exec-1".into(),
                ResultOutcomeKind::Finished,
                None, // read-only intents never provision a worktree
                None,
                None,
                Vec::new(),
                None,
            );
            let records = result::read_results(&root, "sess-1").unwrap();
            let record = result::find_result(&records, "exec-1").unwrap();
            assert!(!record.has_landable_changes(), "{intent:?} result must have nothing to land");

            let commit_outcome = commit_result_at(&locks, &root, "sess-1", "exec-1", intent, "improved: x");
            assert!(
                matches!(commit_outcome, CommitResultOutcome::ReadOnlyIntent),
                "{intent:?} commit must be refused, got {commit_outcome:?}"
            );
        }
    }

    #[test]
    fn committing_a_kept_result_carries_source_and_openspec_trailers() {
        // The other half of the end-to-end guarantee: a real commit made
        // through this flow, for a Fix-intent (can_write) session with an
        // OpenSpec change linked, must carry the `Spec:`/`Assisted-by:`
        // trailers this file drafts -- proving the provenance tie the whole
        // package exists for actually lands in the git object, not just in
        // the drafted text.
        let (_dir, root) = temp_root();
        let wt = worktree_with_a_change();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(wt.path().to_string_lossy().into_owned()),
            Some("agent/exec-1".into()),
            None,
            Vec::new(),
            Some("add-thing".into()),
        );
        let kept = keep_result_at(&locks, &root, "sess-1", "exec-1");
        assert!(matches!(kept, KeepResultOutcome::Kept { .. }));

        let message = crate::airun::complete::commit_message("Add a toggle", "add-thing", "GitHub Copilot");
        let outcome = commit_result_at(&locks, &root, "sess-1", "exec-1", SessionIntent::Fix, &message);
        let (record, oid) = match outcome {
            CommitResultOutcome::Committed { record, oid } => (record, oid),
            other => panic!("expected Committed, got {other:?}"),
        };
        assert_eq!(record.state, ResultState::Committed);
        assert_eq!(record.commit.as_ref().unwrap().oid, oid);

        let repo = git2::Repository::open(wt.path()).unwrap();
        let commit = repo.find_commit(git2::Oid::from_str(&oid).unwrap()).unwrap();
        let commit_message = commit.message().unwrap();
        assert_eq!(trailers::spec_id(commit_message).as_deref(), Some("add-thing"));
        assert_eq!(trailers::assisted_by(commit_message).as_deref(), Some("GitHub Copilot"));
    }

    #[test]
    fn committing_before_keep_is_refused() {
        let (_dir, root) = temp_root();
        let wt = worktree_with_a_change();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(wt.path().to_string_lossy().into_owned()),
            None,
            None,
            Vec::new(),
            None,
        );
        // Never called Keep -- still Reviewing.
        let outcome = commit_result_at(&locks, &root, "sess-1", "exec-1", SessionIntent::Fix, "improved: x");
        assert!(matches!(outcome, CommitResultOutcome::NothingToCommit));
    }

    // -- dir_size_bytes: the number a person decides on. It must be right or
    // absent, never a partial total wearing the word "total". --

    #[test]
    fn dir_size_adds_up_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), vec![0u8; 100]).unwrap();
        std::fs::create_dir_all(dir.path().join("nested/deeper")).unwrap();
        std::fs::write(dir.path().join("nested/b.txt"), vec![0u8; 250]).unwrap();
        std::fs::write(dir.path().join("nested/deeper/c.txt"), vec![0u8; 400]).unwrap();

        assert_eq!(dir_size_bytes(dir.path()), Some(750));
    }

    #[test]
    fn dir_size_of_an_empty_folder_is_zero_not_unknown() {
        // Zero and unknown mean different things: this folder genuinely costs
        // nothing, which is not the same as being unable to look.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(dir_size_bytes(dir.path()), Some(0));
    }

    #[test]
    fn dir_size_is_unknown_rather_than_wrong_when_the_folder_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("never-existed");
        assert_eq!(dir_size_bytes(&gone), None);
    }

    #[test]
    fn count_unexpected_paths_ignores_ordering() {
        // git2 does not promise a stable `statuses()` order, so the same set
        // in a different order must never read as a change.
        let p = |path: &str, status: &str| ResultChangedPath {
            path: path.into(),
            old_path: None,
            status: status.into(),
        };
        let recorded = vec![p("a.txt", "M"), p("b.txt", "A")];
        let live = vec![p("b.txt", "A"), p("a.txt", "M")];
        assert_eq!(count_unexpected_paths(&recorded, &live), None);
    }

    #[test]
    fn count_unexpected_paths_separates_a_new_file_from_a_changed_one() {
        // Untracked is counted apart because it is the one with no way back.
        let p = |path: &str, status: &str| ResultChangedPath {
            path: path.into(),
            old_path: None,
            status: status.into(),
        };
        let recorded = vec![p("a.txt", "M")];
        let live = vec![p("a.txt", "M"), p("mine.txt", "A"), p("theirs.txt", "M")];
        assert_eq!(
            count_unexpected_paths(&recorded, &live),
            Some(UnexpectedPaths { modified: 1, untracked: 1 })
        );
    }

    #[test]
    fn count_unexpected_paths_notices_a_status_that_changed_under_it() {
        // The agent added a file and a person then deleted it: same path,
        // different status, so it is not what the agent left.
        let p = |path: &str, status: &str| ResultChangedPath {
            path: path.into(),
            old_path: None,
            status: status.into(),
        };
        let recorded = vec![p("a.txt", "A")];
        let live = vec![p("a.txt", "D")];
        assert_eq!(
            count_unexpected_paths(&recorded, &live),
            Some(UnexpectedPaths { modified: 1, untracked: 0 })
        );
    }

    #[test]
    fn count_unexpected_paths_allows_the_agent_work_to_have_been_cleaned_up() {
        // Fewer paths than recorded is not a hand edit to refuse over --
        // there is simply less to discard than there was.
        let p = |path: &str, status: &str| ResultChangedPath {
            path: path.into(),
            old_path: None,
            status: status.into(),
        };
        assert_eq!(count_unexpected_paths(&[p("a.txt", "M"), p("b.txt", "A")], &[p("a.txt", "M")]), None);
    }

    /// The bug this iteration set out to prove, stated as the user meets it.
    ///
    /// An agent's work is left UNCOMMITTED in its worktree -- `changed_paths`
    /// is built from `repo.statuses()`, and Keep only flips state -- so
    /// `dirty_count` sees the agent's own output and a human's edit as the
    /// same thing. The result is that Undo, the safety net the whole review
    /// design leans on, refuses on every result that changed anything, and
    /// tells the person their own agent's output is "hand-edited".
    ///
    /// Named for the behaviour a person would report, not the mechanism.
    #[test]
    fn undo_works_on_an_ordinary_result_nobody_touched() {
        let (_dir, root) = temp_root();
        // A worktree holding exactly what an agent that created one file
        // leaves behind: a base commit, plus an uncommitted new file.
        let wt = worktree_with_a_change();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(wt.path().to_string_lossy().into_owned()),
            None,
            None,
            Vec::new(),
            None,
        );

        // The result records the agent's own change, which is the proof that
        // the dirt below is the agent's and not a person's.
        let records = result::read_results(&root, "sess-1").unwrap();
        assert_eq!(records[0].changed_paths.len(), 1, "the agent's own change is the recorded result");

        let outcome = undo_result_at(&locks, &root, "sess-1", "exec-1");
        assert!(
            matches!(outcome, UndoResultOutcome::Discarded { .. }),
            "Undo must work on a result nobody has touched -- got {outcome:?}"
        );
    }

    #[test]
    fn undo_refuses_a_hand_edited_worktree() {
        let (_dir, root) = temp_root();
        // The agent's own output, recorded as this result's changed files.
        let wt = worktree_with_a_change();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(wt.path().to_string_lossy().into_owned()),
            None,
            None,
            Vec::new(),
            None,
        );

        // NOW a person adds a file of their own. This is what the test always
        // claimed to cover: before, it used the agent's own untouched output
        // as the "hand edit", so it passed for the wrong reason and hid the
        // fact that Undo refused on every result.
        std::fs::write(wt.path().join("mine.txt"), "written by a person").unwrap();

        let outcome = undo_result_at(&locks, &root, "sess-1", "exec-1");
        match outcome {
            UndoResultOutcome::RefusedHandEdited { modified, untracked, .. } => {
                assert_eq!(modified + untracked, 1);
            }
            other => panic!("expected RefusedHandEdited, got {other:?}"),
        }
        // The record must still be present and untouched (nothing deleted).
        let records = result::read_results(&root, "sess-1").unwrap();
        assert_eq!(records[0].state, ResultState::Reviewing);
    }

    #[test]
    fn undo_discards_a_clean_worktree() {
        let (_dir, root) = temp_root();
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        std::fs::write(dir.path().join("a.txt"), "one").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[]).unwrap();

        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(dir.path().to_string_lossy().into_owned()),
            None,
            None,
            Vec::new(),
            None,
        );
        let outcome = undo_result_at(&locks, &root, "sess-1", "exec-1");
        assert!(matches!(outcome, UndoResultOutcome::Discarded { .. }));
    }

    #[test]
    fn find_orphaned_flags_a_kept_result_whose_worktree_is_gone() {
        let mut records = Vec::new();
        let mut record = ResultRecord::new_reviewing("exec-1".into(), ResultOutcomeKind::Finished, "2026-01-01T00:00:00Z");
        record.state = ResultState::Kept;
        record.worktree_path = Some("C:/definitely/does/not/exist/xyz".into());
        result::upsert_result(&mut records, record);
        let orphaned = find_orphaned_worktrees_at(&records);
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0].execution_id, "exec-1");
    }

    /// A session with no results file yet is ordinary, not damaged: the sweep
    /// reads it as an empty list and says nothing about it.
    #[test]
    fn a_session_with_no_results_yet_is_swept_without_complaint() {
        let (_dir, root) = temp_root();
        assert!(orphan_records_for_sweep(&root, "sess-never-ran").is_empty());
    }

    /// A damaged sidecar still yields an empty list -- the sweep must not
    /// abandon every other session over one bad file -- but it is a different
    /// event from the case above, and the warning is what makes it one.
    #[test]
    fn a_damaged_results_file_does_not_stop_the_sweep() {
        let (_dir, root) = temp_root();
        let results_dir = root.root_path().join("results");
        std::fs::create_dir_all(&results_dir).unwrap();
        std::fs::write(results_dir.join("sess-1.json"), "{ not json at all").unwrap();

        // Asserted FIRST: without it this test passes for a file that is
        // merely empty, which is the case above and proves nothing about
        // damage. Written the other way round once, and it did exactly that.
        assert!(
            result::read_results(&root, "sess-1").is_err(),
            "the fixture must actually be unreadable"
        );

        // Empty, and the sweep carries on: the whole point is that the next
        // session's real orphans are still found.
        assert!(orphan_records_for_sweep(&root, "sess-1").is_empty());
    }

    #[test]
    fn find_orphaned_ignores_a_committed_results_worktree() {
        // Once committed, cleanup owns removing the worktree deliberately --
        // a missing folder here is the *expected* end state, not something
        // to flag as orphaned/recoverable.
        let mut records = Vec::new();
        let mut record = ResultRecord::new_reviewing("exec-1".into(), ResultOutcomeKind::Finished, "2026-01-01T00:00:00Z");
        record.state = ResultState::Committed;
        record.worktree_path = Some("C:/definitely/does/not/exist/xyz".into());
        result::upsert_result(&mut records, record);
        assert!(find_orphaned_worktrees_at(&records).is_empty());
    }

    #[test]
    fn draft_pull_request_requires_a_commit() {
        let (_dir, root) = temp_root();
        seed_session(&root, "sess-1", "exec-1");
        let locks = SessionLocks::new();
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            None,
            None,
            None,
            Vec::new(),
            None,
        );
        let outcome = draft_pull_request_at(&root, "sess-1", "exec-1");
        assert!(matches!(outcome, DraftPullRequestOutcome::NoCommit));
    }

    #[test]
    fn cleanup_refuses_a_result_that_is_still_reviewing() {
        let (_dir, root) = temp_root();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            None,
            None,
            None,
            Vec::new(),
            None,
        );
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let outcome = cleanup_worktree_at(
            &locks,
            &root,
            &repo,
            &dir.path().to_string_lossy(),
            "sess-1",
            "exec-1",
        );
        assert!(matches!(outcome, CleanupWorktreeOutcome::NotIntegratedOrDiscarded));
    }

    /// Regression test for the Undo-over-Commit defect: once a result has
    /// been committed, Undo must refuse to discard it rather than stamping
    /// `Discarded` over the `Committed` record. Before the fix,
    /// `undo_result_at`'s locked write closure set `entry.state =
    /// ResultState::Discarded` unconditionally -- it never re-checked that
    /// the state was still what the earlier unlocked peek/dirty-scan had
    /// observed. A commit landing between the peek and the write (this test
    /// simulates that by actually committing first, which is the concrete
    /// failure the finding describes: Commit's own real git commit runs
    /// unlocked and can finish while Undo's dirty-scan -- which finds a
    /// post-commit tree clean, same as a never-modified one -- is still in
    /// flight) would have its outcome silently overwritten, orphaning a
    /// real commit from every session record.
    #[test]
    fn undo_refuses_to_discard_a_result_that_was_already_committed() {
        let (_dir, root) = temp_root();
        let wt = worktree_with_a_change();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(wt.path().to_string_lossy().into_owned()),
            Some("agent/exec-1".into()),
            None,
            Vec::new(),
            None,
        );
        let kept = keep_result_at(&locks, &root, "sess-1", "exec-1");
        assert!(matches!(kept, KeepResultOutcome::Kept { .. }));

        let commit_outcome =
            commit_result_at(&locks, &root, "sess-1", "exec-1", SessionIntent::Fix, "improved: land the change");
        let (committed_oid, committed_subject) = match commit_outcome {
            CommitResultOutcome::Committed { record, oid } => {
                assert_eq!(record.state, ResultState::Committed);
                (oid, record.commit.as_ref().unwrap().subject.clone())
            }
            other => panic!("expected Committed, got {other:?}"),
        };

        // Undo fires for the SAME execution after the commit has already
        // landed -- the exact interleaving the finding describes (a stale
        // UI still offering Undo on a result that has since been
        // committed). The worktree is clean (a commit does not dirty the
        // tree), so the dirty-scan alone would not stop the old code from
        // reaching the unconditional `Discarded` write.
        let undo_outcome = undo_result_at(&locks, &root, "sess-1", "exec-1");
        match &undo_outcome {
            UndoResultOutcome::StateChanged { record } => {
                assert_eq!(record.state, ResultState::Committed, "must report the CURRENT (Committed) state");
            }
            other => panic!("expected StateChanged, got {other:?}"),
        }

        // The persisted record must still say Committed with its commit
        // reference intact -- not Discarded, and not missing changed_paths
        // or the commit ref (the mutations the buggy write applied).
        let records = result::read_results(&root, "sess-1").unwrap();
        let record = result::find_result(&records, "exec-1").unwrap();
        assert_eq!(record.state, ResultState::Committed, "undo must not overwrite a Committed result");
        let commit_ref = record.commit.as_ref().expect("commit reference must survive the refused undo");
        assert_eq!(commit_ref.oid, committed_oid);
        assert_eq!(commit_ref.subject, committed_subject);

        // And the commit itself is still reachable in git history --
        // nothing about the refused undo could have unwound it, but this
        // confirms the record and the repository still agree with each
        // other, which is the whole point of refusing instead of
        // overwriting.
        let repo = git2::Repository::open(wt.path()).unwrap();
        assert!(repo.find_commit(git2::Oid::from_str(&committed_oid).unwrap()).is_ok());
    }

    /// Regression test for the same defect, the other direction described
    /// by the finding: a concurrent Undo discarding a result while a Commit
    /// is still mid-flight (Commit's real git commit already succeeded, but
    /// its own record write has not landed yet).
    /// `commit_record_if_still_kept` is the extracted, directly-racable
    /// re-check-and-write step `commit_result_at` calls after its real git
    /// commit finishes -- before the fix it wrote `Committed`
    /// unconditionally with no re-check, so a concurrent discard write that
    /// lands in between would be silently overwritten. This races it
    /// directly against a raw discard write with two real threads and a
    /// barrier (the same technique
    /// `agent_desk::concurrent_start_attempts_yield_exactly_one_started_and_one_already_running`
    /// and `agent_graph::two_concurrent_starts_on_one_proposal_produce_exactly_one_set_of_helpers`
    /// use), since `commit_result_at` as a whole cannot be raced
    /// deterministically -- most of its latency is a real, variable-length
    /// git commit.
    #[test]
    fn commit_refuses_to_overwrite_a_result_that_was_discarded_mid_flight() {
        let (_dir, root) = temp_root();
        let wt = worktree_with_a_change();
        let locks = SessionLocks::new();
        seed_session(&root, "sess-1", "exec-1");
        build_result_at(
            &locks,
            &root,
            "sess-1",
            "exec-1".into(),
            ResultOutcomeKind::Finished,
            Some(wt.path().to_string_lossy().into_owned()),
            Some("agent/exec-1".into()),
            None,
            Vec::new(),
            None,
        );
        let kept = keep_result_at(&locks, &root, "sess-1", "exec-1");
        assert!(matches!(kept, KeepResultOutcome::Kept { .. }));

        // A real commit oid to stamp with -- HEAD is enough; this test is
        // about the record-write race, not about producing a fresh commit.
        let repo = git2::Repository::open(wt.path()).unwrap();
        let oid = repo.head().unwrap().peel_to_commit().unwrap().id();

        let barrier = std::sync::Barrier::new(2);
        let root_ref = &root;
        let locks_ref = &locks;

        let (commit_outcome, discard_outcome) = std::thread::scope(|scope| {
            // Thread A: what `commit_result_at` does AFTER its real git
            // commit succeeds -- re-check `Kept`, then stamp `Committed`.
            let a = scope.spawn(|| {
                barrier.wait();
                commit_record_if_still_kept(locks_ref, root_ref, "sess-1", "exec-1", oid, "improved: land the change")
            });
            // Thread B: what a concurrent Undo does -- a raw discard write
            // under the same session lock, contending for the exact same
            // critical section as thread A.
            let b = scope.spawn(|| {
                barrier.wait();
                locks_ref.with_session_lock("sess-1", || {
                    let mut records = result::read_results(root_ref, "sess-1").unwrap();
                    if let Some(entry) = records.iter_mut().find(|r| r.execution_id == "exec-1") {
                        if entry.state == ResultState::Kept {
                            entry.state = ResultState::Discarded;
                            entry.changed_paths.clear();
                            result::write_results(root_ref, "sess-1", &records).unwrap();
                            return true;
                        }
                    }
                    false
                })
            });
            (a.join().unwrap(), b.join().unwrap())
        });

        // Whichever order the lock granted them, EXACTLY one of the two
        // must have actually changed the state -- never both landing their
        // intended write, which is what "commit refuses to overwrite" means
        // in practice: only one caller's view of the world survives.
        let committed = matches!(commit_outcome, CommitResultOutcome::Committed { .. });
        let commit_saw_conflict = matches!(commit_outcome, CommitResultOutcome::RecordStateChanged { .. });
        assert!(
            committed != discard_outcome || (!committed && !discard_outcome && commit_saw_conflict),
            "exactly one writer's intent may survive: committed={committed} discard_won={discard_outcome}"
        );

        let records = result::read_results(&root, "sess-1").unwrap();
        let record = result::find_result(&records, "exec-1").unwrap();
        if discard_outcome {
            // Undo's discard ran first and won the lock: Commit's re-check
            // must have seen `Discarded` (not `Kept`) and refused to
            // overwrite it with `Committed`.
            assert_eq!(record.state, ResultState::Discarded, "the discard must not be silently overwritten by Commit");
            assert!(record.commit.is_none());
            assert!(
                commit_saw_conflict,
                "commit must report RecordStateChanged when it lost the race, got {commit_outcome:?}"
            );
        } else {
            // Commit ran first (or the discard found the state already
            // `Committed` and declined to touch it, per its own `Kept`
            // guard): the record must be exactly what Commit wrote.
            assert!(committed, "expected Committed when discard did not win, got {commit_outcome:?}");
            assert_eq!(record.state, ResultState::Committed);
            assert_eq!(record.commit.as_ref().unwrap().oid, oid.to_string());
        }
    }
}
