//! Runs git.exe for network operations (fetch/pull/push/clone).
//! Git Credential Manager handles auth; we never touch credentials.
//!
//! Which git runs is decided in `super::bundled`: the path set in Settings, the
//! system git on PATH, then the copy bundled with GitWyrm. The bundled tree is
//! MinGit, which carries Git Credential Manager too, so auth works the same way
//! on a machine with no git installed.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::RwLock;

use crate::error::AppError;

/// Keeps a spawned console process from flashing a window on Windows. Shared so
/// the flag is declared once rather than copied into every module that spawns.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The git path the user set in Settings, if any. Held in a process-global so
/// call sites don't have to thread it through. The actual program to run is
/// decided by `git_program_name`, which falls back to PATH and then to the
/// copy bundled with GitWyrm.
static GIT_PROGRAM: RwLock<Option<String>> = RwLock::new(None);

/// Set the git program used for all shell-outs. An empty or whitespace-only
/// value clears the override, so resolution falls back to PATH then bundled.
pub fn set_git_program(path: Option<&str>) {
    let cleaned = path
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if let Ok(mut guard) = GIT_PROGRAM.write() {
        *guard = cleaned;
    }
    // The choice changed, so the cached resolution is stale.
    super::bundled::clear_git_cache();
}

/// The git program to invoke: the configured path, the system git on PATH, or
/// the bundled copy - whichever is found first. Public so other modules that
/// spawn git directly (e.g. AI staging) share the same resolution.
pub fn git_program_name() -> String {
    let configured = GIT_PROGRAM.read().ok().and_then(|g| g.clone());
    super::bundled::resolve_git(configured.as_deref()).program
}

/// Which git the app is actually using, for display in Settings.
pub fn git_source() -> super::bundled::ToolSource {
    let configured = GIT_PROGRAM.read().ok().and_then(|g| g.clone());
    super::bundled::resolve_git(configured.as_deref()).source
}

pub struct GitOutput {
    pub stdout: String,
    /// Git's diagnostics from a SUCCESSFUL run. Callers take `.stdout` and drop
    /// the rest, so `log_stderr` records this to the app log instead of losing it:
    /// git reports real warnings here even on exit 0 ("redirecting to a new URL",
    /// ref-update notices, hook output), and those explain later surprises.
    pub stderr: String,
}

impl GitOutput {
    /// Git's diagnostics with surrounding whitespace removed, or None when it said
    /// nothing. Lets a caller surface a warning without re-trimming.
    pub fn warning(&self) -> Option<&str> {
        let trimmed = self.stderr.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }
}

/// Log git's stderr from a successful run, so a warning is diagnosable rather
/// than discarded. Failures already carry stderr in their error message.
fn log_stderr(args: &[&str], out: &GitOutput) {
    if let Some(warning) = out.warning() {
        log::debug!("git {} warned: {}", args.first().unwrap_or(&""), warning);
    }
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

    let out = GitOutput { stdout, stderr };
    log_stderr(args, &out);
    Ok(out)
}

pub fn git_available() -> bool {
    run_git(None, &["--version"]).is_ok()
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
    cmd.args(args)
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

    let out = GitOutput { stdout, stderr };
    log_stderr(args, &out);
    Ok(out)
}
