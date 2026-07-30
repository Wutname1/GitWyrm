//! Runs the `openspec` CLI for the two operations that have real semantics we
//! should not reimplement: `validate` and `archive`.
//!
//! Viewing never comes through here. If the CLI is absent, every read path still
//! works from the bundled parser and these two return a typed `CliMissing`
//! outcome carrying a plain-language hint -- not an error dialog.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

/// Whether the CLI is usable, and which version answered.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CliInfo {
  pub available: bool,
  /// Version string as reported, when we could get one.
  pub version: Option<String>,
  /// How it is invoked, for the log and for Settings: "openspec" or "npx".
  pub invocation: Option<String>,
}

/// The result of a validate or archive attempt.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CliOutcome {
  /// The command ran and reported success.
  Ok { output: String },
  /// The command ran and reported a problem. `output` is the CLI's own text,
  /// shown as-is; the UI adds the plain-language framing around it.
  Failed { output: String },
  /// No CLI available. `hint` explains what to install, in plain words.
  CliMissing { hint: String },
}

/// How to invoke the CLI. `openspec` on PATH is preferred; `npx` is the fallback
/// so a repo with the package available still gets validate/archive without a
/// global install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Invocation {
  Direct,
  Npx,
}

impl Invocation {
  fn label(self) -> &'static str {
    match self {
      Self::Direct => "openspec",
      Self::Npx => "npx",
    }
  }

  /// Builds a command for `args`, run inside `cwd`.
  fn command(self, cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = match self {
      Self::Direct => {
        let mut c = Command::new(program_name("openspec"));
        c.args(args);
        c
      }
      Self::Npx => {
        let mut c = Command::new(program_name("npx"));
        // `--yes` so a missing package installs rather than prompting: a prompt
        // would hang forever with no console attached.
        c.args(["--yes", "@fission-ai/openspec@latest"]);
        c.args(args);
        c
      }
    };
    cmd.current_dir(cwd);
    // The CLI collects anonymous stats by default; GitWyrm invoking it is not
    // the user opting in, so turn it off for our calls.
    cmd.env("OPENSPEC_TELEMETRY", "0");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
  }
}

/// On Windows, npm-installed binaries are `.cmd` shims that `Command` will not
/// find by bare name. Keep the plain name elsewhere.
fn program_name(base: &str) -> String {
  #[cfg(windows)]
  {
    format!("{base}.cmd")
  }
  #[cfg(not(windows))]
  {
    base.to_string()
  }
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Probes for a usable CLI, preferring a direct install. Cheap enough to call
/// per `openspec_status`, and the result is what the status bar displays.
pub fn detect() -> CliInfo {
  for invocation in [Invocation::Direct, Invocation::Npx] {
    // A bare `--version` needs no repo, so run it in the current directory.
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let mut cmd = invocation.command(&cwd, &["--version"]);
    // npx would happily spend a minute downloading a package here; only trust it
    // when it answers promptly. `output()` has no timeout, so npx probing is
    // best-effort: it is the same call the real commands make, so a slow answer
    // now means a slow answer later either way.
    if let Ok(out) = cmd.output() {
      if out.status.success() {
        let version = String::from_utf8_lossy(&out.stdout)
          .lines()
          // The CLI prints a telemetry note on the first line sometimes; take
          // the first line that looks like a version.
          .map(str::trim)
          .find(|l| l.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
          .map(str::to_string);
        log::info!(
          "openspec CLI available via {} ({})",
          invocation.label(),
          version.clone().unwrap_or_else(|| "unknown version".into())
        );
        return CliInfo {
          available: true,
          version,
          invocation: Some(invocation.label().to_string()),
        };
      }
    }
  }
  CliInfo { available: false, version: None, invocation: None }
}

const MISSING_HINT: &str =
  "This check needs the OpenSpec tool, which is not installed. Install it with \
   \"npm install -g @fission-ai/openspec\" - everything else here keeps working without it.";

/// Runs a CLI subcommand in `repo_root`, mapping the result into `CliOutcome`.
fn run(repo_root: &Path, args: &[&str]) -> CliOutcome {
  let info = detect();
  let invocation = match info.invocation.as_deref() {
    Some("openspec") => Invocation::Direct,
    Some("npx") => Invocation::Npx,
    _ => return CliOutcome::CliMissing { hint: MISSING_HINT.to_string() },
  };

  match invocation.command(repo_root, args).output() {
    Ok(out) => {
      // The CLI writes findings to both streams; keep both so nothing the user
      // needs is silently dropped.
      let mut text = String::from_utf8_lossy(&out.stdout).trim().to_string();
      let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
      if !err.is_empty() {
        if !text.is_empty() {
          text.push('\n');
        }
        text.push_str(&err);
      }
      if out.status.success() {
        CliOutcome::Ok { output: text }
      } else {
        log::info!("openspec {:?} reported a problem: {text}", args);
        CliOutcome::Failed { output: text }
      }
    }
    Err(e) => {
      // Detection said it was there, so this is a real failure to launch.
      log::warn!("openspec {:?} could not run: {e}", args);
      CliOutcome::CliMissing { hint: MISSING_HINT.to_string() }
    }
  }
}

/// `openspec validate <change_id> --type change`.
pub fn validate_change(repo_root: &Path, change_id: &str) -> CliOutcome {
  run(repo_root, &["validate", change_id, "--type", "change"])
}

/// `openspec archive <change_id> --yes` -- merges deltas into the specs library
/// and moves the change into `changes/archive/`.
pub fn archive_change(repo_root: &Path, change_id: &str) -> CliOutcome {
  run(repo_root, &["archive", change_id, "--yes"])
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn missing_hint_is_plain_language() {
    // No jargon, and it says the rest of the app still works. Guards the copy
    // against a future edit that turns it into a stack trace.
    assert!(MISSING_HINT.contains("not installed"));
    assert!(MISSING_HINT.contains("keeps working"));
    assert!(!MISSING_HINT.contains("ENOENT"));
    assert!(!MISSING_HINT.contains("exit code"));
  }

  #[test]
  fn program_names_are_platform_correct() {
    let name = program_name("npx");
    #[cfg(windows)]
    assert_eq!(name, "npx.cmd");
    #[cfg(not(windows))]
    assert_eq!(name, "npx");
  }

  #[test]
  fn invocation_labels_round_trip() {
    // `run` maps the detected label back to an Invocation; keep them in sync.
    assert_eq!(Invocation::Direct.label(), "openspec");
    assert_eq!(Invocation::Npx.label(), "npx");
  }
}
