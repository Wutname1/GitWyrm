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

/// Cached CLI probe.
///
/// Probing is NOT cheap: with no global install it falls through to `npx`, which
/// can take seconds and hits the network. `openspec_status` is called on every
/// watcher event and once per window, so probing per call made a single file save
/// fire a burst of `npx` runs and left both windows stuck on loading state.
/// Whether the tool is installed does not change while the app runs, so probe
/// once per process.
static CLI_CACHE: std::sync::OnceLock<CliInfo> = std::sync::OnceLock::new();

/// The cached answer, probing on first use.
pub fn detect() -> CliInfo {
  CLI_CACHE.get_or_init(probe).clone()
}

/// Probes for a usable CLI, preferring a direct install.
fn probe() -> CliInfo {
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

/// Stages everything the archive command touched and commits it, with no AI
/// and no CLI involved in the message -- archiving is a mechanical operation,
/// so the record of it should be too.
///
/// Only called after `archive_change` reports `Ok`; a `Failed` or `CliMissing`
/// outcome leaves the working tree for the user to inspect, so nothing here
/// runs in that case.
pub fn commit_archive(
  repo: &git2::Repository,
  repo_path: &Path,
  change_id: &str,
) -> Result<Option<git2::Oid>, crate::error::AppError> {
  let repo_path_str = repo_path.to_string_lossy();

  let mut index = repo.index()?;
  index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
  index.write()?;
  let tree_oid = index.write_tree()?;
  let tree = repo.find_tree(tree_oid)?;

  let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
  if let Some(parent) = &head_commit {
    if parent.tree_id() == tree_oid {
      // Nothing changed -- the archive command itself failed to touch the
      // working tree, which `archive_change`'s Ok/Failed split does not
      // otherwise catch. Leave HEAD alone rather than writing an empty commit.
      return Ok(None);
    }
  }

  let signature = repo.signature().map_err(|_| {
    crate::error::AppError::Other(
      "Git does not know your name and email yet. Add them in Settings > General, then archive again."
        .into(),
    )
  })?;
  let identity = crate::git::commit_write::CommitIdentity {
    author: signature.clone(),
    committer: signature,
  };
  let parents: Vec<&git2::Commit<'_>> = head_commit.iter().collect();
  let message = format!("chore: Archive {change_id} spec");

  let oid = crate::git::commit_write::create(
    repo,
    &repo_path_str,
    Some("HEAD"),
    &identity,
    &message,
    &tree,
    &parents,
  )?;
  Ok(Some(oid))
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

  fn repo_with_base(dir: &Path) -> git2::Repository {
    let repo = git2::Repository::init(dir).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test Wyrm").unwrap();
    config.set_str("user.email", "test@gitwyrm.dev").unwrap();
    std::fs::write(dir.join("base.txt"), "base\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("base.txt")).unwrap();
    index.write().unwrap();
    {
      let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
      let sig = repo.signature().unwrap();
      repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[]).unwrap();
    }
    repo
  }

  #[test]
  fn commit_archive_stages_and_commits_working_tree_changes() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo_with_base(dir.path());

    // Simulate what `openspec archive` does to the working tree: move a
    // change folder and touch a spec file, with no staging of our own.
    std::fs::write(dir.path().join("base.txt"), "changed\n").unwrap();
    std::fs::write(dir.path().join("new-spec.md"), "spec\n").unwrap();

    let oid = commit_archive(&repo, dir.path(), "add-thing").unwrap();
    assert!(oid.is_some());

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id(), oid.unwrap());
    assert_eq!(head.message().unwrap(), "chore: Archive add-thing spec");
    assert_eq!(repo.statuses(None).unwrap().len(), 0, "everything should be committed");

    let tree = head.tree().unwrap();
    assert!(tree.get_path(Path::new("new-spec.md")).is_ok());
  }

  #[test]
  fn commit_archive_is_a_noop_when_nothing_changed() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo_with_base(dir.path());
    let before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let oid = commit_archive(&repo, dir.path(), "add-thing").unwrap();

    assert!(oid.is_none());
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), before);
  }
}
