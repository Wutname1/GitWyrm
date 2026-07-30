//! Choosing a transport for the user's default provider.
//!
//! The engine has no preference of its own. What runs is decided by what the
//! default provider offers and what the user has actually set up -- a user who
//! signed in with Copilot gets the CLI, a user with a key gets the API, and
//! neither is treated as the real one.

use super::transport::{AgentError, Transport};

/// What the user has available for a provider, as far as the caller could
/// determine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Available {
  /// An API key or OAuth credential is stored for this provider.
  pub has_credentials: bool,
  /// A usable provider CLI is installed.
  pub has_cli: bool,
}

/// Which transport to use, or why none can be.
///
/// Kept as a pure decision over inputs so the rule can be tested without a
/// network, a CLI, or a keychain.
pub fn choose(provider_id: &str, available: Available) -> Result<Transport, AgentError> {
  match provider_id {
    // Anthropic is API-key only, deliberately. Their terms prohibit routing
    // requests through Free/Pro/Max plan credentials on behalf of users, and
    // that is the one provider where enforcement has actually been observed.
    // No subprocess fallback: reaching for the CLI here would be the exact
    // thing the terms name.
    "anthropic" => {
      if available.has_credentials {
        Ok(Transport::ApiKey)
      } else {
        Err(AgentError::NeedsReconnect {
          detail: "Claude needs an API key. GitWyrm can't use a Claude subscription for this."
            .into(),
        })
      }
    }

    // Copilot is a subscription, not a key. GitHub documents driving its CLI
    // from third-party tools, and that is the only way to reach real model
    // entitlements -- the API path returns a plan-gated list.
    "github-copilot" => {
      if available.has_cli {
        Ok(Transport::Cli)
      } else if available.has_credentials {
        Err(AgentError::TransportUnavailable {
          transport: Transport::Cli,
          detail: "you're signed in to Copilot, but running tasks needs the Copilot \
                   command-line tool installed as well"
            .into(),
        })
      } else {
        Err(AgentError::NeedsReconnect {
          detail: "sign in to Copilot in settings, and install the Copilot command-line tool"
            .into(),
        })
      }
    }

    // Anything self-hosted or local speaks the OpenAI dialect and often wants
    // no credential at all.
    "openai-compatible" | "ollama" | "lmstudio" | "opencode" => Ok(Transport::OpenAiCompatible),

    // Every other catalog provider is reached with the user's own key.
    _ => {
      if available.has_credentials {
        Ok(Transport::ApiKey)
      } else {
        Err(AgentError::NeedsReconnect {
          detail: "this provider needs an API key before it can run tasks".into(),
        })
      }
    }
  }
}

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

  const NOTHING: Available = Available { has_credentials: false, has_cli: false };
  const KEY: Available = Available { has_credentials: true, has_cli: false };
  const CLI: Available = Available { has_credentials: false, has_cli: true };
  const BOTH: Available = Available { has_credentials: true, has_cli: true };

  #[test]
  fn anthropic_never_reaches_for_a_cli() {
    // The whole point of 2.5: even with a CLI installed and a subscription
    // signed in, Anthropic must not be driven as a subprocess.
    assert!(matches!(choose("anthropic", CLI), Err(AgentError::NeedsReconnect { .. })));
    assert_eq!(choose("anthropic", BOTH).unwrap(), Transport::ApiKey);
    assert_eq!(choose("anthropic", KEY).unwrap(), Transport::ApiKey);
  }

  #[test]
  fn anthropic_without_a_key_says_so_plainly() {
    let err = choose("anthropic", NOTHING).unwrap_err();
    let msg = plain_explanation(&err);
    assert!(msg.contains("API key"), "{msg}");
    // Must not suggest a path that does not exist.
    assert!(!msg.to_lowercase().contains("command-line"), "{msg}");
  }

  #[test]
  fn copilot_uses_the_cli() {
    assert_eq!(choose("github-copilot", CLI).unwrap(), Transport::Cli);
    assert_eq!(choose("github-copilot", BOTH).unwrap(), Transport::Cli);
  }

  #[test]
  fn copilot_signed_in_but_no_cli_names_the_missing_piece() {
    // Distinct from "not signed in": the fix is different, so the sentence is.
    let err = choose("github-copilot", KEY).unwrap_err();
    let msg = plain_explanation(&err);
    assert!(msg.contains("command-line tool"), "{msg}");
    assert!(msg.contains("signed in"), "{msg}");
  }

  #[test]
  fn local_endpoints_need_no_credential() {
    for id in ["ollama", "lmstudio", "opencode", "openai-compatible"] {
      assert_eq!(choose(id, NOTHING).unwrap(), Transport::OpenAiCompatible, "{id}");
    }
  }

  #[test]
  fn an_unknown_provider_uses_its_key() {
    assert_eq!(choose("mistral", KEY).unwrap(), Transport::ApiKey);
    assert!(matches!(choose("mistral", NOTHING), Err(AgentError::NeedsReconnect { .. })));
  }

  #[test]
  fn no_explanation_blames_gitwyrm() {
    let errors = [
      choose("anthropic", NOTHING).unwrap_err(),
      choose("github-copilot", NOTHING).unwrap_err(),
      choose("github-copilot", KEY).unwrap_err(),
      choose("mistral", NOTHING).unwrap_err(),
    ];
    for err in &errors {
      let msg = plain_explanation(err);
      for blame in ["error", "failed", "broken", "unexpected"] {
        assert!(!msg.to_lowercase().contains(blame), "{msg} reads as a GitWyrm fault");
      }
      assert!(!msg.is_empty());
    }
  }
}
