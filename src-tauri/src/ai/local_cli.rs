//! Installed AI tools that can power the Core app without a separate API key.

use std::path::Path;
use std::time::Duration;

use crate::ai::agent::{copilot_cli, registry};
use crate::ai::catalog::{CatalogModel, CatalogProvider, Dialect};
use crate::error::AppError;

/// Kept distinct from the HTTP `openai` provider: this uses the person's
/// existing Codex CLI sign-in and subscription, not an OpenAI API key.
pub const CODEX_PROVIDER_ID: &str = "codex-cli";
pub const DEFAULT_MODEL_ID: &str = "default";

pub fn is_local(provider: &str) -> bool {
    provider == CODEX_PROVIDER_ID
}

pub fn codex_provider() -> CatalogProvider {
    CatalogProvider {
        id: CODEX_PROVIDER_ID.into(),
        name: "Codex (installed app)".into(),
        // No request is sent to this URL. Keeping the existing catalog shape
        // avoids making every provider picker understand a second schema.
        base_url: String::new(),
        dialect: Dialect::OpenAi,
        models: vec![default_model()],
    }
}

pub fn model_list() -> crate::ai::models::ModelList {
    crate::ai::models::ModelList {
        models: vec![default_model()],
        // This is the complete live choice: the model stays under the user's
        // Codex settings instead of being duplicated in GitWyrm.
        live: true,
    }
}

fn default_model() -> CatalogModel {
    CatalogModel {
        id: DEFAULT_MODEL_ID.into(),
        name: "Use my Codex model".into(),
        enabled: true,
    }
}

pub fn codex_installed() -> bool {
    let Some(spec) = registry::find("codex") else {
        return false;
    };
    matches!(
        copilot_cli::detect_agent(spec).state,
        copilot_cli::CliState::Ready { .. }
    )
}

pub async fn complete_codex(
    cwd: &Path,
    system: &str,
    user: &str,
    timeout: Duration,
) -> Result<String, AppError> {
    let spec = registry::find("codex")
        .ok_or_else(|| AppError::Other("this build does not include Codex support".into()))?;
    let program = match copilot_cli::detect_agent(spec).state {
        copilot_cli::CliState::Ready { path, .. } => path,
        copilot_cli::CliState::TooOld { .. } => {
            return Err(AppError::Other("Update Codex, then try this again.".into()));
        }
        copilot_cli::CliState::NotFound => {
            return Err(AppError::Other(
                "Install Codex and sign in, then try this again.".into(),
            ));
        }
    };

    // One-shot Core features only ask for text. A read-only session makes
    // that boundary real even if a prompt happens to mention a file change.
    let args = spec
        .acp_args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let mut connection =
        crate::ai::agent::codex::CodexConnection::spawn(Path::new(&program), cwd, &args)
            .await
            .map_err(agent_error)?;
    connection
        .start_session(cwd, true)
        .await
        .map_err(agent_error)?;

    let prompt = format!(
        "{system}\n\nUse only the information below. Do not inspect or change files.\n\n{user}"
    );
    let result = tokio::time::timeout(timeout, connection.ask(&prompt)).await;
    connection.shutdown().await;
    match result {
        Ok(answer) => answer.map_err(agent_error),
        Err(_) => Err(AppError::Other(
            "Codex took too long to answer. Try again.".into(),
        )),
    }
}

fn agent_error(error: crate::ai::agent::transport::AgentError) -> AppError {
    AppError::Other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_codex_is_not_the_openai_api_provider() {
        assert!(is_local("codex-cli"));
        assert!(!is_local("openai"));
        let provider = codex_provider();
        assert_eq!(provider.models[0].id, DEFAULT_MODEL_ID);
    }
}
