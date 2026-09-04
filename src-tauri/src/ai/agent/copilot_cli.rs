//! Finding an agent's command-line tool, and deciding whether it can be
//! driven.
//!
//! Discovery only. Running a task through it is the ACP transport's job; this
//! module answers "is there a usable tool on this machine", which is what the
//! settings surface and the pre-run check need.
//!
//! Nothing here names a tool any more. Which binaries to look for, what to ask
//! for a version, and how a tool can be told to refuse something all live in
//! [`super::registry`]; this module walks PATH and the known install locations
//! with whatever names that table hands it. The module keeps its old name and
//! its `detect()` entry point because Copilot is still the default and both
//! the settings command and the end-to-end test call them.
//!
//! Verified against GitHub's own CLI command reference (2026-07-30): `copilot
//! version` reports the installed version, `copilot login` uses the OAuth
//! device flow and stores its token in the system credential store, and
//! `copilot --acp --stdio` starts the Agent Client Protocol server we drive.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::registry::{self, AgentSpec};

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
///
/// Applied to Copilot only. The other tools in the registry have no measured
/// floor, so refusing a version of one of them would be refusing on a guess --
/// see [`floor_for`].
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
    /// Found on disk, but the version probe timed out, exited non-zero, or
    /// could not be parsed.
    ///
    /// This used to be reported as `NotFound`, which told the person to
    /// install a tool they already had -- the log line right beside the return
    /// even said "found at {path} but the version probe did not answer". The
    /// same "a failed check reported as an absence" inversion the frontend has
    /// a guard for, surviving here where that guard does not reach.
    FoundButUnresponsive { path: String },
}

impl CliState {
    /// Whether this tool can actually be used right now.
    fn is_usable(&self) -> bool {
        matches!(self, CliState::Ready { .. })
    }
}

/// Result of probing one tool.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CopilotCli {
    pub state: CliState,
}

/// Probing spawns a process, so a FOUND tool is remembered for the rest of the
/// run. A "not found" answer is never remembered.
///
/// The asymmetry is the whole point, and it is a fix rather than a style
/// choice. The cache used to be a bare `OnceLock` holding whatever the first
/// probe said, including "not installed" -- so a user who read "GitHub Copilot
/// is not installed", installed it, and came back was still told it was
/// missing until they restarted the app. With one tool that was merely
/// annoying. With four it is much worse: the natural thing to do after seeing
/// "Gemini is not installed" is to install Gemini, and being told the same
/// thing afterwards reads as the install having failed.
///
/// Re-probing a negative costs one process spawn that immediately fails to
/// find a file, which is cheap. Re-probing a positive would cost a real
/// subprocess launch on every status read -- the mistake made once on the
/// OpenSpec CLI, where probing per call fired `npx` repeatedly and froze both
/// windows. So: remember success, always re-check failure.
static CACHE: OnceLock<Mutex<HashMap<&'static str, CopilotCli>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<&'static str, CopilotCli>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Guards against a hung or non-responding binary blocking the caller.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Drops every remembered probe so the next detection starts from nothing.
///
/// Only found answers are ever cached, so this exists for the other
/// direction: a tool that was present and has since been uninstalled or moved
/// would otherwise keep being reported as ready for the rest of the session.
pub fn forget_all_cached() {
    if let Ok(mut guard) = cache().lock() {
        guard.clear();
    }
}

/// Finds the default agent's tool. Kept as-is so existing callers and the
/// end-to-end test read the same as before.
pub fn detect() -> CopilotCli {
    detect_agent(registry::default_agent())
}

/// Finds one specific agent's tool.
///
/// Returns an owned value rather than a `&'static` reference: a negative
/// result is deliberately not stored (see [`CACHE`]), so there is nothing
/// static to borrow from.
pub fn detect_agent(spec: &'static AgentSpec) -> CopilotCli {
    if let Ok(found) = cache().lock() {
        if let Some(hit) = found.get(spec.id) {
            return hit.clone();
        }
    }

    let result = probe(spec);

    // Only a usable answer is worth keeping. A miss is re-checked next time,
    // so installing the tool mid-session works without a restart.
    if result.state.is_usable() {
        if let Ok(mut found) = cache().lock() {
            found.insert(spec.id, result.clone());
        }
    }

    result
}

/// The version floor for one agent, if there is a measured one.
///
/// Only Copilot has been tested against a known-good version, so it is the
/// only row with a floor. Applying its number to the others would be refusing
/// an install on a guess, which is worse than accepting one that turns out to
/// be too old -- a too-old tool fails at the handshake with the tool's own
/// message, while a wrong floor refuses a working install with ours.
fn floor_for(spec: &AgentSpec) -> Option<(u32, u32, u32)> {
    if spec.id == "copilot" {
        Some(MIN_VERSION)
    } else {
        None
    }
}

fn probe(spec: &AgentSpec) -> CopilotCli {
    let Some(path) = find_executable(spec) else {
        log::info!("{}: not found on PATH or in known locations", spec.display_name);
        return CopilotCli {
            state: CliState::NotFound,
        };
    };

    let Some(raw) = run_version(spec, &path) else {
        log::info!(
            "{}: found at {} but the version probe did not answer",
            spec.display_name,
            path.display()
        );
        return CopilotCli {
            state: CliState::FoundButUnresponsive {
                path: path.to_string_lossy().into_owned(),
            },
        };
    };

    let floor = floor_for(spec);
    let state = match (parse_version(&raw), floor) {
        (Some(v), Some(min)) if v < min => {
            log::info!("{}: {} is below the {:?} floor", spec.display_name, raw.trim(), min);
            CliState::TooOld {
                version: raw.trim().to_string(),
                minimum: format!("{}.{}.{}", min.0, min.1, min.2),
            }
        }
        (Some(_), _) => {
            log::info!("{}: {} at {}", spec.display_name, raw.trim(), path.display());
            CliState::Ready {
                version: raw.trim().to_string(),
                path: path.display().to_string(),
            }
        }
        // An unreadable version string is treated as present-and-usable rather
        // than refused: the format is the tool's to change, and refusing on a
        // parse failure would break users whose install is actually fine.
        (None, _) => {
            log::info!(
                "{}: version string {:?} not understood, treating as usable",
                spec.display_name,
                raw.trim()
            );
            CliState::Ready {
                version: raw.trim().to_string(),
                path: path.display().to_string(),
            }
        }
    };

    CopilotCli { state }
}

/// PATH first, then the places the installers put it.
fn find_executable(spec: &AgentSpec) -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in spec.candidate_names() {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    for dir in registry::known_locations() {
        for name in spec.candidate_names() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn run_version(spec: &AgentSpec, path: &PathBuf) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.args(spec.version_args);
    crate::process_env::scrub_bundled_env(&mut cmd);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    // `output()` has no timeout of its own; a hung binary would block the caller
    // forever, so the probe runs on a thread we can give up on.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(cmd.output());
    });

    let name = spec.display_name;
    match rx.recv_timeout(PROBE_TIMEOUT) {
        Ok(Ok(out)) if out.status.success() => Some(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(Ok(_)) => None,
        Ok(Err(e)) => {
            log::info!("{name}: version probe failed: {e}");
            None
        }
        Err(_) => {
            log::info!("{name}: version probe timed out after {PROBE_TIMEOUT:?}");
            None
        }
    }
}

/// Pulls the first `major.minor.patch` out of whatever the tool printed.
///
/// Deliberately loose: the output carries a product name and an update notice
/// around the number, and both are the tool's to reword. The four tools in the
/// registry each print a different shape -- `GitHub Copilot CLI 1.0.80.`,
/// `2.1.220 (Claude Code)`, a bare `1.18.10` -- and this reads all of them.
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
        let names = registry::default_agent().candidate_names();
        if cfg!(windows) {
            assert!(names.contains(&"copilot.cmd"), "the npm shim must be searched for");
            assert!(names.contains(&"copilot.exe"));
            assert!(
                names.iter().position(|n| *n == "copilot.cmd") < names.iter().position(|n| *n == "copilot"),
                "the .cmd shim must be preferred over the bare shell script"
            );
        } else {
            assert_eq!(names, &["copilot"]);
        }
    }

    #[test]
    fn parses_the_real_version_strings_every_supported_tool_prints() {
        // Verbatim from real installs. Each tool has its own shape, and the
        // parser has to read all of them.
        assert_eq!(parse_version("GitHub Copilot CLI 1.0.80."), Some((1, 0, 80)));
        assert_eq!(parse_version("2.1.220 (Claude Code)"), Some((2, 1, 220)));
        assert_eq!(parse_version("1.18.10"), Some((1, 18, 10)));
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

    /// Only Copilot has a measured floor. Refusing another tool's version
    /// would be refusing on a guess -- a too-old tool fails at the handshake
    /// with its own message, which is better than ours refusing a working
    /// install.
    #[test]
    fn only_copilot_has_a_version_floor() {
        assert_eq!(floor_for(registry::find("copilot").unwrap()), Some(MIN_VERSION));
        for id in ["gemini", "claude", "opencode"] {
            assert_eq!(floor_for(registry::find(id).unwrap()), None, "{id} must have no floor");
        }
    }

    /// The caching fix, proven at the level the bug actually bit: a "not
    /// found" answer must never be remembered, or a user who installs the
    /// tool and comes back is still told it is missing until they restart.
    #[test]
    fn a_not_found_answer_is_never_cached() {
        // A row that cannot exist on disk, so the probe reliably misses.
        static MISSING: AgentSpec = AgentSpec {
            id: "gitwyrm-test-agent-that-is-not-installed",
            display_name: "A tool nobody has",
            protocol: registry::Protocol::Acp,
            windows_names: &["gitwyrm-no-such-tool.exe", "gitwyrm-no-such-tool.cmd"],
            unix_names: &["gitwyrm-no-such-tool"],
            acp_args: &["--acp"],
            version_args: &["--version"],
            denial: registry::Denial::None,
            tool_names: registry::DeniableTools {
                shell: &[],
                network: &[],
                write: &[],
            },
            homepage_url: "https://example.invalid",
            install_hint: "",

        };

        let first = detect_agent(&MISSING);
        assert!(matches!(first.state, CliState::NotFound));

        // The cache must hold nothing for it, so the next call probes again
        // rather than replaying the miss.
        let cached = cache().lock().expect("the cache lock").get(MISSING.id).cloned();
        assert!(
            cached.is_none(),
            "a missing tool must not be remembered as missing, got {cached:?}"
        );

        let second = detect_agent(&MISSING);
        assert!(matches!(second.state, CliState::NotFound));
    }
}
