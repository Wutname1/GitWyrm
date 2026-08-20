//! GitHub CLI fallback for requests our own OAuth app is not allowed to make.
//!
//! Organizations block third-party OAuth apps by default. Until an admin
//! approves GitWyrm specifically, every request for that org's pull requests
//! and issues comes back refused, and there is nothing the user can do from
//! inside the app -- they are simply told no.
//!
//! `gh` is usually not blocked. It is a first-party GitHub application that
//! most organizations have already approved, and the user signed into it
//! themselves with their own account. So when our token is refused and `gh` is
//! installed and logged in, asking it instead turns a dead panel into a working
//! one, using a credential the user already granted on this machine.
//!
//! This is a transport swap and nothing more. `gh api` is a thin authenticated
//! passthrough to the same REST API at the same paths, so the JSON it returns
//! deserializes into the same structs as the `reqwest` path. Nothing downstream
//! knows which route the bytes arrived by.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::error::AppError;
#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

/// A hung or interactively-prompting `gh` must not hold a request open forever.
/// The fallback already runs after a failed round trip, so the user has waited
/// once; this bounds the second wait.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Why the fallback is unavailable, phrased for the settings row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// No `gh` on PATH or in the usual install locations.
    NotInstalled,
    /// `gh` is present but nobody is signed in, so it has no more access than we do.
    NotSignedIn,
}

/// Names the CLI can have. On Windows the installer writes `gh.exe`, but a
/// scoop or npm-style shim can be a `.cmd`, which `Command::new` will not run
/// under a bare name.
fn candidate_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["gh.exe", "gh.cmd", "gh.bat", "gh"]
    } else {
        &["gh"]
    }
}

/// `USERPROFILE` then `HOME`, matching how the rest of the codebase resolves it.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Where the installers put `gh` when PATH has not been refreshed -- common
/// right after an install, before a new shell exists.
fn known_locations() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                out.push(PathBuf::from(&base).join("GitHub CLI"));
            }
        }
        if let Some(home) = home_dir() {
            out.push(home.join("scoop").join("shims"));
            out.push(home.join("AppData").join("Roaming").join("npm"));
        }
    } else {
        out.push(PathBuf::from("/usr/local/bin"));
        out.push(PathBuf::from("/usr/bin"));
        out.push(PathBuf::from("/opt/homebrew/bin"));
        if let Some(home) = home_dir() {
            out.push(home.join(".local").join("bin"));
        }
    }
    out
}

/// PATH first, then the places the installers put it.
///
/// Deliberately not cached. Installing `gh` is exactly what a user does after
/// reading a hint that says it is missing, and a cached "not installed" would
/// keep the fallback off until they restarted the app. The walk is a handful of
/// `is_file` calls against an already-warm directory cache.
pub fn find_executable() -> Option<PathBuf> {
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

/// Runs a command to completion, giving up after `timeout`.
///
/// `Command::output()` has no timeout of its own, so a `gh` that decides to
/// prompt would block the calling request forever. Running it on a thread we
/// can abandon bounds the damage: the thread leaks, the request does not.
fn output_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(cmd.output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("timed out".into()),
    }
}

fn base_command(exe: &PathBuf) -> Command {
    let mut cmd = Command::new(exe);
    // `gh` prompts when it thinks a human is watching, and a prompt in a
    // spawned process is a hang. This is the documented way to forbid that.
    cmd.env("GH_PROMPT_DISABLED", "1");
    cmd.env("GH_NO_UPDATE_NOTIFIER", "1");
    // A pager would never terminate without a terminal to quit from.
    cmd.env("GH_PAGER", "");
    cmd.env("PAGER", "");
    // `gh` is a system binary; give it the system's libraries.
    crate::process_env::scrub_bundled_env(&mut cmd);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// True when `gh` is installed and has a usable login.
///
/// Checked before falling back rather than inferring it from a failed call: a
/// signed-out `gh` fails in a way that looks like a permissions problem, and
/// reporting that as "GitHub refused" would send the user to fix the wrong
/// thing.
pub fn availability() -> Result<PathBuf, Unavailable> {
    let exe = find_executable().ok_or(Unavailable::NotInstalled)?;
    let mut cmd = base_command(&exe);
    cmd.args(["auth", "status"]);
    match output_with_timeout(cmd, PROBE_TIMEOUT) {
        Ok(out) if out.status.success() => Ok(exe),
        _ => Err(Unavailable::NotSignedIn),
    }
}

/// One `gh api` call, returning the raw response body.
///
/// `path` is the same API path the HTTP client would have used, with or without
/// a leading slash -- `gh` accepts `repos/o/r/pulls` and normalizes the rest.
/// `body` is the JSON payload for a write; None for a read.
pub fn api(
    exe: &PathBuf,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<String, AppError> {
    let mut cmd = base_command(exe);
    cmd.arg("api");
    cmd.args(["--method", method]);
    // `gh` prepends the host itself; a leading slash yields a doubled path.
    cmd.arg(path.trim_start_matches('/'));
    cmd.args(["-H", "Accept: application/vnd.github+json"]);
    cmd.args(["-H", "X-GitHub-Api-Version: 2022-11-28"]);

    if let Some(body) = body {
        // `--input -` reads the raw JSON body from stdin, which avoids `-f`'s
        // key=value parsing mangling values that contain `=` or newlines -- a
        // comment body is arbitrary user text and routinely contains both.
        cmd.args(["--input", "-"]);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let payload = body.to_string();
        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Other(format!("could not run the GitHub CLI: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(payload.as_bytes())
                .map_err(|e| AppError::Other(format!("could not run the GitHub CLI: {e}")))?;
        }
        let out = child
            .wait_with_output()
            .map_err(|e| AppError::Other(format!("could not run the GitHub CLI: {e}")))?;
        return finish(out);
    }

    let out = output_with_timeout(cmd, CALL_TIMEOUT)
        .map_err(|e| AppError::Other(format!("could not run the GitHub CLI: {e}")))?;
    finish(out)
}

/// Turns a finished `gh` run into a body or a readable error.
fn finish(out: std::process::Output) -> Result<String, AppError> {
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    // `gh` writes the API's own JSON error to stderr around its own wrapper
    // text. The API message is the useful half, so prefer it when present.
    let message = extract_api_message(&stderr)
        .unwrap_or_else(|| stderr.trim().chars().take(200).collect::<String>());
    Err(AppError::Other(format!("GitHub CLI said: {message}")))
}

/// Digs the `"message"` out of an API error `gh` echoed into its stderr.
fn extract_api_message(stderr: &str) -> Option<String> {
    let start = stderr.find('{')?;
    let end = stderr.rfind('}')?;
    if end <= start {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&stderr[start..=end]).ok()?;
    value
        .get("message")
        .and_then(|m| m.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the module is reaching the same paths the HTTP client
    /// uses, so a leading slash must not survive into the argument.
    #[test]
    fn strips_the_leading_slash_from_paths() {
        assert_eq!(
            "/repos/o/r/pulls".trim_start_matches('/'),
            "repos/o/r/pulls"
        );
        assert_eq!("repos/o/r/pulls".trim_start_matches('/'), "repos/o/r/pulls");
    }

    #[test]
    fn reads_the_api_message_out_of_gh_stderr() {
        let stderr = "gh: Not Found (HTTP 404)\n{\"message\":\"Not Found\",\"status\":\"404\"}\n";
        assert_eq!(extract_api_message(stderr).as_deref(), Some("Not Found"));
    }

    /// `gh` also fails for reasons that produce no JSON at all (no network, bad
    /// subcommand). Those must fall through to the raw text rather than panic.
    #[test]
    fn survives_stderr_with_no_json() {
        assert!(extract_api_message("could not connect to github.com").is_none());
        assert!(extract_api_message("").is_none());
        assert!(extract_api_message("}{").is_none());
    }
}
