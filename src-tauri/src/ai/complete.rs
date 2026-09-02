//! One prompt in, one string out, whichever provider is configured.
//!
//! Every AI feature needs the same three steps: look up the stored credential,
//! find the provider's config, then send the prompt. The only wrinkle is that
//! Copilot cannot use the HTTP chat dialects with our own OAuth app and has to
//! go through the bundled CLI instead (see `ai/copilot_sdk.rs`). That fork lived
//! inline in the commit generator; it lives here now so a second caller cannot
//! quietly get it wrong.
//!
//! This is for one-shot completions. Multi-turn work with tool use belongs to
//! `ai::agent`, which is a different shape entirely.

use std::time::Duration;

use crate::ai::{auth, catalog, client, copilot_sdk};
use crate::error::AppError;

/// Generous ceiling: a drafted change is several markdown files in one reply,
/// and truncating one produces a draft that parses but is missing its tail.
pub const DEFAULT_MAX_TOKENS: u32 = 16_384;

/// Long, because a cold Copilot CLI start plus a large reply legitimately takes
/// minutes. The UI shows staged progress throughout, so a slow answer looks like
/// work rather than a hang.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The credential to send, whichever kind is stored.
fn bearer_for(info: &auth::AuthInfo) -> &str {
    match info {
        auth::AuthInfo::Api { key } => key,
        auth::AuthInfo::Oauth { refresh, .. } => refresh,
    }
}

/// Send `system` + `user` to `provider`/`model` and return the reply text.
pub async fn complete(
    app: &tauri::AppHandle,
    provider: &str,
    model: &str,
    system: &str,
    user: &str,
) -> Result<String, AppError> {
    complete_with(
        app,
        provider,
        model,
        system,
        user,
        DEFAULT_MAX_TOKENS,
        DEFAULT_TIMEOUT,
    )
    .await
}

/// `complete`, with explicit limits for callers whose replies are small enough
/// to want a shorter leash.
pub async fn complete_with(
    app: &tauri::AppHandle,
    provider: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
    timeout: Duration,
) -> Result<String, AppError> {
    complete_streaming(app, provider, model, system, user, max_tokens, timeout, None).await
}

/// `complete_with`, reporting the reply while it is still being written.
///
/// Only the Copilot path can stream today: its SDK is event-based and already
/// consumes a chunk stream internally. The HTTP dialects in `client.rs` send
/// `stream: false`, so for those the sink simply never fires and the caller
/// falls back to showing elapsed time. That is why the sink is optional rather
/// than a required argument -- a caller must work either way.
#[allow(clippy::too_many_arguments)]
pub async fn complete_streaming(
    app: &tauri::AppHandle,
    provider: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
    timeout: Duration,
    on_progress: Option<copilot_sdk::ProgressSink<'_>>,
) -> Result<String, AppError> {
    let info = auth::get(app, provider)?
        .ok_or_else(|| AppError::Other("Connect the selected AI provider first".into()))?;

    if provider == copilot_sdk::PROVIDER_ID {
        return copilot_sdk::complete_streaming(
            bearer_for(&info),
            model,
            system,
            user,
            on_progress,
        )
        .await;
    }

    let provider_config = catalog::find(app, provider).await?;
    client::chat(client::ChatRequest {
        provider: &provider_config,
        bearer: bearer_for(&info),
        model,
        system,
        user,
        max_tokens,
        timeout,
    })
    .await
}
