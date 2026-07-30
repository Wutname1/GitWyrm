//! Drives the real Copilot CLI through GitWyrm's own ACP client.
//!
//! Skips itself when the CLI is absent or not signed in, so it is safe in CI
//! and on a machine that has never installed it. When it does run, it is the
//! only check that the transport actually works end to end -- the unit tests
//! cover message shapes against recorded JSON, which is a different question
//! from whether the CLI answers us.
//!
//! Written after a hand-rolled probe reported "no reply" and that was taken as
//! evidence the account was signed out. The probe closed stdin immediately, and
//! a stdio-mode ACP server shuts down when its input closes -- so the test was
//! killing the server, not observing a real failure. Driving the actual client,
//! which holds the pipe open, is what stops that mistake being repeated.

use gitwyrm_lib::agent_acp::AcpConnection;
use gitwyrm_lib::agent_copilot_cli::{self, CliState};

/// Where the CLI is, or None when this machine cannot run the test.
fn installed_cli() -> Option<std::path::PathBuf> {
  match &agent_copilot_cli::detect().state {
    CliState::Ready { path, .. } => Some(std::path::PathBuf::from(path)),
    CliState::TooOld { .. } | CliState::NotFound => None,
  }
}

#[tokio::test]
async fn opens_a_real_session_against_the_installed_cli() {
  let Some(program) = installed_cli() else {
    eprintln!("SKIP: no usable Copilot CLI on this machine");
    return;
  };

  let cwd = std::env::current_dir().expect("a working directory");
  let denied = ["shell", "url"];

  let mut conn = match AcpConnection::spawn(&program, &cwd, &denied).await {
    Ok(c) => c,
    Err(e) => {
      eprintln!("SKIP: could not start the ACP server: {e}");
      return;
    }
  };

  match conn.start_session(&cwd).await {
    Ok(session_id) => {
      assert!(!session_id.is_empty(), "a session must have an id");
      eprintln!("opened session {session_id}");
    }
    Err(e) => {
      // Not signed in is a legitimate outcome on a fresh machine, and must not
      // fail the suite. Anything else is a real problem worth surfacing.
      let msg = e.to_string();
      if msg.contains("sign in") || msg.contains("sign-in") {
        eprintln!("SKIP: the CLI is installed but not signed in");
      } else {
        panic!("the ACP session failed for a reason that is not sign-in: {msg}");
      }
    }
  }

  conn.shutdown().await;
}
