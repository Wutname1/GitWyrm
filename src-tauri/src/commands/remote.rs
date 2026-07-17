//! Network operations via system git.exe (Git Credential Manager handles auth).
//! Progress lines from stderr stream to the frontend as `git-progress` events.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::git::types::{RebaseResult, RemoteInfo};
use crate::state::RepoManager;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Type)]
pub struct GitProgressPayload {
  pub repo_id: String,
  pub operation: String,
  pub line: String,
}

fn run_streaming(
  app: &AppHandle,
  repo_id: &str,
  repo_path: Option<&str>,
  operation: &str,
  args: &[&str],
) -> Result<String, AppError> {
  let mut cmd = Command::new("git");
  if let Some(path) = repo_path {
    cmd.arg("-C").arg(path);
  }
  cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW);
  }

  let mut child = cmd.spawn().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other("git executable not found on PATH".into())
    } else {
      AppError::Io(e)
    }
  })?;

  // git writes progress to stderr; stream each line to the frontend.
  let stderr = child.stderr.take();
  let mut stderr_lines: Vec<String> = Vec::new();
  if let Some(stderr) = stderr {
    let reader = BufReader::new(stderr);
    for line in reader.split(b'\r') {
      // Progress uses \r updates; split on both \r and \n chunks.
      let Ok(chunk) = line else { break };
      for part in String::from_utf8_lossy(&chunk).split('\n') {
        let part = part.trim();
        if part.is_empty() {
          continue;
        }
        stderr_lines.push(part.to_string());
        let _ = app.emit(
          "git-progress",
          GitProgressPayload {
            repo_id: repo_id.to_string(),
            operation: operation.to_string(),
            line: part.to_string(),
          },
        );
      }
    }
  }

  let output = child.wait_with_output().map_err(AppError::Io)?;
  let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

  if !output.status.success() {
    let detail = stderr_lines.last().cloned().unwrap_or_else(|| stdout.trim().to_string());
    return Err(AppError::Other(format!("git {operation} failed: {detail}")));
  }
  Ok(stdout)
}

#[tauri::command]
#[specta::specta]
pub async fn git_fetch(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    run_streaming(&app, &repo_id, Some(&path), "fetch", &["fetch", "--all", "--prune", "--progress"])?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn git_pull(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    run_streaming(&app, &repo_id, Some(&path), "pull", &["pull", "--progress"])?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn git_push(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    run_streaming(&app, &repo_id, Some(&path), "push", &["push", "--progress"])?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// List configured remotes with their URLs and remote-tracking branches.
/// Local config read only; no network.
#[tauri::command]
#[specta::specta]
pub async fn list_remotes(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<RemoteInfo>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut remotes = Vec::new();

    for name in repo.remotes()?.iter().flatten() {
      let remote = repo.find_remote(name)?;
      let url = remote.url().unwrap_or("").to_string();
      let push_url = remote.pushurl().map(str::to_string).filter(|p| *p != url);

      // Remote-tracking branches live under refs/remotes/<name>/*. Strip the
      // `<name>/` prefix and skip the symbolic HEAD ref.
      let prefix = format!("{name}/");
      let mut branches: Vec<String> = repo
        .branches(Some(git2::BranchType::Remote))?
        .flatten()
        .filter_map(|(b, _)| b.name().ok().flatten().map(str::to_string))
        .filter(|full| full.starts_with(&prefix) && !full.ends_with("/HEAD"))
        .map(|full| full[prefix.len()..].to_string())
        .collect();
      branches.sort();

      remotes.push(RemoteInfo { name: name.to_string(), url, push_url, branches });
    }

    remotes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(remotes)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Add a new remote. Fails if the name is already in use.
#[tauri::command]
#[specta::specta]
pub async fn add_remote(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  url: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    repo.remote(name.trim(), url.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Rename a remote. Also rewrites its remote-tracking refs and any branch
/// upstreams that referenced the old name.
#[tauri::command]
#[specta::specta]
pub async fn rename_remote(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  new_name: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    // Returns any non-default refspecs that couldn't be auto-updated; a standard
    // remote has none, so we don't surface them.
    repo.remote_rename(name.trim(), new_name.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Change a remote's fetch URL.
#[tauri::command]
#[specta::specta]
pub async fn set_remote_url(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  url: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    repo.remote_set_url(name.trim(), url.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete a remote and its remote-tracking branches.
#[tauri::command]
#[specta::specta]
pub async fn remove_remote(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    repo.remote_delete(name.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Set a remote-tracking branch as the upstream ("set target") of the current
/// local branch. `remote_branch` is the full remote-tracking name, e.g.
/// `origin/main`.
#[tauri::command]
#[specta::specta]
pub async fn set_upstream(
  manager: State<'_, RepoManager>,
  repo_id: String,
  remote_branch: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let head = repo.head()?;
    if !head.is_branch() {
      return Err(AppError::Other("HEAD is detached; check out a branch first".into()));
    }
    let shorthand = head
      .shorthand()
      .ok_or_else(|| AppError::Other("could not read current branch name".into()))?
      .to_string();

    let mut local = repo.find_branch(&shorthand, git2::BranchType::Local)?;
    // Confirm the remote-tracking branch exists before wiring it up.
    repo.find_branch(remote_branch.trim(), git2::BranchType::Remote)?;
    local.set_upstream(Some(remote_branch.trim()))?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Push the current branch, overwriting the remote with `--force-with-lease`.
/// Lease-based so it refuses to clobber remote commits the user hasn't fetched;
/// used after a local rewind/rebase leaves the branch diverged from its upstream.
#[tauri::command]
#[specta::specta]
pub async fn git_push_force(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    run_streaming(
      &app,
      &repo_id,
      Some(&path),
      "push",
      &["push", "--force-with-lease", "--progress"],
    )?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// True when the working tree has tracked modifications (ignores untracked).
fn tree_dirty(repo: &git2::Repository) -> Result<bool, AppError> {
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(false);
  Ok(repo.statuses(Some(&mut opts))?.iter().any(|e| !e.status().is_ignored()))
}

/// Paths currently conflicted in the index (both sides of a paused rebase).
fn conflicted_paths(repo: &git2::Repository) -> Result<Vec<String>, AppError> {
  let index = repo.index()?;
  if !index.has_conflicts() {
    return Ok(Vec::new());
  }
  let mut paths = Vec::new();
  for entry in index.conflicts()? {
    let entry = entry?;
    let raw = entry
      .our
      .as_ref()
      .or(entry.their.as_ref())
      .or(entry.ancestor.as_ref())
      .map(|e| e.path.clone());
    if let Some(bytes) = raw {
      paths.push(String::from_utf8_lossy(&bytes).into_owned());
    }
  }
  paths.sort();
  paths.dedup();
  Ok(paths)
}

/// Rebase a branch onto `onto` (e.g. `origin/main`), replaying its commits on
/// top. Rebases the current branch when `branch` is None; otherwise git checks
/// out `branch` first and leaves HEAD there. A clean rebase returns no
/// conflicts. A rebase that hits conflicts leaves the repo paused
/// (rebase-in-progress) and returns the conflicted paths instead of erroring,
/// so the frontend can guide the user. Refuses to start over a dirty tree.
#[tauri::command]
#[specta::specta]
pub async fn git_rebase(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  onto: String,
  branch: Option<String>,
) -> Result<RebaseResult, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    {
      let repo = open.repo.lock().unwrap();
      if tree_dirty(&repo)? {
        return Err(AppError::Other(
          "working tree has changes; commit or stash before rebasing".into(),
        ));
      }
    }

    let mut args = vec!["rebase", "--progress", onto.as_str()];
    if let Some(b) = branch.as_deref() {
      args.push(b);
    }

    match run_streaming(&app, &repo_id, Some(&path), "rebase", &args) {
      Ok(_) => Ok(RebaseResult { conflicts: Vec::new() }),
      Err(e) => {
        // A conflicting rebase exits non-zero but leaves a rebase-in-progress
        // state under .git. If that's what happened, report the conflicts
        // rather than the raw error; a real failure (no rebase state) errors.
        let repo = open.repo.lock().unwrap();
        let git_dir = repo.path();
        let in_progress =
          git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists();
        if in_progress {
          let conflicts = conflicted_paths(&repo)?;
          Ok(RebaseResult { conflicts })
        } else {
          Err(e)
        }
      }
    }
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn git_clone(
  app: AppHandle,
  url: String,
  destination: String,
) -> Result<String, AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    run_streaming(&app, "clone", None, "clone", &["clone", "--progress", &url, &destination])?;
    Ok(destination)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
