//! Commands the run console calls, and the events it listens for.
//!
//! Events go out as global Tauri events rather than a per-caller channel,
//! because a gate has to be visible from wherever the user is: the run tab, the
//! main window's spec card, and the status bar all listen to the same stream.
//! A channel would reach only whoever opened it.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{Emitter, Manager, State};

use crate::airun::driver::{summarize, GateAnswer, RunDriver, RunEventKind, RunState, RunStep};
use crate::airun::scripted::{Scenario, ScriptedDriver};
use crate::airun::session::{RunSession, SessionRegistry, StartRefusal};
use crate::error::AppError;

/// The event name every surface listens on.
pub const RUN_EVENT: &str = "ai-run-event";

/// Held per repository so answers and stops reach the running driver.
#[derive(Default)]
pub struct DriverRegistry {
    inner:
        std::sync::Mutex<std::collections::HashMap<String, Arc<std::sync::Mutex<ScriptedDriver>>>>,
}

impl DriverRegistry {
    fn set(&self, repo_id: &str, driver: Arc<std::sync::Mutex<ScriptedDriver>>) {
        self.inner
            .lock()
            .unwrap()
            .insert(repo_id.to_string(), driver);
    }
    fn get(&self, repo_id: &str) -> Option<Arc<std::sync::Mutex<ScriptedDriver>>> {
        self.inner.lock().unwrap().get(repo_id).cloned()
    }
    fn clear(&self, repo_id: &str) {
        self.inner.lock().unwrap().remove(repo_id);
    }

    /// Crate-visible accessor so `commands::agent_desk::stop_execution_at` can
    /// signal the same scripted-demo driver `ai_run_stop` does. Real (non-demo)
    /// runs started through `agent_session_start_execution` never register
    /// here -- they are driven by `airun::cli_run::run_task` directly, stopped
    /// only via the gate-answer channel `gate_answers()` exposes -- so a
    /// lookup miss for a real run's `repo_id` is the normal, expected case,
    /// not a bug: it means there is no *demo* driver to signal.
    pub(crate) fn get_scripted(&self, repo_id: &str) -> Option<Arc<std::sync::Mutex<ScriptedDriver>>> {
        self.get(repo_id)
    }
}

/// Starting a run either gives you the session or says why not.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartOutcome {
    Started {
        session: RunSession,
    },
    /// One is already going. Carries the sentence and the session to route to.
    AlreadyRunning {
        session_id: String,
        summary: String,
    },
}

/// Which scripted scenario to replay.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DemoScenario {
    Happy,
    Gate,
    ProviderExpired,
    Failure,
}

impl From<DemoScenario> for Scenario {
    fn from(d: DemoScenario) -> Self {
        match d {
            DemoScenario::Happy => Scenario::Happy,
            DemoScenario::Gate => Scenario::Gate,
            DemoScenario::ProviderExpired => Scenario::ProviderExpired,
            DemoScenario::Failure => Scenario::Failure,
        }
    }
}

/// Starts a scripted run, for building and checking the console.
///
/// Deliberately named `demo` at every layer, and every session it produces
/// carries a task text saying so. A scripted run that looked real would be a
/// lie told by the product rather than a test fixture, so there is no way to
/// start one that does not announce itself.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_start_demo(
    app: tauri::AppHandle,
    sessions: State<'_, SessionRegistry>,
    drivers: State<'_, DriverRegistry>,
    repo_id: String,
    change_id: String,
    task_number: u32,
    task_text: String,
    branch: String,
    scenario: DemoScenario,
) -> Result<StartOutcome, AppError> {
    // The demo never creates a folder: it edits nothing, so there is nothing to
    // isolate it from.
    let session = match sessions.start(&repo_id, &change_id, task_number, &task_text, &branch, None)
    {
        Ok(s) => s,
        Err(StartRefusal::AlreadyRunning {
            session_id,
            summary,
        }) => {
            return Ok(StartOutcome::AlreadyRunning {
                session_id,
                summary,
            })
        }
    };

    let driver = Arc::new(std::sync::Mutex::new(ScriptedDriver::new(scenario.into())));
    driver.lock().unwrap().start();
    drivers.set(&repo_id, driver.clone());

    // The clock lives here rather than in the driver, so the driver stays
    // synchronous and testable while the pacing still looks like real work.
    let sessions_handle = app.state::<SessionRegistry>();
    let _ = sessions_handle;
    let session_id = session.session_id.clone();
    let repo = repo_id.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let next = {
                let mut d = driver.lock().unwrap();
                d.next_beat()
            };
            let Some(beat) = next else {
                // Either paused at a gate or done. Either way this task stops; a gate
                // answer starts a fresh pump.
                break;
            };
            tokio::time::sleep(Duration::from_millis(beat.delay_ms)).await;
            emit(&app, &repo, &session_id, beat.state, beat.step);
        }
    });

    Ok(StartOutcome::Started { session })
}

/// Answers the open gate and resumes the run.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_answer_gate(
    app: tauri::AppHandle,
    drivers: State<'_, DriverRegistry>,
    repo_id: String,
    session_id: String,
    answer: GateAnswer,
) -> Result<(), AppError> {
    let Some(driver) = drivers.get(&repo_id) else {
        return Ok(());
    };
    driver.lock().unwrap().answer_gate(answer);
    pump(app, driver, repo_id, session_id);
    Ok(())
}

/// Queues a steering note.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_note(
    app: tauri::AppHandle,
    drivers: State<'_, DriverRegistry>,
    repo_id: String,
    session_id: String,
    text: String,
) -> Result<(), AppError> {
    let Some(driver) = drivers.get(&repo_id) else {
        return Ok(());
    };
    {
        let mut d = driver.lock().unwrap();
        // A note while paused must not resume the run: the gate is still open.
        if d.is_paused() {
            d.note(text);
            return Ok(());
        }
        d.note(text);
    }
    pump(app, driver, repo_id, session_id);
    Ok(())
}

/// Stops the run.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_stop(
    app: tauri::AppHandle,
    drivers: State<'_, DriverRegistry>,
    repo_id: String,
    session_id: String,
) -> Result<(), AppError> {
    let Some(driver) = drivers.get(&repo_id) else {
        return Ok(());
    };
    let ending = {
        let mut d = driver.lock().unwrap();
        d.stop();
        d.emitted().last().cloned()
    };
    if let Some((step, state)) = ending {
        emit(&app, &repo_id, &session_id, state, step);
    }
    drivers.clear(&repo_id);
    Ok(())
}

/// The run the console should show for a repository, if any.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_current(
    sessions: State<'_, SessionRegistry>,
    repo_id: String,
) -> Result<Option<RunSession>, AppError> {
    Ok(sessions.get(&repo_id))
}

/// What a finished run offers for approval.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RunCompletion {
    /// The commit message the user is about to approve, trailers and all. Editable
    /// before committing -- this is a draft, not a decision.
    pub message: String,
    /// Line in tasks.md the run's task sits on, so ticking and un-ticking both
    /// target the same line rather than re-deriving it.
    pub task_line: Option<u32>,
}

/// The message and task line a finished run should offer. **Commits nothing.**
///
/// Kept separate from the commit itself so the user always sees what they are
/// approving first. An agent that could compose and commit in one step would be
/// a different and much harder thing to trust inside a git client.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_completion(
    manager: State<'_, crate::state::RepoManager>,
    sessions: State<'_, SessionRegistry>,
    repo_id: String,
    provider: String,
) -> Result<Option<RunCompletion>, AppError> {
    let Some(session) = sessions.get(&repo_id) else {
        return Ok(None);
    };
    let root = manager.get(&repo_id)?.path.clone();
    let change_id = session.change_id.clone();
    let task_text = session.task_text.clone();

    let task_line = tauri::async_runtime::spawn_blocking(move || {
        let dir = crate::openspec::openspec_dir(&root)?;
        let change =
            crate::openspec::parse::parse_change_dir(&dir.join("changes").join(&change_id))?;
        // Match on the text the session recorded: the run's own task, not whatever
        // is currently next, which may have moved while the run worked.
        change
            .tasks
            .iter()
            .find(|t| t.text == task_text)
            .map(|t| t.line)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

    Ok(Some(RunCompletion {
        message: crate::airun::complete::commit_message(
            &session.task_text,
            &session.change_id,
            &provider,
        ),
        task_line,
    }))
}

/// What discarding an isolated run would do to its folder.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunDiscardPlan {
    /// The run did not have its own folder; discard is the ordinary path.
    NoFolder,
    /// The folder holds only what the run wrote, so it can go.
    FolderCanGo { path: String, folder_name: String },
    /// The user edited files in there by hand. A discard throws away the AI's
    /// result, not theirs, so this is never deleted without asking.
    HandEdited {
        path: String,
        folder_name: String,
        modified: u32,
        untracked: u32,
    },
}

/// What discarding this run would do, asked before anything is deleted.
///
/// The hand-edit check is the whole point: the user may have opened the run's
/// folder and worked in it, and a discard is about throwing away the AI's
/// result, not theirs.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_discard_plan(
    sessions: State<'_, SessionRegistry>,
    repo_id: String,
) -> Result<RunDiscardPlan, AppError> {
    let Some(session) = sessions.get(&repo_id) else {
        return Ok(RunDiscardPlan::NoFolder);
    };
    let (Some(path), Some(folder_name)) = (session.worktree_path, session.worktree_name) else {
        return Ok(RunDiscardPlan::NoFolder);
    };

    tauri::async_runtime::spawn_blocking(move || {
        // An unreadable folder is treated as hand-edited: refusing to delete
        // something we cannot inspect is the safe direction to be wrong in.
        let dirt = crate::git::worktree::dirty_count(std::path::Path::new(&path)).unwrap_or(
            crate::git::worktree::DirtyCount {
                modified: 1,
                untracked: 0,
            },
        );
        if dirt.is_clean() {
            Ok(RunDiscardPlan::FolderCanGo { path, folder_name })
        } else {
            Ok(RunDiscardPlan::HandEdited {
                path,
                folder_name,
                modified: dirt.modified,
                untracked: dirt.untracked,
            })
        }
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Clears a finished run so the repository can start another.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_clear(
    sessions: State<'_, SessionRegistry>,
    drivers: State<'_, DriverRegistry>,
    repo_id: String,
) -> Result<(), AppError> {
    sessions.clear(&repo_id);
    drivers.clear(&repo_id);
    Ok(())
}

/// Resumes emitting after a gate answer or a note.
fn pump(
    app: tauri::AppHandle,
    driver: Arc<std::sync::Mutex<ScriptedDriver>>,
    repo_id: String,
    session_id: String,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let next = {
                let mut d = driver.lock().unwrap();
                d.next_beat()
            };
            let Some(beat) = next else { break };
            tokio::time::sleep(Duration::from_millis(beat.delay_ms)).await;
            emit(&app, &repo_id, &session_id, beat.state, beat.step);
        }
    });
}

/// Records an event and sends it to every window.
///
/// The registry is asked first: it drops anything from a session that is no
/// longer current, so a driver still finishing cannot write into a newer run's
/// console.
///
/// After the existing `ai-run-event` emission (unchanged -- task 4.5), this
/// also routes the same event toward the durable Agent Desk store via
/// `agentdesk::route_run_event`. That path is additive and self-contained: a
/// repository with no linked durable session (the case for every run today,
/// since nothing yet calls `RunSessionLinks::link`) takes the
/// `NoLinkedSession` branch and does nothing further, so this call cannot
/// change what already happens on `RUN_EVENT`, only add to it.
pub(crate) fn emit(app: &tauri::AppHandle, repo_id: &str, session_id: &str, state: RunState, step: RunStep) {
    let event = RunEventKind {
        repo_id: repo_id.to_string(),
        session_id: session_id.to_string(),
        state,
        summary: summarize(&step),
        step,
    };
    if !app.state::<SessionRegistry>().record(&event) {
        return;
    }
    // Logged because a run spans two windows, and "did the event go out at all"
    // is the first question worth answering when one of them looks blank.
    log::debug!(
        "run event: repo={} session={} state={:?}",
        event.repo_id,
        event.session_id,
        event.state
    );
    let _ = app.emit(RUN_EVENT, event.clone());

    route_to_agent_desk(app, &event);
}

/// The Agent Desk equivalent of [`emit`], for executions
/// `commands::agent_desk::start_execution_at` starts.
///
/// `emit` above gates every event on `SessionRegistry::record`, which only
/// accepts events from a run that was registered with `SessionRegistry::start`
/// -- the "one run per repository" bookkeeping the AI-run console tab owns.
/// Agent Desk executions never call `SessionRegistry::start` (they have their
/// own concurrency guard, `record_execution_if_not_running`, keyed by the
/// durable session rather than by repository), so routing them through `emit`
/// means `record` always finds no registered session for the repo and drops
/// every event before `route_to_agent_desk` ever runs -- the execution's
/// state then never leaves `Preparing`, no matter what the engine reports.
///
/// This function is `emit` minus that gate: it still emits `RUN_EVENT` (so the
/// AI-run console tab, if the same repo happens to be open there too, sees
/// the same activity it always would have) and still routes to the durable
/// store, but never touches `SessionRegistry` -- an Agent Desk execution does
/// not participate in that registry's "one run per repo" rule at all.
pub(crate) fn emit_agent_desk_only(
    app: &tauri::AppHandle,
    repo_id: &str,
    session_id: &str,
    state: RunState,
    step: RunStep,
) {
    let event = RunEventKind {
        repo_id: repo_id.to_string(),
        session_id: session_id.to_string(),
        state,
        summary: summarize(&step),
        step,
    };
    log::debug!(
        "agent desk run event: repo={} session={} state={:?}",
        event.repo_id,
        event.session_id,
        event.state
    );
    let _ = app.emit(RUN_EVENT, event.clone());

    route_to_agent_desk(app, &event);
}

/// The durable-path half of [`emit`]. Split out so a failure or an
/// unavailable store root can never touch the `ai-run-event` emission above
/// it -- by the time this runs, `RUN_EVENT` has already gone out either way.
fn route_to_agent_desk(app: &tauri::AppHandle, event: &RunEventKind) {
    use crate::agentdesk::{
        route_run_event, RunEventRouted, RunSessionLinks, SessionLocks, SessionStoreRoot,
    };

    let links = app.state::<RunSessionLinks>();
    // No repository is linked yet in normal operation (nothing calls
    // `RunSessionLinks::link` in this change), so this is almost always a
    // single uncontended map lookup that returns `NoLinkedSession` -- resolving
    // the store root first would mean touching the filesystem on every single
    // run event for no reason.
    if links.get(&event.repo_id).is_none() {
        return;
    }

    let root = match SessionStoreRoot::resolve(app) {
        Ok(root) => root,
        Err(e) => {
            log::warn!("agent desk store unavailable, durable run event dropped: {e}");
            return;
        }
    };

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
    let sequence = links.next_sequence(&event.session_id);
    let locks = app.state::<Arc<SessionLocks>>();

    match route_run_event(&root, &links, &locks, sequence, &now, event) {
        RunEventRouted::Persisted { event: durable } => {
            // R3.7/1.3: the moment an execution reaches a terminal state
            // (Finished/Stopped/Failed -- a helper reaching a conflicted
            // integration is handled separately by
            // `commands::agent_graph::advance_graph_after_helper_completion`,
            // which calls `agent_result::build_result_at` itself once it
            // knows the conflict outcome), automatically build and persist
            // its `ResultRecord` so the mounted `ResultReviewPanel` has
            // something real to show instead of "No result yet" -- this is
            // the ONLY place that call was previously missing from: nothing
            // else in the completion path built one.
            if let crate::agentdesk::events::AgentSessionEventKind::StateChanged { state } = &durable.kind {
                if let Some(outcome) = terminal_result_outcome(*state) {
                    let execution_id = durable.execution_id.clone().unwrap_or_default();
                    build_result_for_completed_execution(
                        &root,
                        locks.inner(),
                        &durable.session_id,
                        &execution_id,
                        outcome,
                    );
                    // R6.1: the OTHER half of a Plan-mode lead's turn -- once
                    // it finishes cleanly, check whether it actually
                    // produced a graph proposal and persist or visibly
                    // refuse it. Only on `Finished` (a stopped/failed turn
                    // has no complete reply to parse); `execution_id` empty
                    // is defensive against a malformed durable event and
                    // mirrors `build_result_for_completed_execution`'s own
                    // early return.
                    if matches!(state, crate::agentdesk::model::SessionState::Finished) && !execution_id.is_empty() {
                        crate::commands::agent_graph::finish_plan_mode_proposal(
                            &locks,
                            &root,
                            &durable.session_id,
                            &execution_id,
                        );
                    }
                }
            }
            let _ = app.emit(crate::agentdesk::AGENT_SESSION_EVENT, durable);
        }
        // Ignored per design.md ("Duplicate event sequence: ignore it" /
        // "Event for replaced execution: ignore it in both backend and
        // frontend"): correct, silent behavior, not a fault. In particular,
        // `BridgeOutcome::ExecutionSuperseded` is deliberately *not* turned
        // into an `AgentSessionEventKind::ExecutionSuperseded` emit here --
        // design.md is explicit that a superseded event is ignored on both
        // sides, and nothing was persisted for it to describe (see
        // "Persist an event before emitting it to the UI", also design.md).
        // `AgentSessionEventKind::ExecutionSuperseded` exists in the event
        // enum and `agentSessionStore.ts` handles it, but nothing in this
        // change's scope produces one; see that store's doc comment for the
        // forward-looking reason it stays.
        RunEventRouted::NoLinkedSession
        | RunEventRouted::Ignored(_) => {}
        RunEventRouted::SessionUnavailable => {
            log::warn!(
                "agent desk session for repo {} could not be read; durable run event dropped",
                event.repo_id
            );
        }
        RunEventRouted::WriteFailed { detail } => {
            // Per task 4.3/design.md: a write failure must never be followed
            // by an emit. There is nothing more to do here than log it --
            // `ai-run-event` already carried the event to the live UI, so the
            // run itself is unaffected, only its durable copy is missing.
            log::warn!("agent desk durable write failed, event not persisted: {detail}");
        }
    }
}

/// Maps a durable [`crate::agentdesk::model::SessionState`] to the
/// [`crate::agentdesk::result::ResultOutcomeKind`] a completed execution's
/// result should carry, or `None` for every non-terminal state (Draft/Ready/
/// Preparing/Working/NeedsInput never produce a result -- there is nothing to
/// review yet).
fn terminal_result_outcome(
    state: crate::agentdesk::model::SessionState,
) -> Option<crate::agentdesk::result::ResultOutcomeKind> {
    use crate::agentdesk::model::SessionState;
    use crate::agentdesk::result::ResultOutcomeKind;
    match state {
        SessionState::Finished => Some(ResultOutcomeKind::Finished),
        SessionState::Stopped => Some(ResultOutcomeKind::Stopped),
        SessionState::Failed => Some(ResultOutcomeKind::Failed),
        // `Interrupted` is a restart-recovery state, not a live completion --
        // `session_recovery`/`reconcile_executions` are what set it, never a
        // live `RunStep::Ended`, so it never reaches this function via the
        // durable event path in the first place. Every other state is
        // ongoing.
        _ => None,
    }
}

/// R3.7's actual automatic-result-build step: reads the session this
/// execution just reached a terminal state on, pulls the provenance
/// (worktree/branch/base_oid) and OpenSpec change id `start_execution_at`
/// already stamped onto the `ExecutionRecord` when the run began, and calls
/// the same `agent_result::build_result_at` a user's own "refresh" action
/// would -- so the very first time `ResultReviewPanel` queries
/// `agent_result_list` after a run ends, there is already a `Reviewing`
/// record waiting, not an empty list.
///
/// Best-effort by design, matching every other durable-path write in this
/// file: a read/build failure here is logged, never surfaced as a run
/// failure -- the run itself already finished and its outcome is already
/// visible in the transcript via the `StateChanged` event this is called
/// alongside. A missing result is a degraded review experience, not a lost
/// run.
fn build_result_for_completed_execution(
    root: &crate::agentdesk::store::SessionStoreRoot,
    locks: &Arc<crate::agentdesk::SessionLocks>,
    session_id: &str,
    execution_id: &str,
    outcome: crate::agentdesk::result::ResultOutcomeKind,
) {
    if execution_id.is_empty() {
        return;
    }
    let session = match crate::agentdesk::store::read_session(root, session_id) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("agent desk result build: could not read session {session_id}: {e}");
            return;
        }
    };
    let Some(record) = session.executions.iter().find(|e| e.execution_id == execution_id) else {
        // The bridge always creates an `ExecutionRecord` before this fires
        // (`apply_run_event`'s `find_or_start_execution` runs earlier in the
        // very same `route_run_event` call), so this should not happen in
        // practice -- logged rather than silently dropped so a future
        // regression here is visible.
        log::warn!("agent desk result build: no execution record for {execution_id} in session {session_id}");
        return;
    };

    let worktree_path = record.worktree_path.clone();
    let branch = record.branch.clone();
    let base_oid = record.base_oid.clone();
    let checks = crate::commands::agent_result::checks_for_execution(&session, execution_id);
    let openspec_change_id = crate::commands::agent_desk::openspec_change_id_of(&session.header.source);

    let _ = crate::commands::agent_result::build_result_at(
        locks,
        root,
        session_id,
        execution_id.to_string(),
        outcome,
        worktree_path,
        branch,
        base_oid,
        checks,
        openspec_change_id,
    );
}

/// Make a disposable folder for a run to work in, on its own branch.
///
/// Returns (absolute path, folder name).
///
/// A branch rather than a detached HEAD: if the app dies mid-run the work is
/// still reachable by name, and "the task worked on this branch" is something
/// that can be explained to someone who has never heard of a worktree. Detached
/// would leave commits reachable only by sha, which is exactly the state people
/// need help getting out of.
///
/// The new branch starts at the tip of the branch the run is pinned to, so the
/// result is a diff against the work the user is actually doing.
fn provision_run_worktree(
    open: &std::sync::Arc<crate::state::OpenRepo>,
    branch: &str,
    task_text: &str,
) -> Result<(String, String), AppError> {
    use crate::git::worktree;

    let (main_path, run_branch, path) = {
        let repo = open.repo.lock().unwrap();
        let main = worktree::main_workdir(&repo)
            .ok_or_else(|| AppError::Other("this project has no working folder".into()))?;

        // Name it after the task so a leftover folder says what it was for.
        let slug = worktree::branch_slug(&task_text.chars().take(40).collect::<String>());
        let mut candidate = format!("gitwyrm-task/{slug}");
        let mut n = 2;
        while repo
            .find_branch(&candidate, git2::BranchType::Local)
            .is_ok()
        {
            candidate = format!("gitwyrm-task/{slug}-{n}");
            n += 1;
            if n > 100 {
                break;
            }
        }
        let path = worktree::suggest_path(&main, &candidate);
        (main.to_string_lossy().into_owned(), candidate, path)
    };

    worktree::add(&main_path, &path, &run_branch, true, Some(branch)).map_err(|e| {
        AppError::Other(format!(
            "The task could not be given its own folder to work in, so it was not started: {e}"
        ))
    })?;

    // Mark it so the list can label it and discard can tell a run's folder from
    // one the user made. The marker lives in the admin directory, never in the
    // working folder, so it cannot be committed and does not read as a hand edit.
    {
        let repo = open.repo.lock().unwrap();
        if let Some(name) = worktree::list(&repo, None)
            .into_iter()
            .find(|w| {
                worktree::paths_equal(std::path::Path::new(&w.path), std::path::Path::new(&path))
            })
            .map(|w| w.name)
        {
            let _ = worktree::mark_as_run_worktree(&repo, &name);
        }
    }

    let folder_name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| run_branch.clone());
    Ok((path, folder_name))
}

/// Starts a real run: the engine, against the user's default provider.
///
/// Separate command from `ai_run_start_demo` on purpose. The demo replays a
/// script and must never be mistaken for this; this spends the user's AI
/// credits and edits their files.
#[tauri::command]
#[specta::specta]
pub async fn ai_run_start(
    app: tauri::AppHandle,
    manager: State<'_, crate::state::RepoManager>,
    sessions: State<'_, SessionRegistry>,
    repo_id: String,
    change_id: String,
    task_index: u32,
    task_number: u32,
    task_text: String,
    branch: String,
    own_folder: bool,
) -> Result<StartOutcome, AppError> {
    let open = manager.get(&repo_id)?;
    let user_root = open.path.clone();

    // Provision the run's own folder BEFORE the session exists, so a failure to
    // create one fails the start outright. Quietly falling back to the user's
    // checkout would be the worst outcome: they asked to keep working in their
    // own files and would be told the run is elsewhere while it edits theirs.
    let worktree = if own_folder {
        Some(provision_run_worktree(&open, &branch, &task_text)?)
    } else {
        None
    };

    // Everything the run touches is rooted here. The guardrails' in-repo check
    // roots on the same path, so the run's boundary moves with it.
    let root = worktree
        .as_ref()
        .map(|(p, _)| std::path::PathBuf::from(p))
        .unwrap_or_else(|| user_root.clone());

    let session = match sessions.start(
        &repo_id,
        &change_id,
        task_number,
        &task_text,
        &branch,
        worktree.clone(),
    ) {
        Ok(s) => s,
        Err(StartRefusal::AlreadyRunning {
            session_id,
            summary,
        }) => {
            return Ok(StartOutcome::AlreadyRunning {
                session_id,
                summary,
            })
        }
    };

    // tasks.md is read from the run's own folder too. Reading the user's copy
    // while writing to the worktree would have the run tick a checkbox in a file
    // it is not otherwise allowed to touch.
    let tasks_file = root
        .join("openspec")
        .join("changes")
        .join(&change_id)
        .join("tasks.md");

    let (answer_tx, answer_rx) = std::sync::mpsc::channel::<GateAnswer>();
    // This legacy path has at most one execution per repository at a time,
    // so `(repo_id, repo_id)` is a faithful encoding of its old single-key
    // behavior under the new `(session, execution)` key shape -- see
    // `gate_answers`'s own doc comment.
    GATE_ANSWERS
        .lock()
        .unwrap()
        .insert((repo_id.clone(), repo_id.clone()), answer_tx);

    let session_id = session.session_id.clone();
    let repo = repo_id.clone();
    let app_for_sink = app.clone();
    let sink_repo = repo_id.clone();
    let sink_session = session_id.clone();
    let sink: crate::airun::engine::Sink = std::sync::Arc::new(move |state, step| {
        emit(&app_for_sink, &sink_repo, &sink_session, state, step);
    });

    // The engine's loop is blocking at the tool boundary (a gate waits on a
    // channel), so it runs on a blocking thread rather than the async runtime.
    tauri::async_runtime::spawn(async move {
        let outcome = tokio::task::spawn_blocking(move || {
            run_engine(root, tasks_file, task_index, task_text, sink, answer_rx)
        })
        .await;
        if let Err(e) = outcome {
            log::error!("run task panicked: {e}");
            emit(
                &app,
                &repo,
                &session_id,
                RunState::Failed,
                RunStep::Ended {
                    state: RunState::Failed,
                    detail: "This run stopped unexpectedly. Nothing was committed and your own \
                   work is untouched."
                        .into(),
                },
            );
        }
        GATE_ANSWERS.lock().unwrap().remove(&(repo.clone(), repo));
    });

    Ok(StartOutcome::Started { session })
}

/// Gate answers, per (session, execution), so a real Agent Desk run's answer
/// channel can be reached without colliding with a sibling execution.
///
/// R6.6: "Key approvals by session, execution, and gate ID." Before this
/// change the map was keyed by `repo_id` alone -- correct for the demo
/// console (one scripted run per repository) but wrong the moment a session
/// can carry more than one concurrent execution against the same repository
/// (a lead plus helpers, `ExecutionRecord::parent_execution_id`): two
/// executions hitting a gate around the same time would insert into the SAME
/// map entry, and the second insert would silently replace the first's
/// sender -- an approval typed for the lead's gate could resume a helper's
/// run instead, or vice versa, with no error either way. `ai_run_start`
/// (the demo/legacy real-run path, still `repo_id`-only, unchanged) is
/// unaffected: it never has more than one execution per repository at a
/// time, so `(repo_id.clone(), repo_id.clone())` below is a faithful,
/// harmless encoding of its existing single-key behavior -- see that
/// call site's own comment.
pub(crate) type GateAnswerKey = (String, String);

static GATE_ANSWERS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<GateAnswerKey, std::sync::mpsc::Sender<GateAnswer>>>,
> = std::sync::LazyLock::new(Default::default);

/// Crate-visible accessor to the same gate-answer registry `ai_run_start`
/// populates, so `commands::agent_desk::start_execution_at` (and, for R6.3,
/// every helper launch) registers a real engine's answer channel exactly the
/// same way -- one registry, not a second one nothing else would know about.
pub(crate) fn gate_answers(
) -> &'static std::sync::Mutex<std::collections::HashMap<GateAnswerKey, std::sync::mpsc::Sender<GateAnswer>>>
{
    &GATE_ANSWERS
}

/// Builds the transport and drives the task.
///
/// The Copilot CLI runs its own agent loop, so this hands the whole task to it
/// rather than feeding it single turns -- see `airun::cli_run`. Our loop in
/// `ai::agent::run` is for transports that answer one turn at a time, which is
/// the API path.
fn run_engine(
    root: std::path::PathBuf,
    _tasks_file: std::path::PathBuf,
    _task_index: u32,
    task_text: String,
    sink: crate::airun::engine::Sink,
    answers: std::sync::mpsc::Receiver<GateAnswer>,
) {
    use crate::ai::agent::cli_agent::CliAgent;

    let agent = match CliAgent::discover(root) {
        Ok(a) => a,
        Err(e) => {
            let detail = crate::ai::agent::select::plain_explanation(&e);
            sink(
                RunState::Failed,
                RunStep::Ended {
                    state: RunState::Failed,
                    detail,
                },
            );
            return;
        }
    };

    let prompt = format!(
        "{}

The task:
{}",
        crate::ai::agent::run::SYSTEM_PROMPT,
        task_text
    );

    // The legacy OpenSpec task-run console (`ai_run_start`) has no
    // `SessionIntent` of its own -- it always ran fully permissive before
    // `ExecutionPolicy` existed, driving an accepted task to completion the
    // same way a Fix session does. Resolving a `Fix`-shaped policy here
    // (rather than gating this console for the first time) keeps that
    // existing, unrestricted behavior unchanged; `started: true` because
    // this console has no Plan-before-Start concept -- it starts running
    // immediately, same as it always has.
    let policy = crate::agentdesk::policy::ExecutionPolicy::resolve(
        crate::agentdesk::model::SessionIntent::Fix,
        crate::agentdesk::policy::ExecutionMode::Auto,
        crate::agentdesk::policy::ExecutionTeam::Solo,
        None,
    )
    .expect("no provider override is passed here, so resolution cannot fail");

    let rt = tokio::runtime::Handle::current();
    // The legacy OpenSpec task console has no Stop of its own -- it always ran
    // to completion and still does. It gets a handle nothing ever signals
    // rather than a special no-cancel code path, so `run_task` has exactly one
    // shape and the console cannot drift from the real execution path.
    rt.block_on(crate::airun::cli_run::run_task(
        &agent,
        &prompt,
        sink,
        answers,
        policy,
        true,
        crate::airun::cli_run::CancelHandle::new(),
        // No budget: this legacy console has no helper/graph concept at all.
        None,
    ));
}
