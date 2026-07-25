//! Locates the git and gpg executables the app shells out to.
//!
//! GitWyrm ships its own copies of both (MinGit and GnuPG, unpacked into the
//! install dir by the bundler) so someone with a clean Windows machine can
//! clone, commit, sign, and push without installing anything first.
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

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{OnceLock, RwLock};

/// Root of the unpacked bundled tools, resolved from the Tauri resource dir at
/// startup. `None` in dev builds, where no resources are bundled.
static BUNDLE_ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Cached resolution for each tool, cleared when the user changes a path.
static GIT_RESOLVED: RwLock<Option<Resolved>> = RwLock::new(None);
static GPG_RESOLVED: RwLock<Option<Resolved>> = RwLock::new(None);

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

/// Record the bundled-tools root. Called once from `setup()`, where the
/// `AppHandle` that knows the resource directory is available.
pub fn set_bundle_root(root: Option<PathBuf>) {
  if let Some(path) = root.as_ref() {
    log::info!("bundled tools root: {}", path.display());
  } else {
    log::info!("no bundled tools (dev build or resources absent)");
  }
  let _ = BUNDLE_ROOT.set(root);
}

fn bundle_root() -> Option<&'static Path> {
  BUNDLE_ROOT.get().and_then(|o| o.as_deref())
}

/// Path to a bundled executable, if the bundle exists and the file is present.
/// A missing file is normal on a dev build and must not be treated as an error.
fn bundled_path(relative: &str) -> Option<String> {
  let candidate = bundle_root()?.join(relative);
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

pub fn resolve_git(configured: Option<&str>) -> Resolved {
  cached(&GIT_RESOLVED, configured, "git", GIT_BUNDLED_REL)
}

pub fn resolve_gpg(configured: Option<&str>) -> Resolved {
  cached(&GPG_RESOLVED, configured, "gpg", GPG_BUNDLED_REL)
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
