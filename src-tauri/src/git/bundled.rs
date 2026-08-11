//! Locates the git and gpg executables the app shells out to.
//!
//! GitWyrm provides its own copies of both (MinGit and GnuPG) so someone with a
//! clean Windows machine can clone, commit, sign, and push without installing
//! anything first. They are downloaded from the CDN rather than unpacked by the
//! installer, and live outside the install directory - see `git::toolset` for
//! why that location is what makes app updates cheap.
//!
//! Resolution is **fallback-only**, per tool, in this order:
//!
//!   1. The path the user set in Settings.
//!   2. The system copy found on PATH.
//!   3. The copy bundled with GitWyrm.
//!
//! A system install always wins over ours. Someone whose gpg already talks to a
//! smartcard, or whose git is a custom build, keeps exactly what they had - the
//! bundled copies exist to fill a gap, never to override a working setup.
//!
//! Resolution is done once and cached: it costs a process spawn per tool, and
//! the answer cannot change while the app is running (a Settings change calls
//! the setter, which clears the cache).

use std::path::PathBuf;
use std::process::Command;
use std::sync::RwLock;

/// Root of the unpacked toolset. `None` when nothing has been downloaded yet,
/// which is normal on a dev build and before the first fetch completes.
///
/// A lock rather than a `OnceLock`: the toolset can arrive *after* startup, and
/// the resolution caches below have to be dropped when it does or a session that
/// began without git would keep reporting it missing.
static BUNDLE_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Cached resolution for each tool, cleared when the user changes a path.
static GIT_RESOLVED: RwLock<Option<Resolved>> = RwLock::new(None);
static GPG_RESOLVED: RwLock<Option<Resolved>> = RwLock::new(None);
static SSH_KEYGEN_RESOLVED: RwLock<Option<Resolved>> = RwLock::new(None);

/// Where a resolved executable came from. Surfaced in Settings so the user can
/// see whether they are on their own install or ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ToolSource {
  /// The path the user typed or browsed to in Settings.
  Configured,
  /// Found on PATH - a system install.
  System,
  /// The copy shipped inside GitWyrm.
  Bundled,
  /// Not found anywhere.
  Missing,
}

#[derive(Debug, Clone)]
pub struct Resolved {
  pub program: String,
  pub source: ToolSource,
}

impl Resolved {
  fn missing(fallback: &str) -> Self {
    Self {
      program: fallback.to_owned(),
      source: ToolSource::Missing,
    }
  }
}

/// Record the toolset root, dropping any cached resolutions.
///
/// Called at startup and again after a download completes. The cache clear is
/// the load-bearing part: without it a session that started with no git would
/// go on reporting it missing even once the tools had landed.
pub fn set_bundle_root(root: Option<PathBuf>) {
  if let Some(path) = root.as_ref() {
    log::info!("toolset root: {}", path.display());
  } else {
    log::info!("no toolset (dev build, or not downloaded yet)");
  }

  if let Ok(mut guard) = BUNDLE_ROOT.write() {
    *guard = root;
  }

  clear_git_cache();
  clear_gpg_cache();
  clear_ssh_keygen_cache();
}

/// Path to a toolset executable, if the toolset exists and the file is present.
/// A missing file is normal before the first download and must not be an error.
fn bundled_path(relative: &str) -> Option<String> {
  let guard = BUNDLE_ROOT.read().ok()?;
  let candidate = guard.as_ref()?.join(relative);
  candidate
    .is_file()
    .then(|| candidate.to_string_lossy().into_owned())
}

/// True when `program --version` runs and exits cleanly. This is the only
/// honest test of "is this a working tool": PATH lookup alone would accept a
/// stub, and `Path::is_file` says nothing about whether it executes.
fn responds_to_version(program: &str) -> bool {
  let mut cmd = Command::new(program);
  cmd.arg("--version");

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(super::shell::CREATE_NO_WINDOW);
  }

  cmd
    .output()
    .map(|out| out.status.success())
    .unwrap_or(false)
}

/// Walk the three tiers for one tool and return the first that answers.
///
/// `configured` is the user's Settings value, `on_path` the bare name to try
/// against PATH, and `bundled_rel` the tool's location inside the bundle.
fn resolve(configured: Option<&str>, on_path: &str, bundled_rel: &str) -> Resolved {
  // 1. The user's explicit choice. Taken at face value even if it does not
  //    respond: silently falling through to a different tool would hide a
  //    typo, and Settings shows a live check for exactly this reason.
  if let Some(path) = configured.map(str::trim).filter(|s| !s.is_empty()) {
    return Resolved {
      program: path.to_owned(),
      source: ToolSource::Configured,
    };
  }

  // 2. A system install. Wins over the bundled copy so an existing setup -
  //    credential helpers, smartcards, custom config - keeps working.
  if responds_to_version(on_path) {
    return Resolved {
      program: on_path.to_owned(),
      source: ToolSource::System,
    };
  }

  // 3. Our copy, the reason a clean machine works at all.
  if let Some(path) = bundled_path(bundled_rel) {
    if responds_to_version(&path) {
      return Resolved {
        program: path,
        source: ToolSource::Bundled,
      };
    }
    log::warn!("bundled {on_path} at {path} did not respond to --version");
  }

  Resolved::missing(on_path)
}

/// Relative locations inside the bundle. These mirror the layout the CI fetch
/// step unpacks; changing one means changing the other.
const GIT_BUNDLED_REL: &str = "git/cmd/git.exe";
const GPG_BUNDLED_REL: &str = "gpg/gpg.exe";
/// Beside gpg rather than under git/: MinGit ships ssh, ssh-add and ssh-agent
/// but not ssh-keygen, so this one is carved out of the portable tree with the
/// GnuPG files.
const SSH_KEYGEN_BUNDLED_REL: &str = "gpg/ssh-keygen.exe";

/// Resolve a tool, reusing the cached answer when one exists.
fn cached(
  cache: &RwLock<Option<Resolved>>,
  configured: Option<&str>,
  on_path: &str,
  bundled_rel: &str,
) -> Resolved {
  if let Ok(guard) = cache.read() {
    if let Some(hit) = guard.as_ref() {
      return hit.clone();
    }
  }

  let resolved = resolve(configured, on_path, bundled_rel);
  log::info!(
    "resolved {on_path}: {} ({:?})",
    resolved.program,
    resolved.source
  );

  if let Ok(mut guard) = cache.write() {
    *guard = Some(resolved.clone());
  }
  resolved
}

/// Drop a cached resolution so the next call re-resolves. Called when the user
/// changes a path in Settings.
pub fn clear_git_cache() {
  if let Ok(mut guard) = GIT_RESOLVED.write() {
    *guard = None;
  }
}

pub fn clear_gpg_cache() {
  if let Ok(mut guard) = GPG_RESOLVED.write() {
    *guard = None;
  }
}

/// Dropped alongside the others when the toolset root changes: ssh-keygen is
/// resolved from the same tree, so a stale "missing" here would outlast the
/// download that fixed it.
pub fn clear_ssh_keygen_cache() {
  if let Ok(mut guard) = SSH_KEYGEN_RESOLVED.write() {
    *guard = None;
  }
}

pub fn resolve_git(configured: Option<&str>) -> Resolved {
  cached(&GIT_RESOLVED, configured, "git", GIT_BUNDLED_REL)
}

pub fn resolve_gpg(configured: Option<&str>) -> Resolved {
  cached(&GPG_RESOLVED, configured, "gpg", GPG_BUNDLED_REL)
}

/// Resolve ssh-keygen, used to make and inspect SSH signing keys.
///
/// Cannot go through the shared `cached` path: that probes with `--version`,
/// which ssh-keygen does not support - it prints usage and exits 1, so a
/// perfectly good ssh-keygen would be judged broken. Existence plus the
/// help-text check below is the honest test here.
pub fn resolve_ssh_keygen() -> Resolved {
  if let Ok(guard) = SSH_KEYGEN_RESOLVED.read() {
    if let Some(hit) = guard.as_ref() {
      return hit.clone();
    }
  }

  let resolved = if responds_to_help("ssh-keygen") {
    Resolved {
      program: "ssh-keygen".to_owned(),
      source: ToolSource::System,
    }
  } else if let Some(path) = bundled_path(SSH_KEYGEN_BUNDLED_REL) {
    Resolved {
      program: path,
      source: ToolSource::Bundled,
    }
  } else {
    Resolved::missing("ssh-keygen")
  };

  log::info!(
    "resolved ssh-keygen: {} ({:?})",
    resolved.program,
    resolved.source
  );
  if let Ok(mut guard) = SSH_KEYGEN_RESOLVED.write() {
    *guard = Some(resolved.clone());
  }
  resolved
}

/// True when running the program produces ssh-keygen's usage text.
///
/// It has no `--version`; an unrecognised flag makes it print usage and exit
/// non-zero, so the output is what identifies it rather than the exit code.
fn responds_to_help(program: &str) -> bool {
  let mut cmd = Command::new(program);
  cmd.arg("-?");

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(super::shell::CREATE_NO_WINDOW);
  }

  cmd
    .output()
    .map(|out| {
      let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
      );
      text.contains("ssh-keygen")
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_configured_path_wins_over_everything() {
    let resolved = resolve(Some("C:/custom/git.exe"), "git", GIT_BUNDLED_REL);
    assert_eq!(resolved.program, "C:/custom/git.exe");
    assert_eq!(resolved.source, ToolSource::Configured);
  }

  #[test]
  fn blank_configured_values_are_ignored() {
    // An empty Settings field means "decide for me", not "use the empty path".
    for blank in ["", "   ", "\t"] {
      let resolved = resolve(Some(blank), "git", GIT_BUNDLED_REL);
      assert_ne!(resolved.source, ToolSource::Configured);
    }
  }

  #[test]
  fn a_tool_that_does_not_exist_resolves_to_missing() {
    let resolved = resolve(None, "gitwyrm-no-such-tool", "nope/nope.exe");
    assert_eq!(resolved.source, ToolSource::Missing);
  }

  #[test]
  fn missing_still_reports_the_bare_name_so_errors_read_sensibly() {
    // Callers spawn `program` regardless; the resulting "not found" error should
    // name the tool the user expects, not an empty string.
    let resolved = Resolved::missing("gpg");
    assert_eq!(resolved.program, "gpg");
  }
}
