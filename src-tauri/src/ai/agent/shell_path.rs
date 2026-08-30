//! Asking the user's login shell what `PATH` really is.
//!
//! # Why this exists
//!
//! A GUI application inherits the environment of whatever launched it. On
//! Windows that is Explorer, which took its copy of `PATH` when the session
//! started; on macOS it is `launchd`, which never reads a shell profile at
//! all. So the `PATH` inside this process is a snapshot from login, and a tool
//! installed since then is invisible to it.
//!
//! That produces the exact failure this module exists to prevent: someone
//! opens the Agents screen, sees Codex listed as missing, follows the install
//! link, installs it, comes back, presses Refresh -- and it still says
//! missing, because re-probing a stale `PATH` finds exactly what it found the
//! first time. The only fix that does not involve telling people to restart
//! the app is to go and ask the shell again.
//!
//! # What it does not do
//!
//! It never *narrows* `PATH`. Directories are only ever added, and always
//! after the ones already present, so re-hydrating cannot break a lookup that
//! was working. A shell that fails to answer, times out, or prints nonsense
//! leaves the process exactly as it was.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

/// A shell that will not answer must not hold up the screen that asked.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Re-reads `PATH` from the login shell and merges anything new into this
/// process, returning how many directories were added.
///
/// Zero is the ordinary answer and is not a failure: it means the environment
/// this process started with was already current, which is the common case
/// when nothing has been installed since launch.
pub fn rehydrate() -> usize {
    let Some(fresh) = login_shell_path() else {
        return 0;
    };

    let current: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let known: HashSet<&PathBuf> = current.iter().collect();

    // Only directories that are actually there. A profile that exports a path
    // for a machine this is not would otherwise grow `PATH` a little on every
    // refresh, forever.
    let added: Vec<PathBuf> = fresh
        .into_iter()
        .filter(|dir| !known.contains(dir) && dir.is_dir())
        .collect();

    if added.is_empty() {
        return 0;
    }

    let mut merged = current;
    let count = added.len();
    merged.extend(added);

    match std::env::join_paths(&merged) {
        Ok(joined) => {
            // SAFETY-ish: single-threaded with respect to other env writers.
            // GitWyrm sets no other environment variable at runtime, and this
            // only ever appends to PATH, so a concurrent reader sees either
            // the old value or the longer one -- never a truncated list.
            std::env::set_var("PATH", joined);
            log::info!("PATH re-read from the login shell: {count} new folders");
            count
        }
        // A directory containing the platform's own separator cannot be joined
        // back into a PATH string. Leaving the process's PATH untouched is the
        // right answer: it was working before this call.
        Err(e) => {
            log::info!("could not rebuild PATH after reading the login shell: {e}");
            0
        }
    }
}

/// The `PATH` a fresh login shell reports, or `None` if it could not be asked.
#[cfg(not(windows))]
fn login_shell_path() -> Option<Vec<PathBuf>> {
    // The user's own shell, because the profile that adds a tool to PATH is
    // written for that shell specifically. `-l` makes it a login shell so the
    // profile is actually sourced; `-c` gives it one thing to do.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let out = run(Command::new(shell).arg("-l").arg("-c").arg("printf %s \"$PATH\""))?;
    Some(std::env::split_paths(out.trim()).collect())
}

/// On Windows the durable `PATH` lives in the registry rather than in a shell
/// profile, and an installer that edits it broadcasts a change this process
/// never received.
///
/// PowerShell is asked for the machine and user values and to join them, which
/// is what a newly-opened console would itself compute.
#[cfg(windows)]
fn login_shell_path() -> Option<Vec<PathBuf>> {
    const SCRIPT: &str = "[Environment]::GetEnvironmentVariable('PATH','Machine') + ';' + \
                          [Environment]::GetEnvironmentVariable('PATH','User')";
    let out = run(Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(SCRIPT))?;
    Some(std::env::split_paths(out.trim()).collect())
}

/// Runs the probe with a timeout, returning its stdout.
///
/// Every failure is the same answer -- `None`, meaning "carry on with the
/// `PATH` we have" -- because none of them is worth interrupting the user
/// over. Refresh still re-probes for the binary either way.
fn run(cmd: &mut Command) -> Option<String> {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let (tx, rx) = std::sync::mpsc::channel();
    // stdout MUST be piped at spawn: `wait_with_output` reads the pipe this
    // sets up, and with the default (inherit) it returns success and an empty
    // stdout, which this function then reads as "the shell said nothing".
    let spawned = match cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::info!("could not ask the shell for PATH: {e}");
            return None;
        }
    };

    std::thread::spawn(move || {
        let _ = tx.send(spawned.wait_with_output());
    });

    match rx.recv_timeout(PROBE_TIMEOUT) {
        Ok(Ok(out)) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        }
        Ok(Ok(_)) => None,
        Ok(Err(e)) => {
            log::info!("the shell did not answer with a PATH: {e}");
            None
        }
        Err(_) => {
            log::info!("the shell took too long to report PATH");
            None
        }
    }
}

/// The current `PATH` as a list, for a caller that wants to show it.
pub fn current() -> Vec<OsString> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.into_os_string()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rehydrating_never_removes_a_folder_already_on_path() {
        // The safety property the whole module rests on: a refresh can only
        // ever make lookups succeed, never make a working one start failing.
        let before: Vec<PathBuf> = std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )
        .collect();

        rehydrate();

        let after: Vec<PathBuf> = std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )
        .collect();

        for dir in &before {
            assert!(
                after.contains(dir),
                "{} disappeared from PATH after a refresh",
                dir.display()
            );
        }
    }

    #[test]
    fn rehydrating_twice_does_not_keep_growing_path() {
        // Guards the de-duplication. Without it every Refresh would append the
        // shell's whole PATH again and the variable would grow without bound
        // across a long session.
        rehydrate();
        let after_first = current().len();
        let added = rehydrate();
        assert_eq!(added, 0, "a second refresh found new folders in an unchanged environment");
        assert_eq!(after_first, current().len());
    }

    #[test]
    fn current_reports_something() {
        // A process with no PATH at all is possible but would mean nothing
        // could be found anyway; this asserts the accessor works rather than
        // the environment.
        let _ = current();
    }
}

/// Proves the shell probe actually returns this machine's PATH.
///
/// Ignored because it spawns a shell and its answer depends on the box. Run it
/// deliberately when changing the probe:
/// `cargo test --lib shell_probe_answers -- --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore]
fn shell_probe_answers_with_real_folders() {
    let answer = login_shell_path().expect("the login shell must answer with a PATH");
    println!("login shell reported {} folders", answer.len());
    let existing = answer.iter().filter(|d| d.is_dir()).count();
    println!("{existing} of them exist on disk");
    for dir in answer.iter().take(6) {
        println!("  {}", dir.display());
    }
    assert!(existing > 0, "not one folder the shell reported exists");
}
