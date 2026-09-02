//! Runs git.exe for network operations (fetch/pull/push/clone).
//!
//! Authentication belongs to Git Credential Manager. A run the user is watching
//! talks to it directly, so it may open a login window and may replace a
//! credential the host rejected. An unattended run goes through
//! `git::credential_helper`, a read-only front for Credential Manager that can
//! read the stored sign-in but never erase it. A credential never enters this
//! module's memory, its arguments, or its environment either way.
//!
//! Which git runs is decided in `super::bundled`: the path set in Settings, the
//! system git on PATH, then the copy bundled with GitWyrm. The bundled tree is
//! MinGit, which carries Git Credential Manager too, so auth works the same way
//! on a machine with no git installed.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::RwLock;

use crate::error::AppError;

/// Keeps a spawned console process from flashing a window on Windows. Shared so
/// the flag is declared once rather than copied into every module that spawns.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The git path the user set in Settings, if any. Held in a process-global so
/// call sites don't have to thread it through. The actual program to run is
/// decided by `git_program_name`, which falls back to PATH and then to the
/// copy bundled with GitWyrm.
static GIT_PROGRAM: RwLock<Option<String>> = RwLock::new(None);

/// Set the git program used for all shell-outs. An empty or whitespace-only
/// value clears the override, so resolution falls back to PATH then bundled.
pub fn set_git_program(path: Option<&str>) {
    let cleaned = path
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if let Ok(mut guard) = GIT_PROGRAM.write() {
        *guard = cleaned;
    }
    // The choice changed, so the cached resolution is stale.
    super::bundled::clear_git_cache();
}

/// The git program to invoke: the configured path, the system git on PATH, or
/// the bundled copy - whichever is found first. Public so other modules that
/// spawn git directly (e.g. AI staging) share the same resolution.
pub fn git_program_name() -> String {
    let configured = GIT_PROGRAM.read().ok().and_then(|g| g.clone());
    super::bundled::resolve_git(configured.as_deref()).program
}

/// Which git the app is actually using, for display in Settings.
pub fn git_source() -> super::bundled::ToolSource {
    let configured = GIT_PROGRAM.read().ok().and_then(|g| g.clone());
    super::bundled::resolve_git(configured.as_deref()).source
}

/// Environment every git child gets, whatever the call site.
///
/// Children are spawned with `CREATE_NO_WINDOW` and no console attached, so an
/// interactive prompt has nowhere to draw. Git still tries, then fails with
/// `could not read Username ... No such file or directory` - an error naming a
/// missing *file* when the real cause is a missing *terminal*. Setting this
/// makes git say `terminal prompts disabled` and fail at once instead of
/// hanging until the operation is killed.
///
/// Note this does not stop a credential *helper* from opening its own window;
/// GCM is a separate process with its own UI. `apply_background_env` is what
/// silences that.
pub fn prepare_git_env(cmd: &mut Command) {
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    // Lets the read-only credential front reach the same git (and so the same
    // Credential Manager) this run uses, rather than whichever is first on PATH.
    cmd.env(
        crate::git::credential_helper::GIT_PROGRAM_ENV,
        git_program_name(),
    );
}

/// Whether a person is watching a git run, which decides how it may
/// authenticate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Attended {
    /// The user pressed something and is waiting: a login window is allowed,
    /// and so is replacing a credential the host rejected.
    User,
    /// A timer started this. Never prompt, and never touch the stored sign-in.
    Background,
}

/// The `credential.helper` list git should use for one run.
///
/// Starts with an empty value, which is the load-bearing part: for a
/// multi-valued config key git treats `key=` as a reset that discards
/// everything accumulated so far, so the name after it becomes the only
/// helper. The bundled MinGit tree ships an `etc/gitconfig` that sets
/// `credential.helper = manager` and then `include`s the system Git for Windows
/// config, which sets it again -- and git runs *every* entry it finds, so a
/// machine with both installs launched Credential Manager twice for one
/// authentication: two login windows, one action. Confirmed against a live
/// client with `GIT_TRACE=1`, which showed `run_command: 'git
/// credential-manager get'` twice before this and once after.
///
/// Deliberately *not* `GIT_CONFIG_NOSYSTEM`, which would also fix the duplicate:
/// the same system config carries `core.autocrlf`, `core.symlinks`,
/// `http.sslbackend=schannel` and the git-lfs filter chain. Dropping that tier
/// to remove one duplicated line would change line endings and checkout
/// behaviour, and would put us in the business of re-supplying Git for Windows'
/// defaults by hand.
///
/// Both kinds of run then name this binary's helper as the only entry -- the
/// account the user connected in Settings answers first, and Credential
/// Manager is reached *through* the helper rather than listed beside it. If
/// `manager` sat in the list too, a push that succeeded on our token would
/// make git `store` that token into Credential Manager, overwriting the user's
/// own sign-in. See `git::credential_helper` for the full policy; the flag
/// difference between the two runs is what stops a background 401 from erasing
/// the stored sign-in.
pub fn credential_args(attended: Attended) -> Vec<String> {
    let mut args = vec!["-c".into(), "credential.helper=".into()];
    args.push("-c".into());
    // When the current executable cannot be located the helper could not be
    // spawned anyway; Credential Manager alone is the behaviour before the
    // helper existed rather than a broken entry that errors on every run.
    args.push(format!(
        "credential.helper={}",
        own_credential_helper(attended).unwrap_or_else(|| "manager".to_string())
    ));
    args
}

/// The `credential.helper` value that points git back at this binary.
///
/// Git treats a value containing a space or starting with `!` as a shell
/// command, so the executable path is quoted: a default Windows install lives
/// under `C:\Program Files\`, and unquoted that parses as the program
/// `C:\Program` with an argument. Confirmed by the quoting rule in
/// git-credential(1).
fn own_credential_helper(attended: Attended) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.to_str()?;
    // `!` marks the value as a shell command, which is what allows an absolute
    // path with an argument. Without it git would look for `git-credential-<value>`.
    let mut value = format!(
        "!\"{path}\" {}",
        crate::git::credential_helper::HELPER_FLAG
    );
    if attended == Attended::Background {
        value.push(' ');
        value.push_str(crate::git::credential_helper::UNATTENDED_FLAG);
    }
    Some(value)
}

/// The environment variable that stops Git Credential Manager opening a window.
///
/// `credential.interactive=false` looks like the setting for this and is not:
/// verified against a live client with `GIT_TRACE=1`, GCM is still launched and
/// still prompts. GCM reads its own `GCM_INTERACTIVE`, and with `never` it
/// refuses at once - "Cannot prompt because user interactivity has been
/// disabled" - which is what an unattended fetch needs.
///
/// Applied by `apply_background_env` rather than passed as `-c`, because it is
/// the helper's variable, not one of git's config keys.
const GCM_NON_INTERACTIVE: (&str, &str) = ("GCM_INTERACTIVE", "never");

/// Mark a command as unattended: no credential window, whatever the helper.
///
/// For work the user did not ask for - the auto-fetch sweep wakes every open
/// repo every 15 minutes. A background operation must never pop a login window:
/// the user is typing somewhere else with no idea what asked. With this an
/// unauthenticated background fetch fails immediately and is logged, while the
/// next fetch the user actually initiates is still free to prompt.
pub fn apply_background_env(cmd: &mut Command) {
    cmd.env(GCM_NON_INTERACTIVE.0, GCM_NON_INTERACTIVE.1);
}

/// Every credential helper git would run, and which config set it.
///
/// The whole point of the Credential Manager investigation is a question we
/// cannot answer from our own machine: on the system that actually prompts, how
/// many helpers does git invoke, and where do they come from? A duplicate is
/// invisible in normal use - it just looks like an extra login window - and it
/// depends entirely on which gits are installed, so it cannot be reproduced by
/// guessing.
///
/// Runs `config --show-origin --get-all credential.helper`, which prints one
/// line per configured helper with its source file. Two lines here means two
/// windows for one authentication.
///
/// Returns entries like `manager <- C:/Program Files/Git/etc/gitconfig`. Safe to
/// log: a helper *name* and the config file that set it are not secrets, and no
/// credential is ever read by this call.
///
/// Failure is reported rather than swallowed - "we could not tell" is a
/// materially different diagnosis from "there were none", and silently
/// returning an empty list would make an unreadable config look like a clean
/// one.
pub fn describe_credential_helpers(repo_path: Option<&str>) -> Vec<String> {
    let out = match run_git(
        repo_path,
        &["config", "--show-origin", "--get-all", "credential.helper"],
    ) {
        Ok(out) => out,
        // Exit 1 with no output is git's way of saying the key is unset, which
        // is a real answer and not a failure.
        Err(_) => return vec!["<none configured>".to_string()],
    };

    let helpers: Vec<String> = out
        .stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            // `--show-origin` prints "<origin>	<value>". Keep both, reversed so
            // the helper reads first: the name is what matters, the file is why.
            match line.split_once('\t') {
                Some((origin, value)) => Some(format!(
                    "{} <- {}",
                    value.trim(),
                    origin.trim().trim_start_matches("file:")
                )),
                None => Some(line.to_string()),
            }
        })
        .collect();

    if helpers.is_empty() {
        vec!["<none configured>".to_string()]
    } else {
        helpers
    }
}

pub struct GitOutput {
    pub stdout: String,
    /// Git's diagnostics from a SUCCESSFUL run. Callers take `.stdout` and drop
    /// the rest, so `log_stderr` records this to the app log instead of losing it:
    /// git reports real warnings here even on exit 0 ("redirecting to a new URL",
    /// ref-update notices, hook output), and those explain later surprises.
    pub stderr: String,
}

impl GitOutput {
    /// Git's diagnostics with surrounding whitespace removed, or None when it said
    /// nothing. Lets a caller surface a warning without re-trimming.
    pub fn warning(&self) -> Option<&str> {
        let trimmed = self.stderr.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }
}

/// Log git's stderr from a successful run, so a warning is diagnosable rather
/// than discarded. Failures already carry stderr in their error message.
fn log_stderr(args: &[&str], out: &GitOutput) {
    if let Some(warning) = out.warning() {
        log::debug!("git {} warned: {}", args.first().unwrap_or(&""), warning);
    }
}

/// `run_git` for work nobody asked for: same call, but Git Credential Manager is
/// told it may not prompt.
///
/// Network commands started by a selection or a timer must fail rather than put
/// a login window in front of someone who is doing something else. Callers pass
/// `credential_args(Attended::Background)` themselves, because those are `-c`
/// options that have to precede the subcommand.
pub fn run_git_unattended(repo_path: Option<&str>, args: &[&str]) -> Result<GitOutput, AppError> {
    run_git_with(repo_path, args, true)
}

pub fn run_git(repo_path: Option<&str>, args: &[&str]) -> Result<GitOutput, AppError> {
    run_git_with(repo_path, args, false)
}

fn run_git_with(
    repo_path: Option<&str>,
    args: &[&str],
    unattended: bool,
) -> Result<GitOutput, AppError> {
    let mut cmd = Command::new(git_program_name());
    if let Some(path) = repo_path {
        cmd.arg("-C").arg(path);
    }
    cmd.args(args);

    // Hand this child the system's libraries, not the AppImage's.
    crate::process_env::scrub_bundled_env(&mut cmd);
    prepare_git_env(&mut cmd);
    if unattended {
        apply_background_env(&mut cmd);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let out = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::Other("git executable not found on PATH".into())
        } else {
            AppError::Io(e)
        }
    })?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    if !out.status.success() {
        let msg = if stderr.trim().is_empty() {
            stdout.clone()
        } else {
            stderr.clone()
        };
        return Err(AppError::Other(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            msg.trim()
        )));
    }

    let out = GitOutput { stdout, stderr };
    log_stderr(args, &out);
    Ok(out)
}

pub fn git_available() -> bool {
    run_git(None, &["--version"]).is_ok()
}

/// Runs git with `stdin` piped in as raw bytes. Used to feed a patch to
/// `git apply`. Returns the raw stdout/stderr; errors carry git's stderr.
pub fn run_git_stdin(
    repo_path: Option<&str>,
    args: &[&str],
    stdin_bytes: &[u8],
) -> Result<GitOutput, AppError> {
    let mut cmd = Command::new(git_program_name());
    if let Some(path) = repo_path {
        cmd.arg("-C").arg(path);
    }
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Hand this child the system's libraries, not the AppImage's.
    crate::process_env::scrub_bundled_env(&mut cmd);
    prepare_git_env(&mut cmd);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::Other("git executable not found on PATH".into())
        } else {
            AppError::Io(e)
        }
    })?;

    child
        .stdin
        .take()
        .ok_or_else(|| AppError::Other("failed to open git stdin".into()))?
        .write_all(stdin_bytes)
        .map_err(AppError::Io)?;

    let out = child.wait_with_output().map_err(AppError::Io)?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    if !out.status.success() {
        let msg = if stderr.trim().is_empty() {
            stdout.clone()
        } else {
            stderr.clone()
        };
        return Err(AppError::Other(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            msg.trim()
        )));
    }

    let out = GitOutput { stdout, stderr };
    log_stderr(args, &out);
    Ok(out)
}

#[cfg(test)]
mod credential_trace_tests {
    use super::*;
    use crate::git::credential_helper::HELPER_FLAG;

    /// The reset must come before the name, or git keeps what it already had and
    /// the duplicate survives. Order is the whole fix, so pin it.
    #[test]
    fn the_helper_reset_precedes_the_helper_name() {
        for attended in [Attended::User, Attended::Background] {
            let args = credential_args(attended);
            let reset = args.iter().position(|a| a == "credential.helper=");
            let set = args
                .iter()
                .position(|a| a.starts_with("credential.helper=") && a != "credential.helper=");
            assert!(reset.is_some() && set.is_some(), "{args:?}");
            assert!(reset < set, "reset must come first: {args:?}");
        }
    }

    /// Both kinds of run name our helper as the only entry. Credential Manager
    /// must NOT also be listed: a push that succeeded on our token would make
    /// git `store` that token into Credential Manager, overwriting the user's
    /// own sign-in -- and on background runs git would erase from it directly,
    /// so the helper would protect nothing.
    #[test]
    fn every_run_goes_through_our_helper_and_never_lists_manager_beside_it() {
        use crate::git::credential_helper::UNATTENDED_FLAG;
        for attended in [Attended::User, Attended::Background] {
            let args = credential_args(attended);
            let Some(front) = args.iter().find(|a| a.contains(HELPER_FLAG)) else {
                // current_exe() failed, so manager alone is the documented fallback.
                assert!(
                    args.contains(&"credential.helper=manager".to_string()),
                    "{args:?}"
                );
                continue;
            };
            assert!(
                !args.contains(&"credential.helper=manager".to_string()),
                "{args:?}"
            );
            assert_eq!(
                args.len(),
                4,
                "exactly one helper after the reset: {args:?}"
            );

            // The path is quoted because a default Windows install sits under
            // `C:\Program Files\`, which unquoted parses as the program `C:\Program`.
            let value = front.trim_start_matches("credential.helper=");
            assert!(
                value.starts_with("!\""),
                "must be a quoted command: {value}"
            );
            // The closing quote has to land before the flag, or the flag is inside
            // the program name.
            let close = value.rfind('"').expect("closing quote");
            let flag = value.find(HELPER_FLAG).expect("flag");
            assert!(close < flag, "flag must sit outside the quotes: {value}");

            // The unattended marker is what stops a background 401 from erasing
            // the stored sign-in; an attended run must not carry it, or the
            // login window it is allowed to open never appears.
            match attended {
                Attended::User => {
                    assert!(!value.contains(UNATTENDED_FLAG), "{value}");
                }
                Attended::Background => {
                    assert!(value.contains(UNATTENDED_FLAG), "{value}");
                }
            }
        }
    }

    /// These are passed to git as global options, so each value needs its own
    /// `-c`. A pair that lost its flag would be read as a subcommand.
    #[test]
    fn every_override_is_introduced_by_its_own_c_flag() {
        for attended in [Attended::User, Attended::Background] {
            let args = credential_args(attended);
            assert_eq!(args.len() % 2, 0, "{args:?}");
            for pair in args.chunks(2) {
                assert_eq!(pair[0], "-c", "{args:?}");
            }
        }
    }

    /// Runs the real thing against the real client. The value of this trace is
    /// entirely in whether it reports the true helper list on a machine we
    /// cannot inspect, so a mocked version would test nothing that matters.
    ///
    /// Asserts shape, not contents: how many helpers this machine has is its own
    /// business, but every line must name one and never come back empty.
    #[test]
    fn the_helper_description_is_never_empty_and_names_a_helper() {
        let helpers = describe_credential_helpers(None);
        assert!(!helpers.is_empty(), "must always report something");
        for line in &helpers {
            assert!(!line.trim().is_empty(), "blank entry in {helpers:?}");
        }
    }

    /// The variable is GCM's, not git's. Naming it wrong fails open - the helper
    /// prompts anyway - which is exactly the bug this guards.
    #[test]
    fn background_suppression_uses_the_helpers_own_variable() {
        assert_eq!(GCM_NON_INTERACTIVE, ("GCM_INTERACTIVE", "never"));
    }
}
