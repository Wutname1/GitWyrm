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

use std::path::Path;
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

/// `complete`, with the repository an installed AI tool is allowed to read.
pub async fn complete_in(
    app: &tauri::AppHandle,
    provider: &str,
    model: &str,
    cwd: &Path,
    system: &str,
    user: &str,
) -> Result<String, AppError> {
    complete_with_in(
        app,
        provider,
        model,
        cwd,
        system,
        user,
        DEFAULT_MAX_TOKENS,
        DEFAULT_TIMEOUT,
    )
    .await
}

/// `complete`, with explicit limits and the repository the local CLI is
/// allowed to read.
pub async fn complete_with_in(
    app: &tauri::AppHandle,
    provider: &str,
    model: &str,
    cwd: &Path,
    system: &str,
    user: &str,
    max_tokens: u32,
    timeout: Duration,
) -> Result<String, AppError> {
    if crate::ai::local_cli::is_local(provider) {
        return crate::ai::local_cli::complete_codex(cwd, system, user, timeout).await;
    }

    let info = auth::get(app, provider)?
        .ok_or_else(|| AppError::Other("Connect the selected AI provider first".into()))?;

    if provider == copilot_sdk::PROVIDER_ID {
        return copilot_sdk::complete(bearer_for(&info), model, system, user).await;
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
