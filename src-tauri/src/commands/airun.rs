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
  inner: std::sync::Mutex<std::collections::HashMap<String, Arc<std::sync::Mutex<ScriptedDriver>>>>,
}

impl DriverRegistry {
  fn set(&self, repo_id: &str, driver: Arc<std::sync::Mutex<ScriptedDriver>>) {
    self.inner.lock().unwrap().insert(repo_id.to_string(), driver);
  }
  fn get(&self, repo_id: &str) -> Option<Arc<std::sync::Mutex<ScriptedDriver>>> {
    self.inner.lock().unwrap().get(repo_id).cloned()
  }
  fn clear(&self, repo_id: &str) {
    self.inner.lock().unwrap().remove(repo_id);
  }
}

/// Starting a run either gives you the session or says why not.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartOutcome {
  Started { session: RunSession },
  /// One is already going. Carries the sentence and the session to route to.
  AlreadyRunning { session_id: String, summary: String },
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
  let session = match sessions.start(&repo_id, &change_id, task_number, &task_text, &branch) {
    Ok(s) => s,
    Err(StartRefusal::AlreadyRunning { session_id, summary }) => {
      return Ok(StartOutcome::AlreadyRunning { session_id, summary })
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
  let Some(driver) = drivers.get(&repo_id) else { return Ok(()) };
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
  let Some(driver) = drivers.get(&repo_id) else { return Ok(()) };
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
  let Some(driver) = drivers.get(&repo_id) else { return Ok(()) };
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
fn emit(
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
  let _ = app.emit(RUN_EVENT, event);
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
) -> Result<StartOutcome, AppError> {
  let open = manager.get(&repo_id)?;
  let root = open.path.clone();

  let session =
    match sessions.start(&repo_id, &change_id, task_number, &task_text, &branch) {
      Ok(s) => s,
      Err(StartRefusal::AlreadyRunning { session_id, summary }) => {
        return Ok(StartOutcome::AlreadyRunning { session_id, summary })
      }
    };

  let tasks_file = root
    .join("openspec")
    .join("changes")
    .join(&change_id)
    .join("tasks.md");

  let (answer_tx, answer_rx) = std::sync::mpsc::channel::<GateAnswer>();
  GATE_ANSWERS.lock().unwrap().insert(repo_id.clone(), answer_tx);

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
    GATE_ANSWERS.lock().unwrap().remove(&repo);
  });

  Ok(StartOutcome::Started { session })
}

/// Gate answers, per repository, so `ai_run_answer_gate` can reach a live run.
static GATE_ANSWERS: std::sync::LazyLock<
  std::sync::Mutex<std::collections::HashMap<String, std::sync::mpsc::Sender<GateAnswer>>>,
> = std::sync::LazyLock::new(Default::default);

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
      sink(RunState::Failed, RunStep::Ended { state: RunState::Failed, detail });
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

  let rt = tokio::runtime::Handle::current();
  rt.block_on(crate::airun::cli_run::run_task(&agent, &prompt, sink, answers));
}
