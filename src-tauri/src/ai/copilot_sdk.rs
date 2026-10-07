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
            selection_id: None,
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

/// What one model costs in AI credits per million tokens.
///
/// Copilot publishes prices per billing batch (`batchSize` tokens), which is a
/// unit nobody reads. Per million is what Copilot's own picker shows.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TokenCredits {
    pub input: f32,
    /// Reading from the prompt cache, when Copilot prices it separately.
    pub cached_input: Option<f32>,
    pub output: f32,
}

/// One Copilot model with what it costs, for pickers that show the price.
///
/// Every number is optional: the SDK marks all of billing as experimental and
/// the `auto` pseudo-model carries a discount rather than prices, so a missing
/// figure means "Copilot did not say", never zero.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CopilotModelInfo {
    pub id: String,
    pub name: String,
    /// Premium-request multiplier relative to the base rate (`1`, `0.33`).
    pub multiplier: Option<f32>,
    pub credits_per_million: Option<TokenCredits>,
    /// Total context window in tokens.
    pub context_window: Option<u32>,
    /// Copilot's relative cost tier: `low`, `medium`, `high` or `very_high`.
    pub price_category: Option<String>,
    /// Reasoning-effort levels this model accepts, in Copilot's order. Empty
    /// when it takes none.
    pub efforts: Vec<String>,
    /// The effort Copilot uses when none is chosen.
    pub default_effort: Option<String>,
}

/// [`list_models`], keeping what each model costs.
///
/// A separate function rather than a change to `list_models`, whose callers
/// only want ids and names and should not grow a dependency on the SDK's
/// experimental billing shape.
pub async fn list_models_detailed(github_token: &str) -> Result<Vec<CopilotModelInfo>, AppError> {
    let client = start().await?;
    let result = client
        .rpc()
        .models()
        .list_with_params(ModelsListRequest {
            git_hub_token: Some(github_token.to_string()),
            selection_id: None,
        })
        .await;
    client.stop().await.ok();

    let list = result.map_err(|e| {
        log::error!("copilot sdk: detailed model list failed: {e}");
        AppError::Other(format!("Could not read your Copilot models: {e}"))
    })?;

    let models: Vec<CopilotModelInfo> = list.models.into_iter().map(model_info).collect();
    log::info!(
        "copilot sdk: {} models available, {} with token prices",
        models.len(),
        models.iter().filter(|m| m.credits_per_million.is_some()).count()
    );
    Ok(models)
}

/// One SDK model as GitWyrm shows it. Pure, so the mapping is testable without
/// starting the CLI.
fn model_info(m: github_copilot_sdk::rpc::Model) -> CopilotModelInfo {
    use github_copilot_sdk::rpc::ModelPickerPriceCategory as Tier;

    let billing = m.billing.as_ref();
    let context_window = m
        .capabilities
        .limits
        .as_ref()
        .and_then(|l| l.max_context_window_tokens)
        .filter(|n| *n > 0)
        .map(|n| n.min(u32::MAX as i64) as u32);
    let price_category = match m.model_picker_price_category {
        Some(Tier::Low) => Some("low"),
        Some(Tier::Medium) => Some("medium"),
        Some(Tier::High) => Some("high"),
        Some(Tier::VeryHigh) => Some("very_high"),
        Some(Tier::Unknown) | None => None,
    }
    .map(str::to_string);

    CopilotModelInfo {
        multiplier: billing.and_then(|b| b.multiplier).map(|x| x as f32),
        credits_per_million: billing
            .and_then(|b| b.token_prices.as_ref())
            .and_then(credits_per_million),
        context_window,
        price_category,
        efforts: m.supported_reasoning_efforts.unwrap_or_default(),
        default_effort: m.default_reasoning_effort,
        id: m.id,
        name: m.name,
    }
}

/// Batch prices converted to credits per million tokens.
///
/// `None` unless both input and output are priced and the batch size is
/// usable: a card showing an input price with no output price would read as
/// "output is free".
fn credits_per_million(
    prices: &github_copilot_sdk::rpc::ModelBillingTokenPrices,
) -> Option<TokenCredits> {
    let batch = prices.batch_size.filter(|b| *b > 0)? as f64;
    let per_million = |price: f64| (price * 1_000_000.0 / batch) as f32;
    Some(TokenCredits {
        input: per_million(prices.input_price?),
        cached_input: prices.cache_read_price.map(per_million),
        output: per_million(prices.output_price?),
    })
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
mod pricing_tests {
    use super::*;
    use github_copilot_sdk::rpc::{
        Model, ModelBilling, ModelBillingTokenPrices, ModelCapabilities, ModelCapabilitiesLimits,
        ModelPickerPriceCategory,
    };

    fn prices(batch: i64, input: f64, cached: Option<f64>, output: f64) -> ModelBillingTokenPrices {
        ModelBillingTokenPrices {
            batch_size: Some(batch),
            input_price: Some(input),
            cache_read_price: cached,
            output_price: Some(output),
            ..Default::default()
        }
    }

    #[test]
    fn a_million_token_batch_is_priced_as_is() {
        let c = credits_per_million(&prices(1_000_000, 1_000.0, Some(100.0), 5_000.0)).unwrap();
        assert_eq!(c.input, 1_000.0);
        assert_eq!(c.cached_input, Some(100.0));
        assert_eq!(c.output, 5_000.0);
    }

    #[test]
    fn a_smaller_batch_is_scaled_up_to_a_million() {
        let c = credits_per_million(&prices(1_000, 1.0, Some(0.1), 5.0)).unwrap();
        assert!((c.input - 1_000.0).abs() < 1e-3);
        assert!((c.cached_input.unwrap() - 100.0).abs() < 1e-3);
        assert!((c.output - 5_000.0).abs() < 1e-3);
    }

    #[test]
    fn an_unusable_batch_or_missing_side_gives_no_price() {
        assert!(credits_per_million(&prices(0, 1.0, None, 5.0)).is_none());
        let mut no_output = prices(1_000, 1.0, None, 5.0);
        no_output.output_price = None;
        assert!(credits_per_million(&no_output).is_none());
        let mut no_batch = prices(1_000, 1.0, None, 5.0);
        no_batch.batch_size = None;
        assert!(credits_per_million(&no_batch).is_none());
    }

    #[test]
    fn a_model_with_billing_carries_its_price() {
        let m = Model {
            id: "gpt-5.5".into(),
            name: "GPT-5.5".into(),
            billing: Some(ModelBilling {
                multiplier: Some(0.33),
                token_prices: Some(prices(1_000_000, 1_000.0, None, 5_000.0)),
                ..Default::default()
            }),
            capabilities: ModelCapabilities {
                limits: Some(ModelCapabilitiesLimits {
                    max_context_window_tokens: Some(400_000),
                    ..Default::default()
                }),
                ..Default::default()
            },
            model_picker_price_category: Some(ModelPickerPriceCategory::VeryHigh),
            supported_reasoning_efforts: Some(vec!["low".into(), "high".into()]),
            default_reasoning_effort: Some("low".into()),
            ..Default::default()
        };
        let info = model_info(m);
        assert_eq!(info.id, "gpt-5.5");
        assert_eq!(info.name, "GPT-5.5");
        assert!((info.multiplier.unwrap() - 0.33).abs() < 1e-6);
        let credits = info.credits_per_million.unwrap();
        assert_eq!((credits.input, credits.cached_input, credits.output), (1_000.0, None, 5_000.0));
        assert_eq!(info.context_window, Some(400_000));
        assert_eq!(info.price_category.as_deref(), Some("very_high"));
        assert_eq!(info.efforts, vec!["low", "high"]);
        assert_eq!(info.default_effort.as_deref(), Some("low"));
    }

    #[test]
    fn a_model_without_billing_says_nothing_rather_than_zero() {
        let info = model_info(Model { id: "auto".into(), name: "Auto".into(), ..Default::default() });
        assert_eq!(info.id, "auto");
        assert!(info.multiplier.is_none());
        assert!(info.credits_per_million.is_none());
        assert!(info.context_window.is_none());
        assert!(info.price_category.is_none());
        assert!(info.efforts.is_empty());
    }
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
        // Deliberately not `models.len() > 1`, which is what this asserted and
        // what made it fail on a perfectly valid account. That is a claim about
        // somebody's GitHub plan, not about GitWyrm, and it aborted before the
        // half of this test that exercises our own code ever ran.
        //
        // The count still matters, so it is printed above and warned about in
        // `models::list` -- an environment fact belongs in the output, not in a
        // pass/fail gate. What is asserted here is the thing the test is named
        // for: a list came back.
        assert!(!models.is_empty(), "expected at least one model");

        // Driven by what the account actually has, rather than a hardcoded
        // `claude-haiku-4.5` that a minimal seat cannot use -- which would have
        // failed one line later for the same wrong reason. On a rich account
        // this exercises the explicit-model branch; on a thin one, the `auto`
        // branch. Both are real paths.
        let model = models[0].id.clone();
        println!("completing with {model:?}");
        let text = super::complete(
            token,
            &model,
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
