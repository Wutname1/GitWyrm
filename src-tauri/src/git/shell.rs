//! Runs the system git.exe for network operations (fetch/pull/push/clone).
//! Git Credential Manager handles auth; we never touch credentials.

use std::process::Command;

use crate::error::AppError;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct GitOutput {
  pub stdout: String,
  pub stderr: String,
}

pub fn run_git(repo_path: Option<&str>, args: &[&str]) -> Result<GitOutput, AppError> {
  let mut cmd = Command::new("git");
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
    let msg = if stderr.trim().is_empty() { stdout.clone() } else { stderr.clone() };
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
