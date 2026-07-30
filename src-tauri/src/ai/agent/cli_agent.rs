//! The CLI transport: a [`ProviderAgent`] backed by the Copilot CLI's ACP
//! server.
//!
//! Exists so a subscription-only user is not shut out. GitWyrm never reads the
//! CLI's stored credentials -- it asks the tool whether it can work and
//! believes the answer, which is the whole difference between driving a tool
//! and impersonating one.

use std::path::PathBuf;

use super::acp::{AcpConnection, StopReason};
use super::copilot_cli::{self, CliState};
use super::transport::{AgentError, Transport};

/// Tools the CLI's own agent may use inside a run.
///
/// Applied when the ACP server starts, because tool filtering is fixed at
/// launch: a client cannot narrow it per session. Deliberately excludes any
/// shell or network tool -- those are the ones the engine gates itself, and a
/// tool the CLI never has is one that cannot slip past a gate.
pub const AVAILABLE_TOOLS: &[&str] = &["view", "edit", "create", "ls", "glob", "grep"];

pub struct CliAgent {
  program: PathBuf,
  cwd: PathBuf,
}

impl CliAgent {
  /// Builds the transport if a usable CLI is installed.
  ///
  /// The version floor and the "not installed" case produce different
  /// sentences, because they need different actions from the user.
  pub fn discover(cwd: PathBuf) -> Result<Self, AgentError> {
    match &copilot_cli::detect().state {
      CliState::Ready { path, version } => {
        log::info!("Copilot CLI transport ready: {version}");
        Ok(Self { program: PathBuf::from(path), cwd })
      }
      CliState::TooOld { version, minimum } => Err(AgentError::TransportUnavailable {
        transport: Transport::Cli,
        detail: format!(
          "the Copilot command-line tool is version {version}, but {minimum} or newer is needed. \
           Running `copilot update` will bring it up to date"
        ),
      }),
      CliState::NotFound => Err(AgentError::TransportUnavailable {
        transport: Transport::Cli,
        detail: "the Copilot command-line tool is not installed".into(),
      }),
    }
  }

  /// Opens a session, proving the tool is installed *and* signed in.
  ///
  /// This is the check that matters: a credential file can look complete while
  /// its sign-in lacks the scope the tool needs, which is the spike's own
  /// failure case. Only the tool can answer that, so we ask it.
  pub async fn connect(&self) -> Result<AcpConnection, AgentError> {
    let mut conn = AcpConnection::spawn(&self.program, &self.cwd, AVAILABLE_TOOLS).await?;
    conn.start_session(&self.cwd).await?;
    Ok(conn)
  }
}

impl super::transport::ProviderAgent for CliAgent {
  fn transport(&self) -> Transport {
    Transport::Cli
  }

  async fn check(&self) -> Result<(), AgentError> {
    // A session that opens and closes cleanly is the proof. Cheaper checks --
    // does the binary exist, is there a token on disk -- answer a different
    // question than "can this actually run a task right now".
    let conn = self.connect().await?;
    conn.shutdown().await;
    Ok(())
  }

  async fn turn(
    &self,
    _req: super::transport::TurnRequest<'_>,
  ) -> Result<super::transport::AgentTurn, AgentError> {
    // The CLI runs its own agent loop: it plans, calls its own tools, and
    // reports what it did. Mapping that onto a single-turn request/response
    // would mean pretending it is a chat completion, which it is not.
    //
    // The engine drives this transport through `connect` and the session's
    // stream instead. Left unimplemented rather than faked so a caller that
    // reaches for the wrong shape fails loudly here rather than silently doing
    // half a run.
    Err(AgentError::Failed {
      detail: "the command-line transport runs whole tasks, not single turns".into(),
    })
  }
}

/// How a finished session should be reported, given why it stopped.
///
/// Separate from [`StopReason`] so the console never has to know protocol
/// vocabulary: every one of these is a sentence a beginner can act on.
pub fn outcome_sentence(stop: StopReason, budget_spent: bool) -> String {
  if budget_spent {
    return "Didn't finish: it used all its steps. You can give it more steps in settings \
            and try again."
      .into();
  }
  match stop {
    StopReason::EndTurn => "Finished.".into(),
    StopReason::MaxTurnRequests => {
      "Didn't finish: the AI reached its own step limit. Try a smaller task.".into()
    }
    StopReason::MaxTokens => {
      "Didn't finish: the reply grew too long to continue. Try a smaller task.".into()
    }
    StopReason::Refusal => "Didn't finish: the AI declined to continue.".into(),
    StopReason::Cancelled => "Stopped.".into(),
    StopReason::Unknown => "Didn't finish: it stopped for a reason GitWyrm didn't recognise.".into(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_tool_set_has_no_shell_or_network_tool() {
    // The engine gates side effects; a tool the CLI never has cannot slip past
    // a gate at all. Guards against one being added without that being noticed.
    for tool in AVAILABLE_TOOLS {
      assert!(
        !matches!(*tool, "bash" | "shell" | "run" | "fetch" | "web" | "network"),
        "{tool} would let the CLI act outside the bounded set"
      );
    }
  }

  #[test]
  fn every_stop_reason_produces_a_sentence_without_jargon() {
    let all = [
      StopReason::EndTurn,
      StopReason::MaxTokens,
      StopReason::MaxTurnRequests,
      StopReason::Refusal,
      StopReason::Cancelled,
      StopReason::Unknown,
    ];
    for stop in all {
      let s = outcome_sentence(stop, false);
      assert!(!s.is_empty());
      for jargon in ["stopReason", "end_turn", "max_turn_requests", "ACP", "token"] {
        assert!(!s.contains(jargon), "{stop:?} produced jargon: {s}");
      }
    }
  }

  #[test]
  fn a_spent_budget_is_reported_as_ours_not_the_agents() {
    // Our budget running out and the agent's own ceiling are different causes
    // with different fixes, so they must not share a sentence.
    let ours = outcome_sentence(StopReason::EndTurn, true);
    let theirs = outcome_sentence(StopReason::MaxTurnRequests, false);
    assert_ne!(ours, theirs);
    assert!(ours.contains("settings"), "ours should say where to change it: {ours}");
  }
}
