//! Finding the opencode CLI and starting it on a task.
//!
//! The Spec Desk's "Open in opencode" hands one task to opencode with the
//! handoff already in the conversation. opencode's TUI takes exactly that:
//! `opencode <project> --prompt <text>` starts in `<project>` with `<text>` as
//! the first message.
//!
//! The launch has to go through a terminal. opencode is a full-screen TUI, so
//! spawning it headless from a GUI process gives it no console to draw on and
//! it dies immediately -- the terminal is the window the user actually sees.

use std::path::PathBuf;
use std::process::Command;

use crate::error::AppError;

#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

/// Launcher names to try on PATH, best first. The install script and the
/// package managers all land on `opencode`; npm additionally leaves an
/// `opencode.cmd` shim, which `where` finds under the same bare name.
const COMMANDS: [&str; 1] = ["opencode"];

/// Whether a command resolves on PATH, using the platform's own lookup so
/// PATHEXT (and therefore the npm `.cmd` shim) is honoured on Windows.
fn command_exists(program: &str) -> bool {
  #[cfg(windows)]
  let mut probe = {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new("where");
    c.arg(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
  };
  #[cfg(not(windows))]
  let mut probe = {
    let mut c = Command::new("which");
    c.arg(program);
    c
  };

  probe
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

/// The user's home directory, from the platform's own variable.
fn home_dir() -> Option<PathBuf> {
  #[cfg(windows)]
  let var = "USERPROFILE";
  #[cfg(not(windows))]
  let var = "HOME";
  std::env::var_os(var).map(PathBuf::from)
}

/// Install locations worth checking when opencode is not on PATH.
///
/// A GUI app launched from the Start menu does not inherit the PATH edits a
/// shell profile makes, so a perfectly working `opencode` in a terminal can be
/// invisible here. These are the paths the documented installers use.
fn fallback_paths() -> Vec<PathBuf> {
  let mut out = Vec::new();
  let home = home_dir();

  #[cfg(windows)]
  {
    // Install script, then npm's global shim, then scoop.
    if let Some(h) = &home {
      out.push(h.join(".opencode").join("bin").join("opencode.exe"));
      out.push(h.join("scoop").join("shims").join("opencode.exe"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
      out.push(PathBuf::from(&appdata).join("npm").join("opencode.cmd"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
      out.push(
        PathBuf::from(&local)
          .join("Programs")
          .join("opencode")
          .join("opencode.exe"),
      );
    }
  }

  #[cfg(not(windows))]
  {
    if let Some(h) = &home {
      out.push(h.join(".opencode").join("bin").join("opencode"));
      out.push(h.join(".local").join("bin").join("opencode"));
    }
    out.push(PathBuf::from("/usr/local/bin/opencode"));
    out.push(PathBuf::from("/opt/homebrew/bin/opencode"));
  }

  out
}

/// How to invoke opencode on this machine: either a bare name to resolve
/// through PATH, or an absolute path found in a known install location.
pub fn find_opencode() -> Option<String> {
  if let Some(found) = COMMANDS.iter().copied().find(|c| command_exists(c)) {
    return Some(found.to_string());
  }
  fallback_paths()
    .into_iter()
    .find(|p| p.exists())
    .map(|p| p.to_string_lossy().into_owned())
}

/// Whether opencode can be launched. Drives the Desk button's enabled state, so
/// the button is never offered when clicking it could only fail.
#[tauri::command]
#[specta::specta]
pub fn opencode_available() -> bool {
  find_opencode().is_some()
}

/// Start opencode in `path` with `prompt` as the opening message.
///
/// The prompt is passed as its own argv element, never interpolated into a
/// command string. Handoffs are multi-line and contain backticks, quotes and
/// `#` -- pasted into a shell they would be mangled at best and executed at
/// worst. Everything below builds an argument vector for that reason.
pub fn launch(opencode: &str, path: &str, prompt: &str) -> Result<(), AppError> {
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;

    // Windows Terminal takes the child command line after its own options, and
    // passes the remaining argv through without a shell in between.
    let wt = Command::new("wt")
      .args(["-d", path, opencode, path, "--prompt", prompt])
      .creation_flags(CREATE_NO_WINDOW)
      .spawn();
    if wt.is_ok() {
      return Ok(());
    }

    // No Windows Terminal: fall back to a classic console. `start` treats a
    // first quoted argument as the window title, so the explicit "" keeps the
    // title slot from swallowing the command.
    Command::new("cmd")
      .args(["/C", "start", "", "cmd", "/K", opencode, path, "--prompt", prompt])
      .current_dir(path)
      .creation_flags(CREATE_NO_WINDOW)
      .spawn()
      .map_err(AppError::Io)?;
    return Ok(());
  }

  #[cfg(target_os = "macos")]
  {
    // Terminal.app can only be handed a script, not an argv, so this is the one
    // place a command string is unavoidable. Single-quote everything and escape
    // any embedded single quote, which makes the prompt inert to the shell.
    let script = format!(
      "cd {} && {} {} --prompt {}",
      sh_quote(path),
      sh_quote(opencode),
      sh_quote(path),
      sh_quote(prompt)
    );
    Command::new("osascript")
      .args([
        "-e",
        &format!(
          "tell application \"Terminal\" to do script {}",
          applescript_quote(&script)
        ),
        "-e",
        "tell application \"Terminal\" to activate",
      ])
      .spawn()
      .map_err(AppError::Io)?;
    return Ok(());
  }

  #[cfg(all(unix, not(target_os = "macos")))]
  {
    // `-e` takes the command as argv on every terminal below, so the prompt
    // stays a single argument and never reaches a shell.
    let candidates = [
      "x-terminal-emulator",
      "gnome-terminal",
      "konsole",
      "xterm",
    ];
    for term in candidates {
      let spawned = Command::new(term)
        .arg("-e")
        .args([opencode, path, "--prompt", prompt])
        .current_dir(path)
        .spawn();
      if spawned.is_ok() {
        return Ok(());
      }
    }
    return Err(AppError::Other(
      "No terminal emulator found to run opencode in.".into(),
    ));
  }

  #[allow(unreachable_code)]
  Ok(())
}

/// Wrap `s` in single quotes for a POSIX shell, escaping any single quote.
#[cfg(target_os = "macos")]
fn sh_quote(s: &str) -> String {
  format!("'{}'", s.replace('\'', r"'\''"))
}

/// Quote a string for AppleScript, escaping backslashes and double quotes.
#[cfg(target_os = "macos")]
fn applescript_quote(s: &str) -> String {
  format!("\"{}\"", s.replace('\\', r"\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Detection must never claim opencode is available without something to
  /// run. The inverse is not asserted: this machine may legitimately have no
  /// opencode installed, which is exactly the case the Desk button guards.
  #[test]
  fn availability_agrees_with_what_was_found() {
    assert_eq!(opencode_available(), find_opencode().is_some());
    if let Some(found) = find_opencode() {
      assert!(!found.is_empty(), "a found launcher must be nameable");
    }
  }

  /// Fallbacks are only consulted when PATH misses, but they still have to be
  /// absolute -- a relative path would resolve against the app's working
  /// directory rather than the user's install.
  #[test]
  fn fallback_paths_are_absolute() {
    for p in fallback_paths() {
      assert!(p.is_absolute(), "fallback path should be absolute: {p:?}");
    }
  }

  /// The handoff carries quotes, backticks and newlines. Quoting has to make
  /// all of them inert, or a prompt could end the quoted string and have its
  /// tail run as shell commands.
  #[cfg(target_os = "macos")]
  #[test]
  fn shell_quoting_neutralizes_a_hostile_prompt() {
    let hostile = "one'; rm -rf /; echo '";
    let quoted = sh_quote(hostile);
    assert!(quoted.starts_with('\'') && quoted.ends_with('\''));
    // No bare single quote survives to close the string early.
    assert!(!quoted[1..quoted.len() - 1].contains("'") || quoted.contains(r"'\''"));
  }
}
