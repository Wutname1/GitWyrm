//! Runs the system git.exe for network operations (fetch/pull/push/clone).
//! Git Credential Manager handles auth; we never touch credentials.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::RwLock;

use crate::error::AppError;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The git program invoked for every shell-out. Defaults to `git` (found on
/// PATH); the user can point it at a specific git.exe in Settings. Held in a
/// process-global so call sites don't have to thread the path through.
static GIT_PROGRAM: RwLock<Option<String>> = RwLock::new(None);

/// Set the git program used for all shell-outs. An empty or whitespace-only
/// value clears the override, falling back to `git` on PATH.
pub fn set_git_program(path: Option<&str>) {
  let cleaned = path
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_owned);
  if let Ok(mut guard) = GIT_PROGRAM.write() {
    *guard = cleaned;
  }
}

/// The git program to invoke: the configured path, or `git` when unset.
/// Public so other modules that spawn git directly (e.g. AI staging) share it.
pub fn git_program_name() -> String {
  GIT_PROGRAM
    .read()
    .ok()
    .and_then(|g| g.clone())
    .unwrap_or_else(|| "git".to_owned())
}

pub struct GitOutput {
  pub stdout: String,
  pub stderr: String,
}

pub fn run_git(repo_path: Option<&str>, args: &[&str]) -> Result<GitOutput, AppError> {
  let mut cmd = Command::new(git_program_name());
  if let Some(path) = repo_path {
    cmd.arg("-C").arg(path);
  }
  cmd.args(args);

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW);
  }

  let out = cmd.output().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other("git executable not found on PATH".into())
    } else {
      AppError::Io(e)
    }
  })?;

  let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
  let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

  if !out.status.success() {
    let msg = if stderr.trim().is_empty() {
      stdout.clone()
    } else {
      stderr.clone()
    };
    return Err(AppError::Other(format!(
      "git {} failed: {}",
      args.first().unwrap_or(&""),
      msg.trim()
    )));
  }

  Ok(GitOutput { stdout, stderr })
}

pub fn git_available() -> bool {
  run_git(None, &["--version"]).is_ok()
}

/// Run `<candidate> --version` to confirm a chosen git path works. Returns the
/// version banner (e.g. "git version 2.45.1") on success. Used by Settings to
/// give immediate feedback when the user picks or types a git executable.
/// An empty/blank candidate checks `git` on PATH.
#[tauri::command]
#[specta::specta]
pub fn verify_git_executable(path: String) -> Result<String, AppError> {
  let program = {
    let trimmed = path.trim();
    if trimmed.is_empty() {
      "git".to_owned()
    } else {
      trimmed.to_owned()
    }
  };

  let mut cmd = Command::new(&program);
  cmd.arg("--version");

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW);
  }

  let out = cmd.output().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other(format!("No git found at {program}"))
    } else {
      AppError::Io(e)
    }
  })?;

  if !out.status.success() {
    let stderr = String::from_utf8_lossy(&out.stderr);
    return Err(AppError::Other(format!(
      "{program} is not a working git: {}",
      stderr.trim()
    )));
  }

  Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Runs git with `stdin` piped in as raw bytes. Used to feed a patch to
/// `git apply`. Returns the raw stdout/stderr; errors carry git's stderr.
pub fn run_git_stdin(
  repo_path: Option<&str>,
  args: &[&str],
  stdin_bytes: &[u8],
) -> Result<GitOutput, AppError> {
  let mut cmd = Command::new(git_program_name());
  if let Some(path) = repo_path {
    cmd.arg("-C").arg(path);
  }
  cmd
    .args(args)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

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

  child
    .stdin
    .take()
    .ok_or_else(|| AppError::Other("failed to open git stdin".into()))?
    .write_all(stdin_bytes)
    .map_err(AppError::Io)?;

  let out = child.wait_with_output().map_err(AppError::Io)?;
  let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
  let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

  if !out.status.success() {
    let msg = if stderr.trim().is_empty() {
      stdout.clone()
    } else {
      stderr.clone()
    };
    return Err(AppError::Other(format!(
      "git {} failed: {}",
      args.first().unwrap_or(&""),
      msg.trim()
    )));
  }

  Ok(GitOutput { stdout, stderr })
}
