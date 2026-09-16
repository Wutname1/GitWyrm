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
    // Looked up by `event.session_id` (the `airun` run's own ID, which IS the
    // durable `execution_id` -- see `agentdesk::execution_id_for_run_session`),
    // never by `event.repo_id`. This is the P0 routing fix: two sessions with
    // live executions against the same repository each keep their own
    // mapping, so an event for one never lands in the other. Most events for
    // an unlinked execution return `NoLinkedSession` here cheaply -- resolving
    // the store root first would mean touching the filesystem on every single
    // run event for no reason.
    if links.get(&event.session_id).is_none() {
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
    // Kept before `event` is consumed by routing, for the cleanup below.
    let event_session_id = event.session_id.clone();
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
                        crate::commands::agent_graph::finish_lead_graph_proposal(
                            app,
                            &locks,
                            &root,
                            &durable.session_id,
                            &execution_id,
                        );
                    }
                    // A message the user sent while this turn was still
                    // running was only saved to the transcript; nothing else
                    // ever starts the turn that reads it. Runs after the
                    // proposal hook so a graph it auto-started (or a proposal
                    // now waiting on Start) is visible to the idle check and
                    // wins.
                    //
                    // Called on every terminal state, not only `Finished`. A
                    // turn that stops or fails still must not restart itself,
                    // and does not -- but the person who sent that message was
                    // promised it would be picked up, and used to get silence
                    // instead. `start_queued_follow_up` decides which of the
                    // two happens.
                    if !execution_id.is_empty() {
                        start_queued_follow_up(app, &locks, &root, &durable.session_id, &execution_id);
                    }

                    // The run is over, so its sequence counter is dead weight.
                    // `next_sequence` above adds one entry per run session and
                    // nothing was removing them: the map grew for the life of
                    // the process. Keyed by the RUN session id, which is what
                    // `next_sequence` uses -- `unlink` is keyed by execution
                    // id, so the two are not interchangeable and cleanup could
                    // not simply be added beside it.
                    links.forget_sequence(&event_session_id);
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
        // Nothing was recorded and nothing was meant to be: no session is
        // linked to this repository. The sequence number goes back, because a
        // number spent on nothing still makes the next real event look like it
        // skipped one -- and a skipped number is how a listener decides part
        // of the chat went missing.
        RunEventRouted::NoLinkedSession => {
            links.release_sequence(&event_session_id, sequence);
        }
        // A duplicate or superseded event. `apply_run_event` was reached and
        // decided this event adds nothing, which is correct and silent -- but
        // it recorded no sequence either, so the number goes back for the same
        // reason as above.
        //
        // In particular, `BridgeOutcome::ExecutionSuperseded` is deliberately
        // *not* turned into an `AgentSessionEventKind::ExecutionSuperseded`
        // emit here -- design.md is explicit that a superseded event is
        // ignored on both sides, and nothing was persisted for it to describe
        // (see "Persist an event before emitting it to the UI", also
        // design.md). `AgentSessionEventKind::ExecutionSuperseded` exists in
        // the event enum and `agentSessionStore.ts` handles it, but nothing in
        // this change's scope produces one; see that store's doc comment for
        // the forward-looking reason it stays.
        RunEventRouted::Ignored(_) => {
            links.release_sequence(&event_session_id, sequence);
        }
        RunEventRouted::SessionUnavailable => {
            log::warn!(
                "agent desk session for repo {} could not be read; durable run event dropped",
                event.repo_id
            );
            links.release_sequence(&event_session_id, sequence);
        }
        RunEventRouted::WriteFailed { detail } => {
            // Per task 4.3/design.md: a write failure must never be followed
            // by an emit *of the event*, because a message shown to someone
            // has to be one they can still find after reopening the chat.
            //
            // The sequence number is deliberately NOT released here, and this
            // is the one case where the gap it leaves is telling the truth:
            // this event really is missing from the durable record. Releasing
            // it would hide a genuine loss, which is the failure this whole
            // path exists to avoid.
            //
            // What is emitted is a separate notice carrying no message
            // content, so it says the record is incomplete without pretending
            // anything was saved. Without it the window drew "close this and
            // open it again to load the full record" -- advice that sends
            // someone to the copy that is missing the message, while the copy
            // that has it is the one on their screen.
            log::warn!("agent desk durable write failed, event not persisted: {detail}");
            if let Some(session_id) = links.get(&event_session_id) {
                let _ = app.emit(
                    crate::agentdesk::AGENT_SESSION_EVENT,
                    crate::agentdesk::events::AgentSessionEvent {
                        session_id: session_id.to_string(),
                        execution_id: Some(crate::agentdesk::execution_id_for_run_session(
                            &event_session_id,
                        )),
                        sequence,
                        occurred_at: now.clone(),
                        kind: crate::agentdesk::events::AgentSessionEventKind::NotSaved,
                    },
                );
            }
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

// The real-run command that used to live here (`ai_run_start`) and the
// engine driver behind it (`run_engine`) are gone, along with the worktree
// they provisioned.
//
// They were a second, independent way to run an OpenSpec task: their own
// session registry, their own event stream, their own worktree, and no
// durable Agent Desk session behind any of it. Work started that way
// recorded no usage, was never checked over afterwards, could not be
// recovered after a restart, and produced no result anyone could keep or
// undo -- and every improvement to the real execution path silently missed
// it. Spec Desk's task button now opens an Agent Desk chat through
// `agent_kickoff::agent_session_start`, which is the only execution path
// left. What remains here is the scripted demo (which edits nothing) and
// the event plumbing both surfaces share.

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


// ---------------------------------------------------------------------------
// Queued follow-up: a message sent while the agent was busy starts the next turn
// ---------------------------------------------------------------------------

/// What the next turn should run as when a queued message is picked up:
/// the same mode/team/provider the turn that just ended used, so the user
/// gets the behaviour they last chose rather than a silent reset to defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueuedFollowUp {
    pub mode: crate::agentdesk::policy::ExecutionMode,
    pub team: crate::agentdesk::policy::ExecutionTeam,
    pub provider: Option<String>,
    pub queued_message_count: usize,
}

/// Is a user message still waiting after a turn that did NOT finish cleanly?
///
/// [`queued_follow_up_for`] deliberately refuses to restart a stopped or
/// failed turn, so a message sent during one is never picked up. That rule is
/// right -- a turn that keeps failing must not loop on the same message -- but
/// it used to be enforced in silence. The composer had already told the person
/// their message would be read when the current turn finished, and after Stop
/// nothing ran and nothing was said, leaving the message sitting in the
/// transcript with no explanation.
///
/// So this answers the narrower question the other function cannot: was
/// something left waiting, and is the session now idle enough that nothing
/// else is about to read it? Same queued-message rule (no lead started at or
/// after the message) and same idleness checks; only the ended state differs.
pub(crate) fn stranded_message_count(
    session: &crate::agentdesk::model::AgentSession,
    ended_execution_id: &str,
    live_execution_ids: &[String],
) -> usize {
    use crate::agentdesk::model::{MessageRole, SessionState};

    let Some(ended) = session
        .executions
        .iter()
        .find(|e| e.execution_id == ended_execution_id)
    else {
        return 0;
    };
    // A helper ending says nothing about the lead's queue, and a clean finish
    // is the other function's job.
    if ended.parent_execution_id.is_some() || ended.state == SessionState::Finished {
        return 0;
    }

    let is_running = |state: SessionState| {
        matches!(state, SessionState::Preparing | SessionState::Working | SessionState::NeedsInput)
    };
    // Anything still live may yet read the message, so it is not stranded.
    if live_execution_ids.iter().any(|id| id != ended_execution_id) {
        return 0;
    }
    if session
        .executions
        .iter()
        .any(|e| e.execution_id != ended_execution_id && is_running(e.state))
    {
        return 0;
    }
    if is_running(session.header.state) {
        return 0;
    }

    let lead_starts: Vec<time::OffsetDateTime> = session
        .executions
        .iter()
        .filter(|e| e.parent_execution_id.is_none())
        .filter_map(|e| parse_rfc3339(&e.started_at))
        .collect();
    session
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::User)
        .filter_map(|m| parse_rfc3339(&m.timestamp))
        .filter(|sent_at| !lead_starts.iter().any(|started| started >= sent_at))
        .count()
}

fn parse_rfc3339(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

/// Accepts both spellings in circulation: `ExecutionRecord::mode` is written
/// as the enum's `Debug` name (`"Plan"`) while the composer stores the same
/// choice on the header as it sends it, and nothing forces that casing on an
/// older session file.
fn parse_mode(s: &str) -> Option<crate::agentdesk::policy::ExecutionMode> {
    use crate::agentdesk::policy::ExecutionMode;
    match s.trim().to_ascii_lowercase().as_str() {
        "ask" => Some(ExecutionMode::Ask),
        "plan" => Some(ExecutionMode::Plan),
        "auto" => Some(ExecutionMode::Auto),
        _ => None,
    }
}

/// `"helpers"` is the composer's own word for the lead-plus-helpers team
/// (`agentDeskComposer.ts`'s `teamToExecutionTeam`), so a header preference
/// carries that spelling while an execution record carries `"Lead"`.
fn parse_team(s: &str) -> Option<crate::agentdesk::policy::ExecutionTeam> {
    use crate::agentdesk::policy::ExecutionTeam;
    match s.trim().to_ascii_lowercase().as_str() {
        "solo" => Some(ExecutionTeam::Solo),
        "lead" | "helpers" => Some(ExecutionTeam::Lead),
        _ => None,
    }
}

/// Decides, from the persisted session alone, whether the execution that
/// just ended should be followed by a fresh turn that reads the messages the
/// user sent while it was running.
///
/// A user message counts as queued when no lead/solo execution started at or
/// after it: an execution's prompt is built from the whole transcript, so any
/// turn that started later has already read it. The kickoff message for a
/// turn is always appended before that turn's record is minted, so it is
/// consumed by the very turn it started.
///
/// Returns `None` unless every one of these holds:
/// - `ended_execution_id` names a lead/solo record (no `parent_execution_id`)
///   that ended as `Finished`. A stopped or failed turn never restarts, so a
///   turn that keeps failing cannot loop on the same message.
/// - Nothing else is live: `live_execution_ids` (what the process registry
///   still has, minus the ended execution itself, which is unregistered only
///   after its final event is routed) is otherwise empty, no other record is
///   in `Preparing`/`Working`/`NeedsInput`, and the session header is not in
///   one of those states either (a Plan proposal waiting on Start leaves the
///   header at `NeedsInput`).
/// - At least one user message is queued by the rule above.
pub(crate) fn queued_follow_up_for(
    session: &crate::agentdesk::model::AgentSession,
    ended_execution_id: &str,
    live_execution_ids: &[String],
) -> Option<QueuedFollowUp> {
    use crate::agentdesk::model::{MessageRole, SessionState};
    use crate::agentdesk::policy::{ExecutionMode, ExecutionTeam};

    let ended = session
        .executions
        .iter()
        .find(|e| e.execution_id == ended_execution_id)?;
    if ended.parent_execution_id.is_some() || ended.state != SessionState::Finished {
        return None;
    }

    let is_running = |state: SessionState| {
        matches!(state, SessionState::Preparing | SessionState::Working | SessionState::NeedsInput)
    };
    if live_execution_ids.iter().any(|id| id != ended_execution_id) {
        return None;
    }
    if session
        .executions
        .iter()
        .any(|e| e.execution_id != ended_execution_id && is_running(e.state))
    {
        return None;
    }
    if is_running(session.header.state) {
        return None;
    }

    let lead_starts: Vec<time::OffsetDateTime> = session
        .executions
        .iter()
        .filter(|e| e.parent_execution_id.is_none())
        .filter_map(|e| parse_rfc3339(&e.started_at))
        .collect();
    let queued_message_count = session
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::User)
        .filter_map(|m| parse_rfc3339(&m.timestamp))
        .filter(|sent_at| !lead_starts.iter().any(|started| started >= sent_at))
        .count();
    if queued_message_count == 0 {
        return None;
    }

    let header = &session.header;
    let mode = ended
        .mode
        .as_deref()
        .and_then(parse_mode)
        .or_else(|| header.preferred_mode.as_deref().and_then(parse_mode))
        .unwrap_or(ExecutionMode::Auto);
    let team = ended
        .team
        .as_deref()
        .and_then(parse_team)
        .or_else(|| header.preferred_team.as_deref().and_then(parse_team))
        .unwrap_or(ExecutionTeam::Lead);
    let provider = ended
        .provider
        .clone()
        .or_else(|| header.preferred_provider.clone());

    Some(QueuedFollowUp {
        mode,
        team,
        provider,
        queued_message_count,
    })
}

/// Production half of the queued follow-up: reads the session the ended
/// execution belongs to, asks [`queued_follow_up_for`], and if a turn is
/// owed, notes why in the transcript and starts it through the same
/// `start_execution_at` the composer's Send uses. Best-effort like every
/// other durable-path step in this file: a failure here is logged and noted
/// for the user, never turned into a run failure.
fn start_queued_follow_up(
    app: &tauri::AppHandle,
    locks: &Arc<crate::agentdesk::SessionLocks>,
    root: &crate::agentdesk::store::SessionStoreRoot,
    session_id: &str,
    ended_execution_id: &str,
) {
    let executions = app.state::<crate::agentdesk::ExecutionRegistry>();
    let live = executions.live_executions_for_session(&session_id.to_string());
    let session = match locks.with_session_lock(session_id, || {
        crate::agentdesk::store::read_session(root, session_id)
    }) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("agent desk: queued follow-up skipped, session {session_id} unreadable: {e}");
            return;
        }
    };
    let Some(follow_up) = queued_follow_up_for(&session, ended_execution_id, &live) else {
        // No turn is owed. That is the ordinary case, but it is also what
        // happens after Stop or a failure with a message still waiting, and
        // those two must not look the same to the person who sent it.
        let stranded = stranded_message_count(&session, ended_execution_id, &live);
        if stranded > 0 {
            log::info!(
                "agent desk: session {session_id} has {stranded} message(s) left waiting after a turn that did not finish"
            );
            crate::commands::agent_graph::append_system_note(
                locks,
                root,
                session_id,
                "The last turn ended before reading the message you sent. It is saved above. Send again to start a new turn.",
            );
        }
        return;
    };
    log::info!(
        "agent desk: session {session_id} has {} queued message(s); starting a follow-up turn",
        follow_up.queued_message_count
    );
    crate::commands::agent_graph::append_system_note(
        locks,
        root,
        session_id,
        "Picking up the message you sent while the agent was busy.",
    );

    let links = app.state::<crate::agentdesk::RunSessionLinks>();
    let manager = app.state::<crate::state::RepoManager>();
    let outcome = crate::commands::agent_desk::start_execution_at(
        app,
        locks,
        root,
        links.inner(),
        manager.inner(),
        executions.inner(),
        session_id,
        follow_up.mode,
        follow_up.team,
        follow_up.provider,
    );
    if !matches!(outcome, crate::commands::agent_desk::StartExecutionOutcome::Started { .. }) {
        log::warn!("agent desk: queued follow-up for session {session_id} did not start: {outcome:?}");
        crate::commands::agent_graph::append_system_note(
            locks,
            root,
            session_id,
            "Your message is saved, but GitWyrm could not start a new turn for it. Send it again when you are ready.",
        );
    }
}

#[cfg(test)]
mod queued_follow_up_tests {
    use super::*;
    use crate::agentdesk::model::{
        AgentSession, AgentSessionHeader, ExecutionRecord, MessageKind, MessageRole, SessionIntent,
        SessionMessage, SessionSource, SessionState, CURRENT_SCHEMA_VERSION,
    };
    use crate::agentdesk::policy::{ExecutionMode, ExecutionTeam};

    const T0: &str = "2026-01-01T00:00:00Z";
    const T1: &str = "2026-01-01T00:01:00Z";
    const T2: &str = "2026-01-01T00:02:00Z";
    const T3: &str = "2026-01-01T00:03:00Z";

    fn session(state: SessionState) -> AgentSession {
        AgentSession::new(AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: "sess-1".into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/widgets".into(),
            repo_name: "widgets".into(),
            title: "Fix the bug".into(),
            source: SessionSource::Manual {
                repo_id: "repo-1".into(),
            },
            intent: SessionIntent::Fix,
            state,
            created_at: T0.into(),
            updated_at: T0.into(),
            unread: false,
            changed_file_count: 0,
            active_execution_id: Some("lead-1".into()),
            archived: false,
            graph_started_at: None,
            preferred_provider: None,
            preferred_mode: None,
            preferred_team: None,
        preferred_model: None,
        preferred_effort: None,
        })
    }

    fn user_message(at: &str) -> SessionMessage {
        SessionMessage {
            message_id: format!("m-{at}"),
            segment_id: "seg-1".into(),
            role: MessageRole::User,
            timestamp: at.into(),
            plain_content: "also do this".into(),
            rendered_content: None,
            provider: None,
            model: None,
            kind: MessageKind::User,
            execution_id: None,
            sequence: None,
            import: None,
            targets: Vec::new(),
        }
    }

    fn lead(id: &str, state: SessionState, started_at: &str) -> ExecutionRecord {
        let mut record = ExecutionRecord::minimal(
            id.into(),
            "sess-1".into(),
            None,
            state,
            started_at.into(),
            Some(T3.into()),
            3,
        );
        record.mode = Some("Plan".into());
        record.team = Some("Lead".into());
        record.provider = Some("Codex".into());
        record
    }

    /// The common case: kickoff message, turn starts, a second message lands
    /// mid-turn, the turn finishes cleanly with nothing else running.
    fn finished_with_queued_message() -> AgentSession {
        let mut s = session(SessionState::Finished);
        s.messages.push(user_message(T0));
        s.executions.push(lead("lead-1", SessionState::Finished, T1));
        s.messages.push(user_message(T2));
        s
    }

    #[test]
    fn a_message_sent_after_the_turn_started_is_queued() {
        let s = finished_with_queued_message();
        // The ended execution is still in the registry when its final event
        // is routed; that alone must not read as "something else is live".
        let follow_up = queued_follow_up_for(&s, "lead-1", &["lead-1".to_string()])
            .expect("a message sent mid-turn owes a follow-up");
        assert_eq!(follow_up.queued_message_count, 1, "the kickoff message was consumed by lead-1 itself");
    }

    #[test]
    fn the_kickoff_message_alone_owes_nothing() {
        let mut s = session(SessionState::Finished);
        s.messages.push(user_message(T0));
        s.executions.push(lead("lead-1", SessionState::Finished, T1));
        assert_eq!(queued_follow_up_for(&s, "lead-1", &[]), None);
    }

    #[test]
    fn a_message_already_read_by_a_later_turn_is_not_queued_again() {
        let mut s = session(SessionState::Finished);
        s.messages.push(user_message(T0));
        s.executions.push(lead("lead-1", SessionState::Finished, T1));
        s.messages.push(user_message(T2));
        // The follow-up turn that picked up the T2 message; when IT ends there
        // is nothing left to pick up, so the chain stops here.
        s.executions.push(lead("lead-2", SessionState::Finished, T3));
        assert_eq!(queued_follow_up_for(&s, "lead-2", &[]), None);
    }

    #[test]
    fn no_restart_after_a_stopped_or_failed_turn() {
        for state in [SessionState::Stopped, SessionState::Failed] {
            let mut s = session(state);
            s.messages.push(user_message(T0));
            s.executions.push(lead("lead-1", state, T1));
            s.messages.push(user_message(T2));
            assert_eq!(
                queued_follow_up_for(&s, "lead-1", &[]),
                None,
                "a {state:?} turn must never restart itself on a queued message"
            );
        }
    }

    /// The other half of `no_restart_after_a_stopped_or_failed_turn`.
    ///
    /// Refusing to restart is correct. Refusing in silence was not: the
    /// composer had already promised the message would be picked up when the
    /// current turn finished, and after Stop nothing ran and nothing was
    /// said. These two tests are a pair -- the first pins that no turn
    /// starts, this one pins that the person is told why.
    #[test]
    fn a_message_left_by_a_stopped_or_failed_turn_is_counted_as_stranded() {
        for state in [SessionState::Stopped, SessionState::Failed] {
            let mut s = session(state);
            s.messages.push(user_message(T0));
            s.executions.push(lead("lead-1", state, T1));
            s.messages.push(user_message(T2));
            assert_eq!(
                stranded_message_count(&s, "lead-1", &[]),
                1,
                "a {state:?} turn leaves the message sent after it waiting, and must say so"
            );
        }
    }

    /// A clean finish is the auto-start's job, not this one. Both firing
    /// would append a "send again" note directly above a turn that is in fact
    /// starting by itself.
    #[test]
    fn a_finished_turn_strands_nothing_because_the_follow_up_starts() {
        let s = finished_with_queued_message();
        assert!(
            queued_follow_up_for(&s, "lead-1", &[]).is_some(),
            "fixture must be one the auto-start claims"
        );
        assert_eq!(stranded_message_count(&s, "lead-1", &[]), 0);
    }

    /// Nothing is stranded when there was nothing waiting. Stopping a turn
    /// you sent no follow-up to is the ordinary case and deserves no note.
    #[test]
    fn a_stopped_turn_with_no_queued_message_strands_nothing() {
        let mut s = session(SessionState::Stopped);
        s.messages.push(user_message(T0));
        s.executions.push(lead("lead-1", SessionState::Stopped, T1));
        assert_eq!(stranded_message_count(&s, "lead-1", &[]), 0);
    }

    /// Same idleness rule as the auto-start. Something still running may yet
    /// read the message, so it is not stranded and no note is owed.
    #[test]
    fn nothing_is_stranded_while_another_execution_is_still_live() {
        let mut s = session(SessionState::Stopped);
        s.messages.push(user_message(T0));
        s.executions.push(lead("lead-1", SessionState::Stopped, T1));
        s.messages.push(user_message(T2));
        s.executions.push(ExecutionRecord::minimal(
            "helper-1".into(),
            "sess-1".into(),
            Some("lead-1".into()),
            SessionState::Working,
            T1.into(),
            None,
            0,
        ));
        assert_eq!(
            stranded_message_count(&s, "lead-1", &[]),
            0,
            "a live helper may still read it"
        );
        assert_eq!(
            stranded_message_count(&s, "lead-1", &["helper-1".to_string()]),
            0,
            "and so may one the registry still holds"
        );
    }

    #[test]
    fn no_restart_while_a_helper_is_live_in_the_registry() {
        let mut s = finished_with_queued_message();
        s.executions.push(ExecutionRecord::minimal(
            "helper-1".into(),
            "sess-1".into(),
            Some("lead-1".into()),
            SessionState::Working,
            T1.into(),
            None,
            0,
        ));
        assert_eq!(
            queued_follow_up_for(&s, "lead-1", &["lead-1".to_string(), "helper-1".to_string()]),
            None
        );
    }

    #[test]
    fn no_restart_while_a_helper_record_is_still_working_even_if_unregistered() {
        let mut s = finished_with_queued_message();
        s.executions.push(ExecutionRecord::minimal(
            "helper-1".into(),
            "sess-1".into(),
            Some("lead-1".into()),
            SessionState::Preparing,
            T1.into(),
            None,
            0,
        ));
        assert_eq!(queued_follow_up_for(&s, "lead-1", &[]), None);
    }

    #[test]
    fn no_restart_while_the_session_header_says_it_still_needs_input() {
        let mut s = finished_with_queued_message();
        s.header.state = SessionState::NeedsInput;
        assert_eq!(queued_follow_up_for(&s, "lead-1", &[]), None);
    }

    #[test]
    fn a_finished_helper_never_triggers_a_follow_up() {
        let mut s = finished_with_queued_message();
        s.executions.push(ExecutionRecord::minimal(
            "helper-1".into(),
            "sess-1".into(),
            Some("lead-1".into()),
            SessionState::Finished,
            T1.into(),
            Some(T3.into()),
            0,
        ));
        assert_eq!(queued_follow_up_for(&s, "helper-1", &[]), None);
    }

    #[test]
    fn the_follow_up_reuses_the_ended_turns_mode_team_and_provider() {
        let s = finished_with_queued_message();
        let follow_up = queued_follow_up_for(&s, "lead-1", &[]).unwrap();
        assert_eq!(follow_up.mode, ExecutionMode::Plan);
        assert_eq!(follow_up.team, ExecutionTeam::Lead);
        assert_eq!(follow_up.provider.as_deref(), Some("Codex"));
    }

    #[test]
    fn the_follow_up_falls_back_to_header_preferences_then_defaults() {
        let mut s = finished_with_queued_message();
        let ended = s.executions.iter_mut().find(|e| e.execution_id == "lead-1").unwrap();
        ended.mode = None;
        ended.team = None;
        ended.provider = None;
        s.header.preferred_mode = Some("Ask".into());
        s.header.preferred_team = Some("solo".into());
        s.header.preferred_provider = Some("claude".into());
        let follow_up = queued_follow_up_for(&s, "lead-1", &[]).unwrap();
        assert_eq!(follow_up.mode, ExecutionMode::Ask);
        assert_eq!(follow_up.team, ExecutionTeam::Solo);
        assert_eq!(follow_up.provider.as_deref(), Some("claude"));

        s.header.preferred_mode = None;
        s.header.preferred_team = Some("helpers".into());
        s.header.preferred_provider = None;
        let follow_up = queued_follow_up_for(&s, "lead-1", &[]).unwrap();
        assert_eq!(follow_up.mode, ExecutionMode::Auto, "the composer's own default mode");
        assert_eq!(follow_up.team, ExecutionTeam::Lead, "the composer's 'helpers' spelling maps to Lead");
        assert_eq!(follow_up.provider, None, "no provider means GitWyrm's default, never an invented one");
    }
}
