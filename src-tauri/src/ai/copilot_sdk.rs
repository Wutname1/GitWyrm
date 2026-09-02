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
use github_copilot_sdk::session_events::SessionEventType;
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

/// A live report from a model that is still working.
///
/// The SDK streams the reply as it is produced, so a caller that wants to show
/// the wait can watch these instead of staring at a spinner for two minutes.
#[derive(Debug, Clone)]
pub enum Progress {
    /// The subprocess is starting, or the session is being set up.
    Starting,
    /// The model's own reasoning, streamed as it thinks.
    Thinking(String),
    /// A chunk of the actual answer.
    Answer(String),
}

/// Something that wants to hear about a request while it runs.
///
/// Boxed rather than generic so the streaming and non-streaming paths can share
/// one function body; there is exactly one call per event, so the indirection
/// costs nothing measurable.
pub type ProgressSink<'a> = &'a (dyn Fn(Progress) + Send + Sync);

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
    complete_streaming(github_token, model, system, user, None).await
}

/// [`complete`], reporting the reply as it arrives.
///
/// `send_and_wait` already consumes this same event stream internally and
/// simply keeps the last message, so subscribing alongside it costs one extra
/// receiver and changes nothing about how the request completes.
pub async fn complete_streaming(
    github_token: &str,
    model: &str,
    system: &str,
    user: &str,
    on_progress: Option<ProgressSink<'_>>,
) -> Result<String, AppError> {
    // Starting the bundled CLI is itself a slow step on a cold run, and it
    // happens before the model is even asked -- so say so rather than showing
    // an empty panel for the first few seconds.
    if let Some(report) = on_progress {
        report(Progress::Starting);
    }
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

    // Subscribe BEFORE sending: events emitted between the send and the
    // subscription would otherwise be missed, and the first reasoning chunk is
    // the one that proves to the user that something is happening.
    let pump = on_progress.map(|report| {
        let mut events = session.subscribe();
        // The callback borrows, so the pump has to stay on this thread rather
        // than being spawned onto the runtime. Driving it with `select!`
        // alongside the send keeps both making progress on one task.
        async move {
            while let Ok(event) = events.recv().await {
                let text = |key: &str| {
                    event
                        .data
                        .get(key)
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string()
                };
                match event.parsed_type() {
                    SessionEventType::AssistantReasoningDelta => {
                        let chunk = text("deltaContent");
                        if !chunk.is_empty() {
                            report(Progress::Thinking(chunk));
                        }
                    }
                    SessionEventType::AssistantMessageDelta => {
                        let chunk = text("deltaContent");
                        if !chunk.is_empty() {
                            report(Progress::Answer(chunk));
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    // The SDK has no separate system-prompt slot, so the instruction is folded
    // into the message ahead of the payload.
    let prompt = format!("{system}\n\n{user}");
    let send = session.send_and_wait(MessageOptions::new(prompt).with_wait_timeout(SEND_TIMEOUT));

    let result = match pump {
        Some(pump) => {
            tokio::pin!(send);
            tokio::pin!(pump);
            // The pump only ends when the session closes, so the send is what
            // decides when this is over.
            loop {
                tokio::select! {
                    outcome = &mut send => break outcome,
                    _ = &mut pump => break send.await,
                }
            }
        }
        None => send.await,
    };

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
