//! Commands behind the Security settings screen: reporting the signing setup,
//! making a key, and turning signing on or off.

use crate::error::AppError;
use crate::git::bundled::ToolSource;
use crate::git::identity::{self, GitIdentity};
use crate::git::signing::{self, SigningStatus};
use crate::git::ssh;

/// The name and email git puts on commits. Empty fields mean git has not been
/// set up yet, which the UI treats as "ask for it" rather than an error.
#[tauri::command]
#[specta::specta]
pub async fn get_git_identity() -> Result<GitIdentity, AppError> {
  let identity = identity::read_identity();
  if !identity.is_complete() {
    // Worth a log line: this is why a first commit gets refused, and it is the
    // single most likely thing to be wrong on a fresh machine.
    log::info!("git identity is incomplete; commits will be refused until it is set");
  }
  Ok(identity)
}

#[tauri::command]
#[specta::specta]
pub async fn set_git_identity(name: String, email: String) -> Result<(), AppError> {
  identity::write_identity(&name, &email)
}

/// Which git and gpg the app resolved, and where each came from. Drives the
/// "using the copy that came with GitWyrm" vs "using your own" line in Settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
  pub program: String,
  pub source: ToolSource,
  pub version: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn git_tool_info() -> Result<ToolInfo, AppError> {
  let program = crate::git::shell::git_program_name();
  let source = crate::git::shell::git_source();
  let version = crate::git::shell::run_git(None, &["--version"])
    .ok()
    .map(|out| out.stdout.trim().to_owned());

  Ok(ToolInfo {
    program,
    source,
    version,
  })
}

#[tauri::command]
#[specta::specta]
pub async fn gpg_tool_info() -> Result<ToolInfo, AppError> {
  let program = signing::gpg_program_name();
  let source = signing::gpg_source();
  let version = signing::run_gpg(&["--version"])
    .ok()
    .and_then(|out| out.lines().next().map(str::to_owned));

  Ok(ToolInfo {
    program,
    source,
    version,
  })
}

#[tauri::command]
#[specta::specta]
pub async fn get_signing_status(repo_path: String) -> Result<SigningStatus, AppError> {
  // Cheap enough to run inline, but it does spawn gpg twice; keep it off the
  // hot path by only calling it when the Security screen is open.
  tauri::async_runtime::spawn_blocking(move || signing::signing_status(&repo_path))
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn create_signing_key(name: String, email: String) -> Result<String, AppError> {
  tauri::async_runtime::spawn_blocking(move || signing::generate_key(&name, &email))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn export_signing_key(key_id: String) -> Result<String, AppError> {
  tauri::async_runtime::spawn_blocking(move || signing::export_public_key(&key_id))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn set_signing_enabled(
  repo_path: String,
  enabled: bool,
  key_id: Option<String>,
) -> Result<(), AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    if enabled {
      let key = key_id.ok_or_else(|| {
        AppError::Other("Pick a key to sign with before turning signing on.".into())
      })?;
      signing::enable_signing(&repo_path, &key)
    } else {
      signing::disable_signing(&repo_path)
    }
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete a signing key for good. The UI confirms before calling this.
///
/// Takes the fingerprint (unique) rather than the key id (can collide), plus
/// the id so a repository configured to sign with it can be cleaned up in the
/// same step - otherwise its next commit fails with "secret key not available".
#[tauri::command]
#[specta::specta]
pub async fn delete_signing_key(
  repo_path: String,
  key_id: String,
  fingerprint: String,
) -> Result<(), AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    signing::delete_key(&fingerprint)?;
    signing::forget_key_if_configured(&repo_path, &key_id)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/* ------------------------------------------------- SSH access (push/pull) */

/// SSH keys on this computer, from `~/.ssh`.
#[tauri::command]
#[specta::specta]
pub async fn list_ssh_keys() -> Result<Vec<ssh::SshKey>, AppError> {
  tauri::async_runtime::spawn_blocking(|| ssh::list_keys())
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Make a new ed25519 SSH key in `~/.ssh`.
#[tauri::command]
#[specta::specta]
pub async fn create_ssh_key(name: String, comment: String) -> Result<ssh::SshKey, AppError> {
  tauri::async_runtime::spawn_blocking(move || ssh::generate_key(&name, &comment))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete an SSH key pair. The UI confirms first; this cannot be undone.
#[tauri::command]
#[specta::specta]
pub async fn delete_ssh_key(public_path: String) -> Result<(), AppError> {
  tauri::async_runtime::spawn_blocking(move || ssh::delete_key(&public_path))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// The public half of an SSH key, for pasting into a host's settings.
#[tauri::command]
#[specta::specta]
pub async fn read_ssh_public_key(public_path: String) -> Result<String, AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    std::fs::read_to_string(&public_path)
      .map(|s| s.trim().to_owned())
      .map_err(AppError::Io)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Try to connect to a host over SSH and report what happened.
///
/// Spawns ssh once per key in the worst case, so it can take a few seconds -
/// the UI shows a spinner rather than calling this on a timer.
#[tauri::command]
#[specta::specta]
pub async fn test_ssh_host(host: String) -> Result<ssh::SshTestResult, AppError> {
  tauri::async_runtime::spawn_blocking(move || ssh::test_host(&host))
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Which key `~/.ssh/config` sends to a host, if it names one.
#[tauri::command]
#[specta::specta]
pub async fn ssh_key_for_host(host: String) -> Result<Option<String>, AppError> {
  tauri::async_runtime::spawn_blocking(move || ssh::configured_key_for(&host))
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Point a host at a key in `~/.ssh/config`, backing the file up first.
#[tauri::command]
#[specta::specta]
pub async fn set_ssh_key_for_host(
  host: String,
  key_path: String,
  stamp: String,
) -> Result<(), AppError> {
  tauri::async_runtime::spawn_blocking(move || ssh::set_key_for_host(&host, &key_path, &stamp))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn repair_signing_format(repo_path: String) -> Result<(), AppError> {
  tauri::async_runtime::spawn_blocking(move || signing::repair_gpg_format(&repo_path))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
