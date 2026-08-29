//! Tauri commands for the `snip` integration.
//!
//! Thin wrappers: the work lives in `crate::snip`. Both spawn a process, so
//! both run on the blocking pool -- a synchronous command would hold the IPC
//! thread for as long as the CLI took to answer, and every unrelated command
//! behind it would queue.

use crate::error::AppError;
use crate::snip;

/// Whether `snip` is installed, and which version answered.
///
/// Safe to call repeatedly: a found result is cached, and a "not found" is
/// re-probed so installing the tool is noticed without restarting GitWyrm.
#[tauri::command]
#[specta::specta]
pub async fn snip_detect() -> Result<snip::SnipState, AppError> {
    tauri::async_runtime::spawn_blocking(snip::detect)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// How many tokens `snip` has saved, as it has recorded them.
///
/// Never fails for the ordinary reasons -- not installed, nothing recorded yet,
/// a build without the tracking database -- because each of those is a state
/// the UI should describe in its own words rather than an error dialog.
#[tauri::command]
#[specta::specta]
pub async fn snip_gain() -> Result<snip::SnipGainOutcome, AppError> {
    tauri::async_runtime::spawn_blocking(snip::gain)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}
