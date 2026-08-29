//! Is there a usable `snip` on this machine, and which version answered.
//!
//! Modelled on [`crate::ai::agent::copilot_cli`], with one deliberate
//! difference in the caching -- see [`CACHE`].

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

/// What discovery found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SnipState {
    /// Found, and it answered `--version`.
    ///
    /// No version floor. GitWyrm only reads what `snip` reports; it does not
    /// drive any interface that could break between releases, so refusing an
    /// older install would deny the user a working tool for no gain.
    Ready { version: String, path: String },
    /// Nothing on PATH or in a known install location.
    NotFound,
}

/// Guards against a hung or non-responding binary blocking the caller.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Cached probe result, holding only a *positive* answer.
///
/// Probing spawns a process, so repeating it on every read would be wasteful.
/// But the sibling Copilot probe caches in a bare `OnceLock`, which means a
/// "not found" answer sticks until the app restarts -- and installing the tool
/// right after reading "not installed" is precisely what a user does next.
/// Caching only the success keeps the common case cheap while letting a fresh
/// install be noticed without a restart. The cost of being wrong the other way
/// (one extra spawn per read on machines without `snip`) is small and bounded.
static CACHE: std::sync::RwLock<Option<SnipState>> = std::sync::RwLock::new(None);

/// The cached answer, probing when there is not one yet.
pub fn detect() -> SnipState {
    if let Ok(guard) = CACHE.read() {
        if let Some(state) = guard.as_ref() {
            return state.clone();
        }
    }

    // Probe outside the write lock: it spawns a process and can take seconds,
    // and holding the lock across that would queue every other caller behind it.
    let state = probe();

    // Only a found result is remembered. A negative is re-probed next time so a
    // user who installs `snip` mid-session gets a correct answer straight away.
    if matches!(state, SnipState::Ready { .. }) {
        if let Ok(mut guard) = CACHE.write() {
            *guard = Some(state.clone());
        }
    }

    state
}

/// Drops the cached answer so the next [`detect`] probes again.
///
/// Only meaningful after a *positive* result, since negatives are never cached.
/// Exists so a "check again" affordance can also recover from the rarer case of
/// the tool being uninstalled or moved while GitWyrm is open.
#[cfg_attr(not(test), expect(dead_code, reason = "for a future check-again action"))]
pub fn forget_cached() {
    if let Ok(mut guard) = CACHE.write() {
        *guard = None;
    }
}

fn probe() -> SnipState {
    let Some(path) = find_executable() else {
        log::info!("snip: not found on PATH or in known locations");
        return SnipState::NotFound;
    };

    let Some(raw) = run_version(&path) else {
        log::info!(
            "snip: found at {} but `--version` did not answer",
            path.display()
        );
        return SnipState::NotFound;
    };

    let version = clean_version(&raw);
    log::info!("snip: {} at {}", version, path.display());
    SnipState::Ready {
        version,
        path: path.display().to_string(),
    }
}

/// Names the CLI can have, in the order worth trying.
///
/// `snip` ships as a Go binary, so `snip.exe` is the usual Windows shape. The
/// shims are still searched because package managers and wrapper installs leave
/// `.cmd` or `.bat` behind, and looking only for `.exe` is exactly the mistake
/// that reported "not installed" for the Copilot CLI on machines where it was
/// on PATH and working.
///
/// The bare name comes last: on Windows it is the extensionless shell script,
/// which `Command::new` cannot execute directly.
fn candidate_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["snip.exe", "snip.cmd", "snip.bat", "snip"]
    } else {
        &["snip"]
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
/// (see `git::ssh` and `ai::agent::copilot_cli`).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Where a Go install lands when PATH has not been refreshed -- common right
/// after `go install`, before a new shell picks up the change.
fn known_locations() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = home_dir() {
        out.push(home.join("go").join("bin"));
        out.push(home.join(".local").join("bin"));
        if cfg!(windows) {
            out.push(
                home.join("AppData")
                    .join("Local")
                    .join("Programs")
                    .join("snip"),
            );
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
    cmd.arg("--version");
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
            log::info!("snip: version probe failed: {e}");
            None
        }
        Err(_) => {
            log::info!("snip: version probe timed out after {PROBE_TIMEOUT:?}");
            None
        }
    }
}

/// Turns what `--version` printed into the number worth showing.
///
/// A release build prints `snip v0.25.0`; a build from source prints
/// `snip vdev`. Both keep whatever follows the `v` verbatim, because "dev" is a
/// useful thing to show a user wondering why their `snip` behaves oddly --
/// blanking it would hide exactly the detail that explains the difference.
fn clean_version(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_name = trimmed.strip_prefix("snip").unwrap_or(trimmed).trim();
    without_name
        .strip_prefix('v')
        .unwrap_or(without_name)
        .to_string()
}

/// Splits a cleaned version into `major.minor.patch`, when it has that shape.
///
/// Returns `None` for `dev` and anything else non-numeric.
///
/// Nothing gates on this today -- there is deliberately no version floor -- so
/// it is kept for its tests: they pin down that a release version is
/// comparable, a source build is not, and that comparing these as strings
/// would order them wrongly. A future floor starts from a checked parser
/// rather than a fresh guess.
#[cfg_attr(not(test), expect(dead_code, reason = "kept tested for a future version floor"))]
pub fn parse_version(cleaned: &str) -> Option<(u32, u32, u32)> {
    let mut parts = cleaned.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    // Trailing text after the patch number (an `-rc1` suffix, say) is dropped
    // rather than failing the whole parse.
    let patch_field = parts.next()?;
    let digits: String = patch_field
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let patch = digits.parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_version_string_loses_its_name_and_v_prefix() {
        // Verbatim from `snip --version` on v0.25.0. The literal `v` is the
        // detail that breaks a naive semver parse.
        assert_eq!(clean_version("snip v0.25.0\n"), "0.25.0");
    }

    #[test]
    fn a_source_build_reports_dev_rather_than_a_number() {
        // Building from source prints `snip vdev`. Showing "dev" is the point:
        // it explains behaviour a numbered release would not have.
        assert_eq!(clean_version("snip vdev\n"), "dev");
    }

    #[test]
    fn a_version_without_the_product_name_still_cleans_up() {
        assert_eq!(clean_version("v0.25.0"), "0.25.0");
        assert_eq!(clean_version("0.25.0"), "0.25.0");
    }

    #[test]
    fn a_release_version_parses_into_comparable_parts() {
        assert_eq!(parse_version("0.25.0"), Some((0, 25, 0)));
    }

    #[test]
    fn a_dev_build_has_no_comparable_version() {
        assert_eq!(parse_version("dev"), None);
    }

    #[test]
    fn version_comparison_is_by_component_not_string() {
        // "0.9.0" > "0.25.0" as strings; as tuples it is not.
        assert!(parse_version("0.25.0").unwrap() > parse_version("0.9.0").unwrap());
    }

    #[test]
    fn windows_looks_for_the_shims_not_just_an_exe() {
        let names = candidate_names();
        if cfg!(windows) {
            assert!(names.contains(&"snip.exe"));
            assert!(names.contains(&"snip.cmd"));
            assert!(
                names.iter().position(|n| *n == "snip.cmd")
                    < names.iter().position(|n| *n == "snip"),
                "the .cmd shim must be preferred over the bare shell script"
            );
        } else {
            assert_eq!(names, &["snip"]);
        }
    }

    #[test]
    fn a_negative_detection_is_not_cached() {
        // A user who installs snip after reading "not installed" must get a
        // correct answer without restarting GitWyrm. Only a found result is
        // remembered, so the cache must still be empty after a miss.
        forget_cached();
        let state = detect();
        if state == SnipState::NotFound {
            let guard = CACHE.read().expect("cache lock");
            assert!(
                guard.is_none(),
                "a not-found answer must never be cached, or installing snip would need a restart"
            );
        }
    }
}
