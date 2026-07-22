//! AI provider configuration and commit message generation. API keys go
//! into auth.json on disk and are never sent back to the webview; the
//! webview only learns which providers are configured.

use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::ai::{auth, catalog, client, copilot, models, prompt};
use crate::settings;
use crate::error::AppError;
use crate::git::shell::run_git;
use crate::state::RepoManager;

const MAX_DIFF_CHARS: usize = 60_000;

#[derive(Debug, Clone, Serialize, Type)]
pub struct AiProviderStatus {
  pub id: String,
  pub configured: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn ai_get_catalog(app: tauri::AppHandle) -> Result<Vec<catalog::CatalogProvider>, AppError> {
  catalog::get(&app).await
}

#[tauri::command]
#[specta::specta]
pub fn ai_list_configured(app: tauri::AppHandle) -> Result<Vec<AiProviderStatus>, AppError> {
  Ok(
    auth::load_all(&app)?
      .into_keys()
      .map(|id| AiProviderStatus { id, configured: true })
      .collect(),
  )
}

#[tauri::command]
#[specta::specta]
pub fn ai_set_api_key(app: tauri::AppHandle, provider: String, key: String) -> Result<(), AppError> {
  let key = key.trim().to_string();
  if key.is_empty() {
    return Err(AppError::Other("API key is empty".into()));
  }
  auth::set(&app, &provider, auth::AuthInfo::Api { key })
}

#[tauri::command]
#[specta::specta]
pub fn ai_remove_provider(app: tauri::AppHandle, provider: String) -> Result<(), AppError> {
  auth::remove(&app, &provider)
}

/// The models the user can actually use for a provider. Asks the provider's
/// `/models` endpoint when a key is configured (so Copilot reflects plan
/// entitlements and other providers reflect key access), falling back to the
/// static catalog list.
#[tauri::command]
#[specta::specta]
pub async fn ai_list_models(
  app: tauri::AppHandle,
  provider: String,
) -> Result<models::ModelList, AppError> {
  let cat = catalog::find(&app, &provider).await?;
  Ok(models::list(&app, &cat).await)
}

#[tauri::command]
#[specta::specta]
pub async fn ai_copilot_device_start() -> Result<copilot::DeviceCodeInfo, AppError> {
  copilot::device_start("read:user").await
}

/// One poll pass; the frontend loops on Pending so sign-in stays cancellable.
#[tauri::command]
#[specta::specta]
pub async fn ai_copilot_device_poll(
  app: tauri::AppHandle,
  device_code: String,
  interval: u32,
) -> Result<copilot::PollResult, AppError> {
  match copilot::device_poll(&device_code, interval).await? {
    copilot::PollOutcome::Token(token) => {
      auth::set(
        &app,
        "github-copilot",
        auth::AuthInfo::Oauth {
          refresh: token.clone(),
          access: token,
          expires: 0,
          enterprise_url: None,
        },
      )?;
      Ok(copilot::PollResult::Complete)
    }
    copilot::PollOutcome::Pending { interval } => Ok(copilot::PollResult::Pending { interval }),
  }
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct GeneratedCommitMessage {
  pub summary: String,
  pub description: String,
}

/// The built-in commit instruction, exposed so the settings UI can show it as
/// the placeholder and restore it with "Reset to default".
#[tauri::command]
#[specta::specta]
pub fn ai_default_instruction() -> String {
  prompt::default_instruction()
}

fn truncate_diff(diff: &str) -> String {
  if diff.len() <= MAX_DIFF_CHARS {
    return diff.to_string();
  }
  let cut: String = diff.chars().take(MAX_DIFF_CHARS).collect();
  format!("{cut}\n\n[diff truncated: staged changes exceed the size limit]")
}

fn split_message(text: &str) -> GeneratedCommitMessage {
  let text = text
    .trim()
    .trim_start_matches("```")
    .trim_end_matches("```")
    .trim();
  let mut lines = text.lines();
  let summary = lines.next().unwrap_or_default().trim().to_string();
  let description = lines.collect::<Vec<_>>().join("\n").trim().to_string();
  GeneratedCommitMessage { summary, description }
}

fn bearer_for(info: &auth::AuthInfo) -> &str {
  match info {
    auth::AuthInfo::Api { key } => key,
    auth::AuthInfo::Oauth { refresh, .. } => refresh,
  }
}

#[tauri::command]
#[specta::specta]
pub async fn generate_commit_message(
  app: tauri::AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  provider: String,
  model: String,
) -> Result<GeneratedCommitMessage, AppError> {
  let open = manager.get(&repo_id)?;
  let repo_path = open.path.to_string_lossy().into_owned();

  let info = auth::get(&app, &provider)?
    .ok_or_else(|| AppError::Other(format!("no API key configured for {provider}")))?;
  let cat = catalog::find(&app, &provider).await?;

  // The user's editable guidance (empty falls back to the default), always
  // combined with the fixed format contract our parser depends on.
  let user_instruction = settings::get_settings(app.clone())?
    .ai_instruction
    .unwrap_or_default();
  let system = prompt::build_system(&user_instruction);

  let (diff, log) = tauri::async_runtime::spawn_blocking(move || {
    let diff = run_git(Some(&repo_path), &["diff", "--cached", "--no-color"])?.stdout;
    let log = run_git(Some(&repo_path), &["log", "--oneline", "--no-decorate", "-10"])
      .map(|o| o.stdout)
      .unwrap_or_default();
    Ok::<_, AppError>((diff, log))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))??;

  if diff.trim().is_empty() {
    return Err(AppError::Other("nothing staged to describe".into()));
  }

  let user = format!(
    "Recent commit subjects:\n{}\n\nStaged diff:\n{}",
    if log.trim().is_empty() { "(none)" } else { log.trim() },
    truncate_diff(&diff)
  );

  let text = client::chat(client::ChatRequest {
    provider: &cat,
    bearer: bearer_for(&info),
    model: &model,
    system: &system,
    user: &user,
    max_tokens: 1024,
    timeout: client::DEFAULT_TIMEOUT,
  })
  .await?;

  Ok(split_message(&text))
}
