//! GitHub Copilot via GitHub's own SDK, which drives the Copilot CLI.
//!
//! Why this exists instead of talking to api.githubcopilot.com directly:
//! Copilot only returns real model entitlements to OAuth apps on its approved
//! client allowlist. GitWyrm's app is not on it, and the failure is silent --
//! the endpoint answers 200 with a short public model list and
//! `model_picker_enabled: false` on every entry, then rejects the eventual chat
//! call with `model_not_supported`. Measured on one Copilot Business seat: a
//! token from an approved client saw 29 models with 12 enabled; a token from
//! GitWyrm's own app saw ~8 with 0. Scope and request headers made no
//! difference -- only the app identity mattered.
//!
//! The CLI *is* on that allowlist, so routing through it gets the user's real
//! entitlements. The SDK embeds the CLI binary (`bundled-cli`, on by default)
//! and extracts it to a per-user cache on first use, so there is nothing for
//! the user to install.
//!
//! Costs to know about: the SDK is an agent runtime speaking JSON-RPC to a
//! subprocess, not a chat-completions endpoint, so it does not fit the
//! `Dialect` split in `client.rs` and lives as its own path. Starting a client
//! spawns that subprocess, which is why callers should do it once per request
//! and stop it rather than holding one open.

use std::sync::Arc;
use std::time::Duration;

use github_copilot_sdk::handler::DenyAllHandler;
use github_copilot_sdk::rpc::ModelsListRequest;
use github_copilot_sdk::{Client, ClientOptions, MessageOptions, SessionConfig};

use super::catalog::CatalogModel;
use crate::error::AppError;

pub const PROVIDER_ID: &str = "github-copilot";

/// The CLI can take a moment to start on first use, when it extracts itself.
const SEND_TIMEOUT: Duration = Duration::from_secs(90);

/// Starts the bundled CLI. Each call spawns a subprocess, so callers should
/// reuse the returned client for the whole operation and stop it afterwards.
async fn start() -> Result<Client, AppError> {
    Client::start(ClientOptions::default()).await.map_err(|e| {
    log::error!("copilot sdk: could not start the Copilot CLI: {e}");
    AppError::Other(format!(
      "Could not start GitHub Copilot. Make sure you are signed in to Copilot, then try again. ({e})"
    ))
  })
}

/// The models this account's Copilot plan can actually use.
///
/// `list()` without a token returns only the `auto` pseudo-model; the real
/// per-user entitlements need the GitHub token passed explicitly.
pub async fn list_models(github_token: &str) -> Result<Vec<CatalogModel>, AppError> {
    let client = start().await?;
    let result = client
        .rpc()
        .models()
        .list_with_params(ModelsListRequest {
            git_hub_token: Some(github_token.to_string()),
        })
        .await;
    client.stop().await.ok();

    let list = result.map_err(|e| {
        log::error!("copilot sdk: model list failed: {e}");
        AppError::Other(format!("Could not read your Copilot models: {e}"))
    })?;

    let models: Vec<CatalogModel> = list
        .models
        .into_iter()
        // `auto` lets Copilot choose, which is a reasonable default but reads as a
        // model name in a picker. Keep it -- it is genuinely selectable -- but it
        // sorts first below so it reads as the default rather than an odd entry.
        .map(|m| CatalogModel {
            // Everything this endpoint returns is already entitled, unlike the raw
            // HTTP list where entries come back disabled.
            enabled: true,
            id: m.id,
            name: m.name,
        })
        .collect();

    log::info!("copilot sdk: {} models available", models.len());
    Ok(models)
}

/// One-shot prompt. Returns the assistant's text.
///
/// The permission handler rejects everything: generating a commit message is a
/// pure text transform and the agent has no business reading or writing files.
pub async fn complete(
    github_token: &str,
    model: &str,
    system: &str,
    user: &str,
) -> Result<String, AppError> {
    let client = start().await?;

    let mut config = SessionConfig::default()
        .with_permission_handler(Arc::new(DenyAllHandler))
        .with_github_token(github_token);
    // `auto` means "let Copilot decide", which is what omitting the model does.
    if !model.is_empty() && model != "auto" {
        config = config.with_model(model);
    }

    let session = match client.create_session(config).await {
        Ok(session) => session,
        Err(e) => {
            client.stop().await.ok();
            log::error!("copilot sdk: could not create a session: {e}");
            return Err(AppError::Other(format!(
                "Could not start a Copilot session: {e}"
            )));
        }
    };

    // The SDK has no separate system-prompt slot, so the instruction is folded
    // into the message ahead of the payload.
    let prompt = format!("{system}\n\n{user}");
    let result = session
        .send_and_wait(MessageOptions::new(prompt).with_wait_timeout(SEND_TIMEOUT))
        .await;

    session.disconnect().await.ok();
    client.stop().await.ok();

    let event = result
        .map_err(|e| {
            log::error!("copilot sdk: request failed: {e}");
            AppError::Other(format!("Copilot could not answer: {e}"))
        })?
        .ok_or_else(|| {
            log::error!("copilot sdk: request finished with no reply");
            AppError::Other(
                "Copilot finished without replying. No changes were made -- try again.".into(),
            )
        })?;

    let text = event
        .data
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();

    if text.is_empty() {
        log::error!(
            "copilot sdk: reply had no text (event_type={})",
            event.event_type
        );
        return Err(AppError::Other(
            "Copilot replied with nothing. No changes were made -- try again.".into(),
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    /// Live check against the signed-in Copilot account. Ignored by default so
    /// CI and offline machines are not gated on a network round-trip; run with
    /// `cargo test --lib copilot_sdk -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn lists_models_and_answers_a_prompt() {
        tauri::async_runtime::block_on(run());
    }

    async fn run() {
        let raw = std::fs::read_to_string(
            dirs_next_home().join("AppData/Roaming/dev.gitwyrm.app/auth.json"),
        )
        .expect("auth.json");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let token = v["github-copilot"]["refresh"]
            .as_str()
            .expect("copilot token");

        let models = super::list_models(token).await.expect("model list");
        println!("models = {}", models.len());
        for m in &models {
            println!("  {} | {}", m.id, m.name);
        }
        assert!(
            models.len() > 1,
            "expected more than the `auto` pseudo-model"
        );

        let text = super::complete(
            token,
            "claude-haiku-4.5",
            "Reply with exactly one word.",
            "Say PONG.",
        )
        .await
        .expect("completion");
        println!("reply = {text:?}");
        assert!(!text.is_empty());
    }

    fn dirs_next_home() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE"))
    }
}
