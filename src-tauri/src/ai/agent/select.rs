//! Explaining why the engine's one transport could not run.
//!
//! There used to be a decision here: which transport a provider's default
//! setting implied. That never shipped past the Copilot CLI, so the only
//! thing left is turning an [`AgentError`] into a sentence the console can
//! show without reading as a GitWyrm fault.

use super::transport::AgentError;

/// One sentence explaining an engine failure, in words a beginner can act on.
///
/// Never implies GitWyrm is broken: every branch names the thing that is
/// missing and what would fix it.
pub fn plain_explanation(err: &AgentError) -> String {
    match err {
        AgentError::TransportUnavailable { detail, .. } => detail.clone(),
        AgentError::NeedsReconnect { detail } => detail.clone(),
        AgentError::Refused { detail } => {
            format!("Your AI provider turned this down: {detail}")
        }
        AgentError::Cancelled => "Stopped.".into(),
        AgentError::Failed { detail } => format!("This didn't work: {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::transport::Transport;

    #[test]
    fn no_explanation_blames_gitwyrm() {
        let errors = [
            AgentError::TransportUnavailable {
                transport: Transport::Cli,
                detail: "the Copilot command-line tool is not installed".into(),
            },
            AgentError::NeedsReconnect {
                detail: "sign in to Copilot in settings".into(),
            },
            AgentError::Refused {
                detail: "rate limited".into(),
            },
            AgentError::Failed {
                detail: "the connection to the tool closed".into(),
            },
        ];
        for err in &errors {
            let msg = plain_explanation(err);
            for blame in ["error", "failed", "broken", "unexpected"] {
                assert!(
                    !msg.to_lowercase().contains(blame),
                    "{msg} reads as a GitWyrm fault"
                );
            }
            assert!(!msg.is_empty());
        }
    }
}
