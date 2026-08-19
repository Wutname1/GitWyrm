//! The CLI transport: a [`ProviderAgent`] backed by the Copilot CLI's ACP
//! server.
//!
//! Exists so a subscription-only user is not shut out. GitWyrm never reads the
//! CLI's stored credentials -- it asks the tool whether it can work and
//! believes the answer, which is the whole difference between driving a tool
//! and impersonating one.

use std::path::PathBuf;

use super::acp::AcpConnection;
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
                Ok(Self {
                    program: PathBuf::from(path),
                    cwd,
                })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_and_network_access_are_denied() {
        // The engine gates side effects itself; a tool the CLI never has cannot
        // slip past a gate at all. Guards against a denial being dropped.
        assert!(
            DENIED_TOOLS.contains(&"shell"),
            "the CLI must not run shell commands"
        );
        assert!(
            DENIED_TOOLS.contains(&"url"),
            "the CLI must not reach the network"
        );
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
            assert!(
                KINDS.contains(tool),
                "{tool} is not a permission kind the CLI knows"
            );
        }
    }
}
