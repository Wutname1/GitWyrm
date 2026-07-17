//! Network operations via system git.exe (Git Credential Manager handles auth).
//! Progress lines from stderr stream to the frontend as `git-progress` events.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
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
