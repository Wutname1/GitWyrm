use tauri::State;

use crate::commands::log::commit_change_stats;
use crate::error::AppError;
use crate::git::types::{StashInfo, StashOutcome};
use crate::state::RepoManager;

#[tauri::command]
#[specta::specta]
pub async fn stash_save(
  manager: State<'_, RepoManager>,
  repo_id: String,
  message: Option<String>,
) -> Result<StashOutcome, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();
    let signature = repo.signature()?;
    let msg = message.unwrap_or_else(|| "WIP".into());
    match repo.stash_save(&signature, &msg, Some(git2::StashFlags::INCLUDE_UNTRACKED)) {
      Ok(_) => Ok(StashOutcome::Stashed),
      // A clean working tree isn't a failure -- there's simply nothing to save.
      // git2 signals this as Stash/NotFound; report it as a benign no-op so the
      // UI can inform rather than alarm.
      Err(e) if e.class() == git2::ErrorClass::Stash && e.code() == git2::ErrorCode::NotFound => {
        Ok(StashOutcome::NothingToStash)
      }
      Err(e) => Err(e.into()),
    }
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn stash_pop(
  manager: State<'_, RepoManager>,
  repo_id: String,
  index: u32,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();
    repo.stash_pop(index as usize, None)?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Apply a stash to the working tree while keeping it in the stash list.
#[tauri::command]
#[specta::specta]
pub async fn stash_apply(
  manager: State<'_, RepoManager>,
  repo_id: String,
  index: u32,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();
    repo.stash_apply(index as usize, None)?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete a stash without touching the working tree.
#[tauri::command]
#[specta::specta]
pub async fn stash_drop(
  manager: State<'_, RepoManager>,
  repo_id: String,
  index: u32,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();
    repo.stash_drop(index as usize)?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Split a raw stash message into (branch, summary). Git formats stash
/// messages as "WIP on <branch>: <sha> <subject>" for the default message and
/// "On <branch>: <message>" for custom ones. Detached-HEAD stashes use
/// "(no branch)" as the branch name.
fn parse_stash_message(message: &str) -> (Option<String>, String) {
  let wip = message.strip_prefix("WIP on ");
  let rest = wip.or_else(|| message.strip_prefix("On "));
  let Some(rest) = rest else { return (None, message.to_string()) };
  let Some((branch, tail)) = rest.split_once(": ") else { return (None, message.to_string()) };
  let branch = match branch {
    "(no branch)" => None,
    b => Some(b.to_string()),
  };
  // The default "WIP on" form embeds "<short-sha> <subject>"; drop the sha so
  // the summary reads as plain words.
  let summary = if wip.is_some() {
    match tail.split_once(' ') {
      Some((sha, subject)) if sha.chars().all(|c| c.is_ascii_hexdigit()) => subject.to_string(),
      _ => tail.to_string(),
    }
  } else {
    tail.to_string()
  };
  (branch, summary)
}

#[tauri::command]
#[specta::specta]
pub async fn list_stashes(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<StashInfo>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let mut repo = open.repo.lock().unwrap();
    // stash_foreach borrows the repo mutably, so collect the raw entries first
    // and resolve commit details in a second pass.
    let mut entries = Vec::new();
    repo.stash_foreach(|index, message, oid| {
      entries.push((index as u32, message.to_string(), *oid));
      true
    })?;

    let mut stashes = Vec::with_capacity(entries.len());
    for (index, message, oid) in entries {
      let commit = repo.find_commit(oid)?;
      let base_sha = commit
        .parent_id(0)
        .map(|p| p.to_string())
        .unwrap_or_default();
      let (files_changed, additions, deletions) =
        commit_change_stats(&repo, &commit).unwrap_or((0, 0, 0));
      let (branch, summary) = parse_stash_message(&message);
      stashes.push(StashInfo {
        index,
        summary,
        branch,
        sha: oid.to_string(),
        base_sha,
        time: commit.time().seconds() as f64,
        files_changed,
        additions,
        deletions,
        message,
      });
    }
    Ok(stashes)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
