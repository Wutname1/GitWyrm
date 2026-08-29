//! A read-only front for Git Credential Manager, used on unattended git runs.
//!
//! # Why this exists
//!
//! Git reacts to any HTTP 401 by running `erase` on every credential helper it
//! has, on the theory that the credential it just sent must be bad. GitHub
//! answers 401 for more than a bad credential: a repository that was renamed or
//! deleted gets `401 Repository not found` even when the sign-in is perfectly
//! good. So one stale checkout in the open-tabs list -- fetched by the
//! background sweep every fifteen minutes -- deleted the user's saved GitHub
//! sign-in from Credential Manager over and over. Each time they signed in
//! again, the next sweep threw it away, and every push after that opened a
//! login window. Traced on a live machine: the first wipe was a background
//! fetch of a repo the org had renamed.
//!
//! An unattended operation has no business changing the credential store. It
//! cannot prompt, so it cannot replace what it erases; all an erase does there
//! is punish the next thing the user tries by hand. With this helper in the
//! list instead of `manager`, a background fetch still reads the stored
//! credential -- `get` is forwarded to Credential Manager untouched -- but
//! `erase` and `store` stop here and do nothing.
//!
//! Attended operations keep talking to Credential Manager directly. There, a
//! 401 followed by an erase is the right thing: the login window replaces the
//! credential immediately, and a genuinely revoked token would otherwise be
//! offered forever.
//!
//! # What this is not
//!
//! This helper never reads the account the user connected in Settings and never
//! supplies a token of its own. An earlier version did, and on an organization
//! with OAuth App access restrictions that token is refused for git operations
//! outright, which made every push to the org fail before Credential Manager
//! was ever asked. Credential Manager's own GitHub sign-in is what those orgs
//! have approved, so it stays the only source of git credentials.
//!
//! # The protocol
//!
//! Git runs `<helper> <operation>` and writes `key=value` lines to stdin,
//! ending with a blank line. For `get`, the helper replies with its own lines.
//! Saying nothing is legal and means "I have nothing", which is how a `get`
//! that Credential Manager cannot answer still fails cleanly.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

/// The argv token that puts the binary in helper mode.
///
/// Deliberately not a bare word like `get`: this is the *application* binary,
/// and a repo folder or file argument must never be mistaken for a request to
/// read a credential.
pub const HELPER_FLAG: &str = "--credential-helper";

/// Environment variable naming the git program the app itself is using.
///
/// Set by the app on every network command it spawns, so the helper reaches
/// the same git -- and therefore the same Credential Manager -- rather than
/// whichever one is first on PATH. Falls back to `git` when absent, which is
/// what a helper launched outside the app (a test, say) gets.
pub const GIT_PROGRAM_ENV: &str = "GITWYRM_GIT";

/// Whether this process was launched by git to answer a credential request.
///
/// Checked before Tauri initializes: a helper invocation must not build a
/// window, register plugins, or trip the single-instance guard and hand its
/// arguments to the running app. It forwards one request and exits.
pub fn requested(args: &[String]) -> bool {
    args.iter().any(|a| a == HELPER_FLAG)
}

/// The credential operation git asked for: the first argument after the flag.
fn operation(args: &[String]) -> &str {
    args.iter()
        .skip(1)
        .find(|a| *a != HELPER_FLAG)
        .map(String::as_str)
        .unwrap_or("")
}

/// Whether an operation is forwarded to Credential Manager or stops here.
///
/// Only `get` goes through. `store` would re-save what Credential Manager
/// already holds, and `erase` is the whole reason this helper exists: an
/// unattended run must never delete the user's sign-in.
fn forwards(operation: &str) -> bool {
    operation == "get"
}

/// Answer one credential request, then exit.
///
/// Returns the process exit code. Always 0: a helper that fails is expected to
/// say nothing and let git carry on, and a non-zero exit would turn "nothing
/// stored" into a hard fetch failure with a confusing message.
pub fn run(args: &[String]) -> i32 {
    if !forwards(operation(args)) {
        return 0;
    }

    // Read the whole request before spawning: git closes its end after the
    // blank line, and Credential Manager expects the same block verbatim.
    let mut request = Vec::new();
    let _ = std::io::stdin().lock().read_to_end(&mut request);

    let git = std::env::var(GIT_PROGRAM_ENV).unwrap_or_else(|_| "git".to_string());
    let mut cmd = Command::new(git);
    cmd.args(["credential-manager", "get"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Credential Manager's own diagnostics ("Cannot prompt because user
        // interactivity has been disabled") reach git's stderr as they do
        // when it is called directly, so nothing is hidden by the detour.
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(super::shell::CREATE_NO_WINDOW);
    }

    let Ok(mut child) = cmd.spawn() else {
        // No Credential Manager reachable: say nothing, exactly as a helper
        // with no stored credential would.
        return 0;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&request);
        // Dropping closes the pipe, which is how Credential Manager knows the
        // request is complete.
    }
    let Ok(output) = child.wait_with_output() else {
        return 0;
    };

    // Forward the reply byte-for-byte. It is never inspected or logged here:
    // the credential passes through this process on the way to git and leaves
    // no trace in it.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = out.write_all(&output.stdout);
    let _ = out.flush();
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_explicit_flag_enters_helper_mode() {
        assert!(requested(&[
            "gitwyrm.exe".into(),
            HELPER_FLAG.into(),
            "get".into()
        ]));
        // A repo path from the Explorer context menu must open the app.
        assert!(!requested(&["gitwyrm.exe".into(), "C:/code/get".into()]));
        assert!(!requested(&["gitwyrm.exe".into()]));
    }

    #[test]
    fn the_operation_is_the_argument_after_the_flag() {
        assert_eq!(
            operation(&["gitwyrm.exe".into(), HELPER_FLAG.into(), "get".into()]),
            "get"
        );
        assert_eq!(operation(&["gitwyrm.exe".into(), HELPER_FLAG.into()]), "");
    }

    /// The regression this helper exists to prevent. An unattended git run must
    /// be able to read the stored sign-in and must never be able to delete it:
    /// git erases on every 401, and GitHub sends 401 for a renamed repository
    /// even when the sign-in is fine.
    #[test]
    fn reads_are_forwarded_and_writes_stop_here() {
        assert!(forwards("get"));
        assert!(!forwards("erase"));
        assert!(!forwards("store"));
        assert!(!forwards(""));
    }
}
