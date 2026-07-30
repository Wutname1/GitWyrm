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

/// Permission kinds denied to the CLI's own agent for the whole run.
///
/// Verified against Copilot CLI 1.0.76 (`copilot help permissions`). The
/// vocabulary is permission *kinds* -- `shell(command)`, `write(path)`,
/// `url(domain)`, and MCP server names -- not the file-operation names an
/// earlier reading of the docs suggested.
///
/// Denials rather than an allow-list, because `--available-tools` names what
/// the model can see while `--deny-tool` is what it cannot use, and denial
/// takes precedence over every allow rule including `--allow-all-tools`. That
/// precedence is the property worth having: it cannot be widened by anything
/// the model or a config file says later.
///
/// `shell` and `url` are denied outright. Those are the side effects the engine
/// gates itself, and a tool the CLI never has is one that cannot slip past a
/// gate. `write` is deliberately *not* denied -- editing files in the
/// repository is the job, and the repository boundary is enforced by our own
/// path checks.
pub const DENIED_TOOLS: &[&str] = &["shell", "url"];

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
    let mut conn = AcpConnection::spawn(&self.program, &self.cwd, DENIED_TOOLS).await?;
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
  fn shell_and_network_access_are_denied() {
    // The engine gates side effects itself; a tool the CLI never has cannot
    // slip past a gate at all. Guards against a denial being dropped.
    assert!(DENIED_TOOLS.contains(&"shell"), "the CLI must not run shell commands");
    assert!(DENIED_TOOLS.contains(&"url"), "the CLI must not reach the network");
  }

  #[test]
  fn writing_files_is_not_denied() {
    // Editing files in the repository is the job. The boundary that keeps
    // those edits inside the repo is our own path check, not this list.
    assert!(!DENIED_TOOLS.contains(&"write"));
  }

  #[test]
  fn denied_tools_use_the_clis_own_vocabulary() {
    // Verified against `copilot help permissions` on 1.0.76: the kinds are
    // shell, write, url, and MCP server names. An earlier list guessed at
    // file-operation names ("view", "edit", "ls") that the CLI does not
    // recognise -- and silently ignores rather than rejecting, so a wrong name
    // here would look like it worked while filtering nothing.
    const KINDS: &[&str] = &["shell", "write", "url"];
    for tool in DENIED_TOOLS {
      assert!(KINDS.contains(tool), "{tool} is not a permission kind the CLI knows");
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
