//! Tauri commands for the OpenSpec integration.
//!
//! Thin wrappers: the work lives in `crate::openspec`. Filesystem reads run on
//! the blocking pool like every other repo command, and the repo handle is only
//! used for its path -- nothing here touches git.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::error::AppError;
use crate::openspec::{self, archive_repair, ask, cli, draft, history, parse, write};
use crate::state::RepoManager;

/// Whether this repository uses OpenSpec, and whether the CLI is around.
///
/// Fields stay snake_case, matching every other type that crosses this boundary
/// (`has_more`, `target_sha`, ...). No `rename_all`: specta and serde then agree,
/// which is what keeps the generated type honest about the wire format.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OpenspecStatus {
  /// False for every repo without an `openspec/` folder -- the UI shows nothing
  /// at all in that case.
  pub present: bool,
  /// Active (non-archived) change count, for the sidebar and status bar.
  pub active_count: u32,
  /// Archived change count, for the Desk's archive link.
  pub archived_count: u32,
  pub cli: cli::CliInfo,
}

/// The built-in archive-commit message template, exposed so the settings UI
/// can show it as the placeholder and restore it with "Reset to default".
#[tauri::command]
#[specta::specta]
pub fn openspec_default_archive_commit_template() -> String {
  cli::DEFAULT_ARCHIVE_COMMIT_TEMPLATE.to_string()
}

/// Resolves a repo id to its working directory.
fn repo_root(manager: &RepoManager, repo_id: &str) -> Result<PathBuf, AppError> {
  Ok(manager.get(repo_id)?.path.clone())
}

/// Re-probe for the OpenSpec CLI, ignoring the cached answer.
///
/// The probe is cached because it can shell out to `npx` and runs on every
/// watcher event. But installing the tool after reading the "not installed"
/// hint is the expected next step, so there has to be a way to notice without
/// restarting GitWyrm.
#[tauri::command]
#[specta::specta]
pub async fn openspec_recheck_cli() -> Result<cli::CliInfo, AppError> {
  tauri::async_runtime::spawn_blocking(|| {
    cli::forget_cached();
    cli::detect()
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn openspec_status(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<OpenspecStatus, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let Some(dir) = openspec::openspec_dir(&root) else {
      return OpenspecStatus {
        present: false,
        active_count: 0,
        archived_count: 0,
        // Don't probe for the CLI when the repo has no specs: nothing would use
        // the answer, and probing can shell out to npx.
        cli: cli::CliInfo { available: false, version: None, invocation: None },
      };
    };
    OpenspecStatus {
      present: true,
      active_count: parse::parse_changes_dir(&dir).len() as u32,
      archived_count: parse::archived_ids(&dir).len() as u32,
      cli: cli::detect(),
    }
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// Every active change, newest first. Empty for a repo without `openspec/`.
#[tauri::command]
#[specta::specta]
pub async fn openspec_list_changes(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<parse::SpecChange>, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    openspec::openspec_dir(&root)
      .map(|dir| parse::parse_changes_dir(&dir))
      .unwrap_or_default()
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// One change by id, or None when it is not there (deleted or archived while
/// the UI was looking at it).
#[tauri::command]
#[specta::specta]
pub async fn openspec_get_change(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
) -> Result<Option<parse::SpecChange>, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)?;
    parse::parse_change_dir(&dir.join("changes").join(&change_id))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// Ids of archived changes, newest first.
#[tauri::command]
#[specta::specta]
pub async fn openspec_archived_ids(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<String>, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    openspec::openspec_dir(&root)
      .map(|dir| parse::archived_ids(&dir))
      .unwrap_or_default()
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// Commits that touched a change's folder, newest first.
///
/// Empty (not an error) for a change that has never been committed -- a brand-new
/// change has no history yet, which the UI states rather than treating as a fault.
#[tauri::command]
#[specta::specta]
pub async fn openspec_change_history(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
) -> Result<Vec<history::SpecHistoryEntry>, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    // 50 is far more than a change accumulates in practice, and it bounds the
    // work for a repository with a long history.
    history::change_history(&root, &change_id, 50).unwrap_or_default()
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))
}

/// Ticks or unticks one task, writing exactly that checkbox to tasks.md.
///
/// `line` comes from the parsed task the user clicked. If the file has moved on
/// since (an agent inserted tasks, someone reordered them), the write is skipped
/// and `LineMoved` tells the UI to re-read rather than guess.
#[tauri::command]
#[specta::specta]
pub async fn openspec_toggle_task(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
  line: u32,
  done: bool,
) -> Result<write::ToggleOutcome, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)
      .ok_or_else(|| AppError::Other("this repository has no openspec folder".to_string()))?;
    let path = write::tasks_path(&dir, &change_id);
    write::toggle_task_line(&path, line, done)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Creates a new change folder with template files.
#[tauri::command]
#[specta::specta]
pub async fn openspec_scaffold_change(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  description: String,
) -> Result<write::ScaffoldResult, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)
      .ok_or_else(|| AppError::Other("this repository has no openspec folder".to_string()))?;
    write::scaffold_change(&dir, &name, &description)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Work out why an archive failed. **Reads only.**
///
/// The CLI's output is a transcript ending in one line that matters, and two of
/// its failures are ordinary and recoverable. This turns the transcript into a
/// named problem with an explanation, and says whether GitWyrm can fix it. It
/// never fixes anything: that is `openspec_repair_archive`, which the user has
/// to ask for.
#[tauri::command]
#[specta::specta]
pub async fn openspec_diagnose_archive(
  change_id: String,
  output: String,
) -> Result<archive_repair::ArchiveDiagnosis, AppError> {
  Ok(archive_repair::diagnose(&change_id, &output))
}

/// Carry out a repair the user accepted, then archive again.
///
/// Both halves in one command so the user's single click produces the whole
/// outcome. Repairing and then leaving them to press Archive a second time
/// would be the app doing the hard part and then stopping short of the point.
#[tauri::command]
#[specta::specta]
pub async fn openspec_repair_archive(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
  problem: archive_repair::ArchiveProblem,
) -> Result<cli::CliOutcome, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    archive_repair::repair(&root, &problem)?;
    Ok::<_, AppError>(cli::archive_change(&root, &change_id))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Draft a change with the AI. **Writes nothing.**
///
/// The returned draft lives in the frontend until the user reviews it and
/// presses Create, which calls `openspec_create_drafted_change`. Cancelling,
/// discarding, or closing the window in between therefore cannot leave anything
/// on disk -- there is no cleanup path because there is nothing to clean up.
#[tauri::command]
#[specta::specta]
pub async fn openspec_draft_change(
  app: tauri::AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  description: String,
  provider: String,
  model: String,
) -> Result<draft::DraftedChange, AppError> {
  if description.trim().is_empty() {
    return Err(AppError::Other("Describe the change first.".into()));
  }
  let root = repo_root(&manager, &repo_id)?;
  let root_for_context = root.clone();

  // Gather everything the AI reads before spending a token on it.
  let (existing_ids, capabilities, recent_commits) =
    tauri::async_runtime::spawn_blocking(move || {
      let Some(dir) = openspec::openspec_dir(&root_for_context) else {
        return (Vec::new(), Vec::new(), String::new());
      };
      let ids = parse::parse_changes_dir(&dir)
        .into_iter()
        .map(|c| c.id)
        .collect::<Vec<_>>();
      let capabilities = capability_names(&dir);
      let commits = crate::git::shell::run_git(
        Some(&root_for_context.to_string_lossy()),
        &["log", "--oneline", "--no-decorate", "-10"],
      )
      .map(|out| out.stdout)
      .unwrap_or_default();
      (ids, capabilities, commits)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

  let desired = write::sanitize_change_id(&name)
    .ok_or_else(|| AppError::Other("that name has no letters or numbers in it".into()))?;
  let (id, renamed) = draft::unique_change_id(&desired, &existing_ids);

  let user = draft::draft_user_prompt(&description, &capabilities, &recent_commits);
  let response =
    crate::ai::complete::complete(&app, &provider, &model, draft::SYSTEM_PROMPT, &user).await?;
  draft::parse_draft(&response, &id, renamed)
}

/// Write a reviewed draft. Only the artifacts passed in are created, so an
/// artifact the user skipped is absent rather than suppressed.
#[tauri::command]
#[specta::specta]
pub async fn openspec_create_drafted_change(
  manager: State<'_, RepoManager>,
  repo_id: String,
  id: String,
  artifacts: Vec<draft::DraftedArtifact>,
) -> Result<write::ScaffoldResult, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)
      .ok_or_else(|| AppError::Other("this repository has no openspec folder".to_string()))?;
    let files = write::create_drafted_change(&dir, &id, &artifacts)?;
    Ok(write::ScaffoldResult { id, files })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Draft one spec delta to fix a change that failed its spec check. **Writes
/// nothing** -- the caller reviews it and then calls
/// `openspec_add_drafted_delta`.
///
/// `change_id` is passed in rather than read from any current selection: the
/// user may well select another change while this drafts, and the fix has to
/// land on the change they actually checked.
#[tauri::command]
#[specta::specta]
pub async fn openspec_draft_fix(
  app: tauri::AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
  validator_output: String,
  provider: String,
  model: String,
) -> Result<draft::DraftedArtifact, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  let change_for_read = change_id.clone();
  let proposal = tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)
      .ok_or_else(|| AppError::Other("this repository has no openspec folder".to_string()))?;
    let path = dir.join("changes").join(&change_for_read).join("proposal.md");
    std::fs::read_to_string(&path).map_err(|_| {
      AppError::Other(format!("{change_for_read} has no proposal to work from."))
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))??;

  let user = draft::fix_user_prompt(&change_id, &proposal, &validator_output);
  let response =
    crate::ai::complete::complete(&app, &provider, &model, draft::FIX_SYSTEM_PROMPT, &user).await?;
  draft::parse_fix(&response)
}

/// Write one reviewed delta into an existing change.
#[tauri::command]
#[specta::specta]
pub async fn openspec_add_drafted_delta(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
  delta: draft::DraftedArtifact,
) -> Result<String, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)
      .ok_or_else(|| AppError::Other("this repository has no openspec folder".to_string()))?;
    write::add_delta(&dir, &change_id, &delta)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Answer a question about a change. **Reads only.**
///
/// This deliberately goes through the one-shot completion path rather than the
/// agent: there is no tool loop, so there is no edit capability to refuse. The
/// read-only promise is structural, not a matter of the model behaving.
#[tauri::command]
#[specta::specta]
pub async fn openspec_ask(
  app: tauri::AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
  question: String,
  provider: String,
  model: String,
) -> Result<ask::AskAnswer, AppError> {
  ask::check_question(&question)?;
  let root = repo_root(&manager, &repo_id)?;
  let change_for_read = change_id.clone();
  // Decided before the question moves into the gathering closure, and from the
  // question alone: a proposal that says "make the change" must not turn every
  // question about it into an escalation.
  let asked_for_work = ask::wants_run(&question);
  let question_for_prompt = question.clone();

  // Gather the change package. Every document handed to the model is also
  // returned as a citable source, so a chip can never name something the model
  // was not actually given.
  let (prompt, sources) = tauri::async_runtime::spawn_blocking(move || {
    let dir = openspec::openspec_dir(&root)
      .ok_or_else(|| AppError::Other("this repository has no openspec folder".to_string()))?;
    let change_dir = dir.join("changes").join(&change_for_read);
    if !change_dir.exists() {
      return Err(AppError::Other(format!("{change_for_read} is not a change here.")));
    }

    let read = |name: &str| std::fs::read_to_string(change_dir.join(name)).unwrap_or_default();
    let proposal = read("proposal.md");
    let tasks = read("tasks.md");
    let design_body = read("design.md");
    let design = (!design_body.trim().is_empty()).then_some(design_body);

    let mut sources = vec![
      ask::AskSource { label: "proposal.md".into(), tab: "proposal".into() },
      ask::AskSource { label: "tasks.md".into(), tab: "tasks".into() },
    ];
    if design.is_some() {
      sources.push(ask::AskSource { label: "design.md".into(), tab: "design".into() });
    }

    let mut deltas = Vec::new();
    for delta in parse::parse_change_dir(&change_dir)
      .map(|c| c.deltas)
      .unwrap_or_default()
    {
      let body = std::fs::read_to_string(change_dir.join(&delta.file)).unwrap_or_default();
      if body.trim().is_empty() {
        continue;
      }
      sources.push(ask::AskSource { label: delta.file.clone(), tab: "deltas".into() });
      deltas.push((delta.file, body));
    }

    let prompt = ask::user_prompt(
      &change_for_read,
      &proposal,
      &tasks,
      design.as_deref(),
      &deltas,
      &question_for_prompt,
    );
    Ok::<_, AppError>((prompt, sources))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))??;

  let reply =
    crate::ai::complete::complete(&app, &provider, &model, ask::SYSTEM_PROMPT, &prompt).await?;
  Ok(ask::parse_answer(&reply, &sources, asked_for_work))
}

/// Capability folder names under `openspec/specs/`, for the drafting prompt.
fn capability_names(openspec_dir: &std::path::Path) -> Vec<String> {
  let Ok(entries) = std::fs::read_dir(openspec_dir.join("specs")) else {
    return Vec::new();
  };
  let mut names: Vec<String> = entries
    .flatten()
    .filter(|e| e.path().is_dir())
    .filter_map(|e| e.file_name().to_str().map(str::to_string))
    .filter(|name| !name.starts_with('.'))
    .collect();
  names.sort();
  names
}

/// Runs `openspec validate` for one change. Never errors for a missing CLI --
/// that comes back as a `cliMissing` outcome with a plain-language hint.
#[tauri::command]
#[specta::specta]
pub async fn openspec_validate_change(
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
) -> Result<cli::CliOutcome, AppError> {
  let root = repo_root(&manager, &repo_id)?;
  tauri::async_runtime::spawn_blocking(move || cli::validate_change(&root, &change_id))
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// The result of asking to archive: either the CLI ran, or GitWyrm stopped
/// first because the change is not ready.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ArchiveAttempt {
  /// GitWyrm did not run anything. Nothing was merged, moved, or committed.
  Blocked {
    block: archive_repair::ArchiveBlock,
    summary: String,
    detail: String,
  },
  /// The CLI ran; its outcome is inside.
  Ran { outcome: cli::CliOutcome },
}

/// Runs `openspec archive` for one change: merges its deltas into the specs
/// library and moves the folder into `changes/archive/`. On success, stages
/// and commits the result immediately -- no AI, no separate commit step --
/// so an archived change never sits as an uncommitted diff.
///
/// Checks the change is finished first, and refuses if not unless `force` says
/// the user was told and chose to continue. That check lives here rather than
/// only in the UI because GitWyrm passes `--yes` to the CLI -- which is what
/// keeps it from prompting in a GUI, and also overrides the CLI's own
/// incomplete-task guard. Without this, a change with half its tasks open
/// merges into the specs library and gets committed, reporting success.
#[tauri::command]
#[specta::specta]
pub async fn openspec_archive_change(
  app: tauri::AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  change_id: String,
  force: bool,
) -> Result<ArchiveAttempt, AppError> {
  let open = manager.get(&repo_id)?;
  let root = open.path.clone();
  let template = crate::settings::get_settings(app)?.openspec_archive_commit_template;

  if !force {
    let root_for_check = root.clone();
    let id_for_check = change_id.clone();
    let block = tauri::async_runtime::spawn_blocking(move || {
      let dir = openspec::openspec_dir(&root_for_check)?;
      let change = parse::parse_change_dir(&dir.join("changes").join(&id_for_check))?;
      archive_repair::check_ready(
        change.progress.done,
        change.progress.total,
        change.deltas.len(),
      )
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

    if let Some(block) = block {
      return Ok(ArchiveAttempt::Blocked {
        summary: block.summary(),
        detail: block.detail(),
        block,
      });
    }
  }

  tauri::async_runtime::spawn_blocking(move || {
    let outcome = cli::archive_change(&root, &change_id);
    if matches!(outcome, cli::CliOutcome::Ok { .. }) {
      let repo = open.repo.lock().unwrap();
      cli::commit_archive(&repo, &root, &change_id, template.as_deref())?;
    }
    Ok(ArchiveAttempt::Ran { outcome })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
