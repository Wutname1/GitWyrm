//! Which URL the updater asks for a manifest.
//!
//! The endpoint baked into tauri.conf.json is a single GitHub URL, and the
//! updater plugin's JS `check()` cannot override it -- `CheckOptions` carries
//! headers, a timeout and a proxy, but no endpoint. So the frontend used to
//! send `X-Update-Channel: beta` to a static release asset, which GitHub serves
//! identically whatever headers arrive. `/releases/latest` also skips
//! prereleases by definition, so the Beta setting could not have worked: every
//! beta user was quietly served stable builds.
//!
//! Resolving the endpoint here fixes that, because `UpdaterBuilder::endpoints`
//! does take a URL at runtime.
//!
//! Each channel is a distinct static object on the CDN rather than one endpoint
//! that branches on a header. That keeps the request a plain cached GET with no
//! Worker in the path, so a client polling every two hours costs no compute no
//! matter how many clients there are.

use crate::error::AppError;
use crate::settings::{self, UpdateChannel};
use tauri_plugin_updater::UpdaterExt;

/// Manifest URL per channel.
///
/// Stable keeps the GitHub URL it has always had. Builds already in the wild
/// point at it, and it is still published, so moving stable to the CDN buys
/// nothing and risks stranding anyone the CDN cutover missed. Beta is
/// CDN-only: betas are not GitHub releases at all, which is the point of the
/// exercise -- no tag, no prerelease entry, nothing accumulating in the repo.
const STABLE_ENDPOINT: &str =
  "https://github.com/Wutname1/GitWyrm/releases/latest/download/latest.json";
const BETA_ENDPOINT: &str = "https://cdn.gitwyrm.com/updates/beta.json";

fn endpoint_for(channel: &UpdateChannel) -> &'static str {
  match channel {
    UpdateChannel::Stable => STABLE_ENDPOINT,
    UpdateChannel::Beta => BETA_ENDPOINT,
  }
}

/// The manifest URL for the channel the user has chosen.
///
/// Exposed so the frontend can show which channel a check actually used, and
/// so a bug report says which endpoint was consulted rather than leaving us to
/// guess from the version alone.
#[tauri::command]
#[specta::specta]
pub async fn update_endpoint(app: tauri::AppHandle) -> Result<String, AppError> {
  let settings = tauri::async_runtime::spawn_blocking({
    let app = app.clone();
    move || settings::read_settings(&app)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))??;

  Ok(endpoint_for(&settings.update_channel).to_string())
}

/// Build an updater bound to the user's channel.
///
/// Both checking and installing go through this, so the two can never disagree
/// about which channel they are on -- a check that found a beta followed by an
/// install that fetched stable would silently downgrade the user.
async fn updater_for_channel(
  app: &tauri::AppHandle,
) -> Result<tauri_plugin_updater::Updater, AppError> {
  let endpoint = update_endpoint(app.clone()).await?;

  let url = endpoint
    .parse()
    .map_err(|e| AppError::Other(format!("bad update endpoint {endpoint}: {e}")))?;

  // `endpoints` here overrides the list in tauri.conf.json. It has to: the
  // plugin treats multiple configured endpoints as a failover chain, taking the
  // first that answers, so listing both channels there would hand everyone
  // whichever URL responded first rather than the one they chose.
  app
    .updater_builder()
    .endpoints(vec![url])
    .map_err(|e| AppError::Other(e.to_string()))?
    .build()
    .map_err(|e| AppError::Other(e.to_string()))
}

/// A newer version on the user's channel, or None when up to date.
///
/// Returns the version string only. Installing re-checks against the same
/// endpoint, so there is no `Update` handle for the frontend to hold or leak.
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
  let updater = updater_for_channel(&app).await?;

  // No `allow_downgrades`: the plugin only reports a version strictly newer
  // than the running one. That is what keeps a beta tester who switches back to
  // Stable from being dragged down to an older build -- they simply sit where
  // they are until stable overtakes them, instead of a 0.6.0 install trying to
  // read settings a 0.6.1-beta wrote.
  match updater.check().await {
    Ok(Some(update)) => Ok(Some(update.version.clone())),
    Ok(None) => Ok(None),
    Err(e) => Err(AppError::Other(e.to_string())),
  }
}

/// Download and install the pending update, returning the version installed.
///
/// Relaunching is left to the caller so the frontend can show "restarting"
/// before the window disappears.
///
/// This exists rather than the JS plugin's `downloadAndInstall` because that
/// path rebuilds the updater from tauri.conf.json and so would always fetch the
/// stable manifest -- a beta user would check beta, find a version, then
/// install whatever stable happened to be.
#[tauri::command]
#[specta::specta]
pub async fn install_update(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
  let updater = updater_for_channel(&app).await?;

  let update = match updater.check().await {
    Ok(Some(update)) => update,
    Ok(None) => return Ok(None),
    Err(e) => return Err(AppError::Other(e.to_string())),
  };

  let version = update.version.clone();
  update
    .download_and_install(|_, _| {}, || {})
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

  Ok(Some(version))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn each_channel_maps_to_its_own_endpoint() {
    let stable = endpoint_for(&UpdateChannel::Stable);
    let beta = endpoint_for(&UpdateChannel::Beta);
    assert_ne!(
      stable, beta,
      "stable and beta must not share an endpoint, or the channel setting does nothing"
    );
  }

  #[test]
  fn beta_endpoint_is_channel_specific() {
    // A beta pointed at /releases/latest is the bug this module exists to fix:
    // that path skips prereleases, so it can only ever serve stable.
    let beta = endpoint_for(&UpdateChannel::Beta);
    assert!(
      !beta.contains("releases/latest"),
      "beta must not resolve through /releases/latest - it skips prereleases"
    );
    assert!(beta.contains("beta"), "beta endpoint should name its channel");
  }

  #[test]
  fn endpoints_are_https() {
    // The manifest carries the signature the installer is checked against, so a
    // plaintext fetch would let a network attacker choose which build to offer.
    for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
      let url = endpoint_for(&channel);
      assert!(url.starts_with("https://"), "{url} must be https");
    }
  }

  #[test]
  fn endpoints_parse_as_urls() {
    for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
      let url = endpoint_for(&channel);
      assert!(
        url.parse::<tauri::Url>().is_ok(),
        "{url} must parse, or check_for_update fails at runtime"
      );
    }
  }
}
