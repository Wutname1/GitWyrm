use tauri::State;

use crate::commands::log::cached_change_stats;
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

/// Stash only the changes under one folder, leaving the rest of the working
/// tree alone. git2 has no pathspec-scoped stash, so this shells out to git,
/// which supports `stash push -- <pathspec>`.
#[tauri::command]
#[specta::specta]
pub async fn stash_folder(
  manager: State<'_, RepoManager>,
  repo_id: String,
  folder: String,
  message: Option<String>,
) -> Result<StashOutcome, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let msg = message.unwrap_or_else(|| format!("WIP in {folder}"));
    // `--include-untracked` matches the whole-tree stash, so a new file in the
    // folder is saved rather than silently left behind. The pathspec is passed
    // after `--` so a folder named like a flag can't be read as one.
    let pathspec = format!("{}/", folder.trim_end_matches('/'));
    let out = crate::git::shell::run_git(
      Some(&path),
      &[
        "stash",
        "push",
        "--include-untracked",
        "-m",
        &msg,
        "--",
        &pathspec,
      ],
    )?;
    // git reports a clean subtree on stdout rather than as a failure.
    if out.stdout.contains("No local changes to save") {
      Ok(StashOutcome::NothingToStash)
    } else {
      Ok(StashOutcome::Stashed)
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

/// Rename a stash by giving it a new message, keeping its saved changes exactly
/// as they are.
///
/// A stash entry's message lives in the `refs/stash` reflog, not in the stash
/// commit, so there is nothing to amend: the entry is re-stored under the new
/// message and the old entry dropped. `stash store` is used rather than
/// rebuilding the commit so the stash keeps its original sha, which is what the
/// graph anchors its row to.
///
/// Order matters for safety. Storing first means the stashed changes are
/// referenced twice for a moment; if the drop then fails the user has a
/// duplicate, which is recoverable. Dropping first would leave a window where a
/// failed store loses the changes outright.
#[tauri::command]
#[specta::specta]
pub async fn rename_stash(
  manager: State<'_, RepoManager>,
  repo_id: String,
  index: u32,
  message: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let message = message.trim().to_string();
    if message.is_empty() {
      return Err(AppError::Other("a stash name is required".into()));
    }

    // Resolve the entry to its sha up front. The index shifts as soon as
    // anything is added to or removed from the stash list, so every step after
    // this one addresses the stash by sha instead.
    let sha = {
      let mut repo = open.repo.lock().unwrap();
      let mut found = None;
      repo.stash_foreach(|i, _, oid| {
        if i as u32 == index {
          found = Some(*oid);
        }
        true
      })?;
      found.ok_or_else(|| AppError::Other("that stash no longer exists".into()))?
    };
    let sha = sha.to_string();

    // Store re-points refs/stash at the same commit under the new message,
    // pushing it to the top of the list. The old entry is now one deeper than
    // it was, so re-resolve its position by sha rather than reusing `index`.
    crate::git::shell::run_git(
      Some(&path),
      &["stash", "store", "-m", &message, &sha],
    )?;

    let mut repo = open.repo.lock().unwrap();
    let mut stale = None;
    repo.stash_foreach(|i, _, oid| {
      // Skip the entry just written at the top; the match below it is the
      // original. Both point at the same sha.
      if i > 0 && oid.to_string() == sha && stale.is_none() {
        stale = Some(i);
      }
      true
    })?;
    if let Some(i) = stale {
      repo.stash_drop(i)?;
    }
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
        cached_change_stats(&open, &repo, &commit).unwrap_or((0, 0, 0));
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
