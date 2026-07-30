//! Finding the Copilot CLI, and deciding whether it can be driven.
//!
//! Discovery only. Running a task through it is the ACP transport's job; this
//! module answers "is there a usable `copilot` on this machine", which is what
//! the settings surface and the pre-run check need.
//!
//! Verified against GitHub's own CLI command reference (2026-07-30): `copilot
//! version` reports the installed version, `copilot login` uses the OAuth
//! device flow and stores its token in the system credential store, and
//! `copilot --acp --stdio` starts the Agent Client Protocol server we drive.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

/// Lowest version whose ACP server we have checked against.
///
/// A floor rather than a pinned path because these tools self-update, often
/// silently. Pinning would break the day the user's CLI updated itself; a floor
/// only refuses versions that predate the interface we rely on.
///
/// 1.0.0 because the ACP handshake was verified working against 1.0.76 (the
/// first real install available), and nothing older has been tested. Set no
/// higher than the version actually confirmed: a floor above what we know
/// would refuse installs that may well work.
pub const MIN_VERSION: (u32, u32, u32) = (1, 0, 0);

/// What discovery found.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CliState {
  /// Found, new enough, and ready to be asked for a session.
  Ready { version: String, path: String },
  /// Found but older than the floor. Says so, and that updating fixes it --
  /// `copilot update` is the CLI's own command for this.
  TooOld { version: String, minimum: String },
  /// Nothing on PATH or in a known install location.
  NotFound,
}

/// Result of probing the CLI once.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CopilotCli {
  pub state: CliState,
}

/// Probing spawns a process, so the answer is cached for the process lifetime.
///
/// Learned the hard way on the OpenSpec CLI: probing per call fired `npx`
/// repeatedly and froze both windows. A user who installs the CLI mid-session
/// restarts, which is a far better trade than a probe on every status read.
static CACHE: OnceLock<CopilotCli> = OnceLock::new();

/// Guards against a hung or non-responding binary blocking the caller.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

pub fn detect() -> &'static CopilotCli {
  CACHE.get_or_init(probe)
}

fn probe() -> CopilotCli {
  let Some(path) = find_executable() else {
    log::info!("Copilot CLI: not found on PATH or in known locations");
    return CopilotCli { state: CliState::NotFound };
  };

  let Some(raw) = run_version(&path) else {
    log::info!("Copilot CLI: found at {} but `version` did not answer", path.display());
    return CopilotCli { state: CliState::NotFound };
  };

  let state = match parse_version(&raw) {
    Some(v) if v >= MIN_VERSION => {
      log::info!("Copilot CLI: {} at {}", raw.trim(), path.display());
      CliState::Ready { version: raw.trim().to_string(), path: path.display().to_string() }
    }
    Some(_) => {
      log::info!("Copilot CLI: {} is below the {:?} floor", raw.trim(), MIN_VERSION);
      CliState::TooOld {
        version: raw.trim().to_string(),
        minimum: format!("{}.{}.{}", MIN_VERSION.0, MIN_VERSION.1, MIN_VERSION.2),
      }
    }
    // An unreadable version string is treated as present-and-usable rather
    // than refused: the format is the CLI's to change, and refusing on a
    // parse failure would break users whose install is actually fine.
    None => {
      log::info!("Copilot CLI: version string {:?} not understood, treating as usable", raw.trim());
      CliState::Ready { version: raw.trim().to_string(), path: path.display().to_string() }
    }
  };

  CopilotCli { state }
}

/// Names the CLI can have, in the order worth trying.
///
/// `npm install -g @github/copilot` -- the install method GitHub documents
/// first -- writes `copilot.cmd` and `copilot.ps1` on Windows and no `.exe` at
/// all. Looking only for `copilot.exe` reported "not installed" on a machine
/// where the CLI was on PATH and working, which would have been every Windows
/// user who followed the npm instructions.
///
/// `.cmd` before the bare name because a bare `copilot` on Windows is the
/// shell script for Git Bash, which `Command::new` cannot execute directly.
fn candidate_names() -> &'static [&'static str] {
  if cfg!(windows) {
    &["copilot.exe", "copilot.cmd", "copilot.bat", "copilot"]
  } else {
    &["copilot"]
  }
}

/// PATH first, then the places the installers put it.
fn find_executable() -> Option<PathBuf> {
  if let Some(paths) = std::env::var_os("PATH") {
    for dir in std::env::split_paths(&paths) {
      for name in candidate_names() {
        let candidate = dir.join(name);
        if candidate.is_file() {
          return Some(candidate);
        }
      }
    }
  }

  for dir in known_locations() {
    for name in candidate_names() {
      let candidate = dir.join(name);
      if candidate.is_file() {
        return Some(candidate);
      }
    }
  }

  None
}

/// `USERPROFILE` then `HOME`, matching how the rest of the codebase resolves it
/// (see `git::ssh` and `git::identity`).
fn home_dir() -> Option<PathBuf> {
  std::env::var_os("USERPROFILE")
    .or_else(|| std::env::var_os("HOME"))
    .map(PathBuf::from)
}

/// Where the npm and native installers place it when PATH has not been
/// refreshed -- common right after an install, before a new shell.
fn known_locations() -> Vec<PathBuf> {
  let mut out = Vec::new();
  if let Some(home) = home_dir() {
    out.push(home.join(".copilot").join("bin"));
    out.push(home.join(".local").join("bin"));
    if cfg!(windows) {
      out.push(home.join("AppData").join("Local").join("Programs").join("copilot"));
      out.push(home.join("AppData").join("Roaming").join("npm"));
    }
  }
  if !cfg!(windows) {
    out.push(PathBuf::from("/usr/local/bin"));
    out.push(PathBuf::from("/opt/homebrew/bin"));
  }
  out
}

fn run_version(path: &PathBuf) -> Option<String> {
  let mut cmd = Command::new(path);
  cmd.arg("version");
  #[cfg(windows)]
  cmd.creation_flags(CREATE_NO_WINDOW);

  // `output()` has no timeout of its own; a hung binary would block the caller
  // forever, so the probe runs on a thread we can give up on.
  let (tx, rx) = std::sync::mpsc::channel();
  std::thread::spawn(move || {
    let _ = tx.send(cmd.output());
  });

  match rx.recv_timeout(PROBE_TIMEOUT) {
    Ok(Ok(out)) if out.status.success() => {
      Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }
    Ok(Ok(_)) => None,
    Ok(Err(e)) => {
      log::info!("Copilot CLI: version probe failed: {e}");
      None
    }
    Err(_) => {
      log::info!("Copilot CLI: version probe timed out after {PROBE_TIMEOUT:?}");
      None
    }
  }
}

/// Pulls the first `major.minor.patch` out of whatever the CLI printed.
///
/// Deliberately loose: the output carries a product name and an update notice
/// around the number, and both are the CLI's to reword.
fn parse_version(raw: &str) -> Option<(u32, u32, u32)> {
  let bytes = raw.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i].is_ascii_digit() {
      let start = i;
      while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
      }
      let mut parts = raw[start..i].split('.');
      let major = parts.next()?.parse().ok();
      let minor = parts.next().and_then(|p| p.parse().ok());
      let patch = parts.next().and_then(|p| p.parse().ok());
      if let (Some(a), Some(b), Some(c)) = (major, minor, patch) {
        return Some((a, b, c));
      }
      continue;
    }
    i += 1;
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_a_plain_version() {
    assert_eq!(parse_version("0.0.352"), Some((0, 0, 352)));
  }

  #[test]
  fn parses_version_surrounded_by_product_text() {
    let raw = "GitHub Copilot CLI 0.1.12\nA new version is available.";
    assert_eq!(parse_version(raw), Some((0, 1, 12)));
  }

  #[test]
  fn ignores_a_leading_bare_number() {
    // A stray number with no dots must not be mistaken for the version.
    assert_eq!(parse_version("build 7 -- copilot 1.2.3"), Some((1, 2, 3)));
  }

  #[test]
  fn returns_none_when_there_is_no_version() {
    assert_eq!(parse_version("not installed"), None);
  }

  #[test]
  fn windows_looks_for_the_npm_shim_not_just_an_exe() {
    // `npm install -g @github/copilot` writes copilot.cmd and no .exe. Looking
    // only for copilot.exe reported "not installed" on a machine where the CLI
    // was on PATH and working -- which would have been every Windows user who
    // followed GitHub's own first-listed install instructions.
    let names = candidate_names();
    if cfg!(windows) {
      assert!(names.contains(&"copilot.cmd"), "the npm shim must be searched for");
      assert!(names.contains(&"copilot.exe"));
      assert!(
        names.iter().position(|n| *n == "copilot.cmd")
          < names.iter().position(|n| *n == "copilot"),
        "the .cmd shim must be preferred over the bare shell script"
      );
    } else {
      assert_eq!(names, &["copilot"]);
    }
  }

  #[test]
  fn parses_a_real_cli_version_string() {
    // Verbatim from an installed Claude Code CLI (`claude --version`), kept as
    // a fixture because it is the shape these tools actually print: a version
    // followed by a parenthesised product name.
    assert_eq!(parse_version("2.1.220 (Claude Code)"), Some((2, 1, 220)));
  }

  #[test]
  fn floor_comparison_is_by_component_not_string() {
    // "1.0.9" > "1.0.10" as strings; as tuples it is not.
    assert!(parse_version("1.0.9").unwrap() >= MIN_VERSION);
    assert!(parse_version("1.0.76").unwrap() >= MIN_VERSION);
    assert!(parse_version("0.9.999").unwrap() < MIN_VERSION);
  }

  #[test]
  fn the_installed_copilot_version_clears_the_floor() {
    // Verbatim from `copilot version` on the real 1.0.76 install this floor
    // was measured against. A regression here means either the parser or the
    // floor moved somewhere that would refuse a working install.
    let raw = "GitHub Copilot CLI 1.0.76\n\nYou are running the latest version.";
    let parsed = parse_version(raw).expect("the real version string must parse");
    assert_eq!(parsed, (1, 0, 76));
    assert!(parsed >= MIN_VERSION);
  }
}
