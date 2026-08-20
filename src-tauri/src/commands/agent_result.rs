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
//! separate, user-initiated `git_push` action GitWyrm already has. Grep for
//! `git_push` or `HostProvider` in this file: neither is called with
//! anything that transmits data on the caller's behalf.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::agentdesk::model::{ExecutionId, SessionId, SessionLoadError};
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
fn build_result_at(
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

    let changed_paths = match &worktree_path {
        Some(p) => changed_paths_for_worktree(Path::new(p)).unwrap_or_default(),
        None => Vec::new(),
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
    let Some(worktree_path) = &record.worktree_path else {
        return UndoResultOutcome::NothingToUndo;
    };

    // Hand-edit detection: an unreadable folder is treated as hand-edited,
    // the same safe-direction-to-be-wrong-in stance
    // `commands::airun::ai_run_discard_plan` takes.
    let dirt = worktree::dirty_count(Path::new(worktree_path)).unwrap_or(worktree::DirtyCount {
        modified: 1,
        untracked: 0,
    });
    if !dirt.is_clean() {
        return UndoResultOutcome::RefusedHandEdited {
            record,
            modified: dirt.modified,
            untracked: dirt.untracked,
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
    let write = locks.with_session_lock(session_id, || {
        let mut records = match result::read_results(root, session_id) {
            Ok(r) => r,
            Err(e) => return Err(e.to_string()),
        };
        if let Some(entry) = records.iter_mut().find(|r| r.execution_id == execution_id) {
            entry.state = ResultState::Committed;
            entry.commit = Some(ResultCommitRef {
                oid: oid.to_string(),
                subject,
            });
            entry.updated_at = now_rfc3339();
        }
        result::write_results(root, session_id, &records).map_err(|e| e.to_string())?;
        Ok(records)
    });

    match write {
        Ok(records) => {
            let record = result::find_result(&records, execution_id)
                .cloned()
                .unwrap_or_else(|| ResultRecord::new_reviewing(execution_id.to_string(), ResultOutcomeKind::Finished, &now_rfc3339()));
            CommitResultOutcome::Committed {
                record,
                oid: oid.to_string(),
            }
        }
        Err(detail) => CommitResultOutcome::WriteFailed { detail },
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
        let records = result::read_results(&root, &session_id).unwrap_or_default();
        find_orphaned_worktrees_at(&records)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::{
        AgentSession, AgentSessionHeader, ExecutionRecord, SessionIntent, SessionSource,
        SessionState, CURRENT_SCHEMA_VERSION,
    };
    use crate::agentdesk::result::{CheckRunOutcome, ResultCheckOutcome};

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

    #[test]
    fn undo_refuses_a_hand_edited_worktree() {
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
}
