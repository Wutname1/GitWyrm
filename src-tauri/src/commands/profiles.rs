//! Commands behind the Profiles screen: managing named identities, switching
//! the active one, and overriding it for a single repository.

use crate::error::AppError;
use crate::git::profiles::{self, GitIdentitySnapshot, Profile};
use crate::settings::{self, Settings};

/// Where generated profile include files live, beside settings.json.
fn config_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, AppError> {
  settings::app_data_dir(app)
}

fn load(app: &tauri::AppHandle) -> Result<Settings, AppError> {
  Ok(settings::read_settings_in(&settings::app_data_dir(app)?))
}

fn store(app: &tauri::AppHandle, settings: &Settings) -> Result<(), AppError> {
  settings::write_settings_in(&settings::app_data_dir(app)?, settings)
}

/// Every profile the user has defined.
#[tauri::command]
#[specta::specta]
pub async fn list_profiles(app: tauri::AppHandle) -> Result<Vec<Profile>, AppError> {
  Ok(load(&app)?.profiles)
}

/// The id of the profile currently written to the global git config.
#[tauri::command]
#[specta::specta]
pub async fn get_active_profile_id(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
  Ok(load(&app)?.active_profile_id)
}

/// Create or update a profile, then refresh the folder rules it owns.
#[tauri::command]
#[specta::specta]
pub async fn save_profile(app: tauri::AppHandle, profile: Profile) -> Result<Vec<Profile>, AppError> {
  if profile.label.trim().is_empty() {
    return Err(AppError::Other("Give this profile a name you'll recognize.".into()));
  }
  if !profile.is_complete() {
    return Err(AppError::Other(
      "A profile needs the name and email to put on your commits.".into(),
    ));
  }

  let mut settings = load(&app)?;
  match settings.profiles.iter_mut().find(|p| p.id == profile.id) {
    Some(existing) => *existing = profile.clone(),
    None => settings.profiles.push(profile.clone()),
  }

  let dir = config_dir(&app)?;
  profiles::write_folder_rules(&settings.profiles, &dir)?;

  // Editing the profile that is currently active has to reach the global
  // config too, or the change appears to save but commits keep the old values.
  if settings.active_profile_id.as_deref() == Some(profile.id.as_str()) {
    profiles::apply_globally(&profile)?;
  }

  store(&app, &settings)?;
  Ok(settings.profiles)
}

/// Remove a profile. Repositories overridden to it keep their local config,
/// which stays valid git even with the profile gone.
#[tauri::command]
#[specta::specta]
pub async fn delete_profile(app: tauri::AppHandle, id: String) -> Result<Vec<Profile>, AppError> {
  let mut settings = load(&app)?;
  settings.profiles.retain(|p| p.id != id);
  if settings.active_profile_id.as_deref() == Some(id.as_str()) {
    settings.active_profile_id = None;
  }

  let dir = config_dir(&app)?;
  // Drops the deleted profile's include file and its rules from the global
  // config; leaving them would keep applying an identity the user deleted.
  profiles::write_folder_rules(&settings.profiles, &dir)?;
  let stale = profiles::include_file_path(&dir, &id);
  let _ = std::fs::remove_file(stale);

  store(&app, &settings)?;
  Ok(settings.profiles)
}

/// Make a profile the global default. This is the switcher.
#[tauri::command]
#[specta::specta]
pub async fn set_active_profile(app: tauri::AppHandle, id: String) -> Result<(), AppError> {
  let mut settings = load(&app)?;
  let profile = settings
    .profiles
    .iter()
    .find(|p| p.id == id)
    .cloned()
    .ok_or_else(|| AppError::Other("That profile no longer exists.".into()))?;

  profiles::apply_globally(&profile)?;
  settings.active_profile_id = Some(id);
  store(&app, &settings)?;
  Ok(())
}

/// Pin one repository to a profile, whatever the active profile is.
#[tauri::command]
#[specta::specta]
pub async fn set_repo_profile(
  app: tauri::AppHandle,
  repo_path: String,
  id: String,
) -> Result<(), AppError> {
  let settings = load(&app)?;
  let profile = settings
    .profiles
    .iter()
    .find(|p| p.id == id)
    .ok_or_else(|| AppError::Other("That profile no longer exists.".into()))?;

  profiles::apply_to_repo(&repo_path, profile)
}

/// Drop a repository's override so it follows the active profile again.
#[tauri::command]
#[specta::specta]
pub async fn clear_repo_profile(repo_path: String) -> Result<(), AppError> {
  profiles::clear_repo_override(&repo_path)
}

/// Adopt the user's existing git setup as their first profile.
///
/// For someone who has been using git for years, profiles should start already
/// describing them rather than empty. Deliberately a pure read of git config:
/// their setup is already correct, and rewriting `~/.gitconfig` to "adopt" it
/// would be a side effect they never asked for. The profile is recorded as
/// active because it already describes what git does.
///
/// Runs at most once. A user who deletes the seeded profile should not find it
/// back on the next launch.
#[tauri::command]
#[specta::specta]
pub async fn seed_profile_from_config(app: tauri::AppHandle) -> Result<Option<Profile>, AppError> {
  let mut settings = load(&app)?;
  let seeded = profile_from_current_config().await?;
  if !profiles::should_seed(settings.profiles_seeded, settings.profiles.len(), &seeded) {
    return Ok(None);
  }
  log::info!("adopted the existing git identity as a first profile");

  settings.active_profile_id = Some(seeded.id.clone());
  settings.profiles.push(seeded.clone());
  settings.profiles_seeded = true;
  store(&app, &settings)?;
  Ok(Some(seeded))
}

/// Record a profile without touching git config.
///
/// Used by onboarding, which has already written the identity to git itself.
/// Applying it again would be redundant, and would fight the wizard's own
/// signing setup.
#[tauri::command]
#[specta::specta]
pub async fn add_profile_without_applying(
  app: tauri::AppHandle,
  profile: Profile,
) -> Result<(), AppError> {
  let mut settings = load(&app)?;
  settings.profiles.retain(|p| p.id != profile.id);
  settings.active_profile_id = Some(profile.id.clone());
  settings.profiles.push(profile);
  settings.profiles_seeded = true;
  store(&app, &settings)?;
  Ok(())
}

/// Build a first profile from the git config the user already has.
///
/// Adopting profiles should not mean retyping what git already knows, and it
/// must not change any behavior on its own: this only proposes a profile from
/// the current global values. Nothing is written until the user saves it.
#[tauri::command]
#[specta::specta]
pub async fn profile_from_current_config() -> Result<Profile, AppError> {
  let identity = crate::git::identity::read_identity();

  // Read the key from the global scope specifically. The effective value could
  // come from a repository we happen to be in, which is not "my default".
  let global = |key: &str| -> Option<String> {
    crate::git::shell::run_git(None, &["config", "--global", "--get", key])
      .ok()
      .map(|out| out.stdout.trim().to_string())
      .filter(|v| !v.is_empty())
  };

  let format = global("gpg.format").unwrap_or_else(|| "openpgp".into());
  let signing = match global("user.signingkey") {
    Some(key) if format.eq_ignore_ascii_case("ssh") => {
      crate::git::profiles::SigningMethod::Ssh(key)
    }
    Some(key) => crate::git::profiles::SigningMethod::Gpg(key),
    None => crate::git::profiles::SigningMethod::None,
  };

  Ok(Profile {
    // The frontend replaces this when saving a brand-new profile; a timestamp
    // keeps it unique if it ever is saved as-is.
    id: format!(
      "p{}",
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
    ),
    label: "My identity".into(),
    name: identity.name,
    email: identity.email,
    signing,
    sign_commits: global("commit.gpgsign")
      .map(|v| v.eq_ignore_ascii_case("true"))
      .unwrap_or(false),
    folders: Vec::new(),
  })
}

/// Every gpg signing key on this machine.
///
/// Separate from `get_signing_status`, which needs a repository: profiles are
/// global, and the user may be picking a key with no repository open.
#[tauri::command]
#[specta::specta]
pub async fn list_signing_keys() -> Result<Vec<crate::git::signing::SigningKey>, AppError> {
  tauri::async_runtime::spawn_blocking(crate::git::signing::list_secret_keys)
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// What git actually resolves for this repository right now.
///
/// Read from git rather than inferred from our settings, so a hand-edited
/// config or an include we did not write still shows the truth.
#[tauri::command]
#[specta::specta]
pub async fn get_effective_identity(repo_path: String) -> Result<GitIdentitySnapshot, AppError> {
  Ok(profiles::effective_identity(&repo_path))
}
