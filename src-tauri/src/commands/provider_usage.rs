//! Account-wide usage limits for each AI tool, for Agent Desk's limit bars.
//!
//! The reading, caching and backoff live in `ai::provider_usage`; this is only
//! the IPC surface. It never fails: a tool whose limits cannot be read comes
//! back with a plain `unavailable_reason` instead, so one broken source does
//! not hide the others.

use crate::ai::provider_usage::{self, ProviderUsage};
use crate::error::AppError;

/// One entry per AI tool that looks set up on this machine.
///
/// `force` skips the minimum re-fetch intervals (for a refresh button), but
/// never the backoff after Claude's endpoint has said to slow down.
#[tauri::command]
#[specta::specta]
pub async fn agent_provider_usage(
    app: tauri::AppHandle,
    force: bool,
) -> Result<Vec<ProviderUsage>, AppError> {
    Ok(provider_usage::all(&app, force).await)
}
