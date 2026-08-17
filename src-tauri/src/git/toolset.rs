//! Where the downloaded git and gpg live, and which version is on disk.
//!
//! GitWyrm used to ship these inside the installer, under the app's
//! `resources/`. That meant every app update deleted and re-extracted the whole
//! tree - 408 files, 103 MB - for tools that only move when Git for Windows
//! ships, roughly monthly. They are fetched from the CDN instead, into a
//! directory that survives an update untouched.
//!
//! **Why this location matters.** The NSIS uninstaller wipes `$INSTDIR`, and an
//! app update runs a full uninstall before reinstalling. It only removes app
//! data when its checkbox is ticked *and* it is not updating, and that page is
//! never shown during an update - so anything here outlives an app update,
//! which is the entire mechanism that makes the re-extract go away.
//!
//! Local app data rather than roaming: a 103 MB unpacked toolset has no
//! business syncing to a domain profile.

use std::path::PathBuf;

/// Matches `identifier` in tauri.conf.json.
const IDENTIFIER: &str = "dev.gitwyrm.app";

/// Records which upstream release the unpacked tree came from, e.g.
/// `v2.55.0.windows.3`. Compared against the CDN manifest to decide whether a
/// download is needed at all.
pub const VERSION_FILE: &str = "TOOLSET-VERSION.txt";

/// Root of the unpacked toolset: `<local app data>/<identifier>/tools`.
///
/// Deliberately not `AppHandle::path().app_data_dir()`, which resolves to
/// Roaming on Windows. Resolved from the environment rather than a handle so
/// the same answer is available before Tauri starts.
pub fn toolset_dir() -> Option<PathBuf> {
  let base = if cfg!(target_os = "windows") {
    // Fall back to APPDATA only if LOCALAPPDATA is somehow unset; a roaming
    // toolset is worse than none, but a missing directory is worse still.
    std::env::var_os("LOCALAPPDATA")
      .or_else(|| std::env::var_os("APPDATA"))
      .map(PathBuf::from)
  } else if cfg!(target_os = "macos") {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
  } else {
    std::env::var_os("XDG_DATA_HOME")
      .map(PathBuf::from)
      .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
  };
  Some(base?.join(IDENTIFIER).join("tools"))
}

/// The toolset version on disk, or None when nothing is installed.
///
/// A present directory with no readable version file counts as "not installed":
/// it means an interrupted unpack, and re-fetching is the safe response.
pub fn installed_version() -> Option<String> {
  let dir = toolset_dir()?;
  let raw = std::fs::read_to_string(dir.join(VERSION_FILE)).ok()?;
  let version = raw.trim().to_owned();
  (!version.is_empty()).then_some(version)
}

/// Whether a usable toolset is unpacked.
///
/// Checks for git itself rather than just the directory, so a half-written tree
/// is not mistaken for a working one.
///
/// Windows-only: the published toolset is Git for Windows, and Linux uses the
/// system git and gpg instead, so nothing off Windows asks this question.
#[cfg(windows)]
pub fn is_installed() -> bool {
  toolset_dir()
    .map(|dir| dir.join("git/cmd/git.exe").is_file())
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_toolset_lives_under_the_app_identifier() {
    let Some(dir) = toolset_dir() else {
      return; // No home directory in this environment; nothing to assert.
    };
    let text = dir.to_string_lossy();
    assert!(
      text.contains(IDENTIFIER),
      "{text} should sit under {IDENTIFIER}"
    );
    assert!(text.ends_with("tools"), "{text} should end in tools");
  }

  #[cfg(target_os = "windows")]
  #[test]
  fn windows_keeps_the_toolset_out_of_the_roaming_profile() {
    // 103 MB of unpacked binaries must never sync to a domain profile.
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
      return;
    };
    let Some(dir) = toolset_dir() else {
      return;
    };
    assert!(
      dir.starts_with(PathBuf::from(local)),
      "{} should be under LOCALAPPDATA",
      dir.display()
    );
  }

  #[test]
  fn the_toolset_is_not_inside_the_install_directory() {
    // The NSIS uninstaller wipes $INSTDIR on every update. Living there is
    // exactly the bug this module exists to avoid.
    let Some(dir) = toolset_dir() else {
      return;
    };
    let text = dir.to_string_lossy().to_lowercase();
    assert!(
      !text.contains("\\gitwyrm\\resources"),
      "{text} must not be the old install-dir resources path"
    );
  }
}
