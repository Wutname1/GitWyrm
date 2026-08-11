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
use serde::Serialize;
use tauri::Emitter;
use tauri_plugin_updater::UpdaterExt;

/// Event name carrying download progress to the splash.
pub const UPDATE_PROGRESS_EVENT: &str = "update://progress";

/// How far the download has got.
///
/// `total` is None when the server sends no Content-Length, which is why the
/// splash has to cope with an unknown total rather than assuming a percentage
/// is always available.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UpdateProgress {
  /// Bytes written so far.
  pub downloaded: u64,
  /// Total bytes expected, when the server declared one.
  pub total: Option<u64>,
}

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
  let builder = app
    .updater_builder()
    .endpoints(vec![url])
    .map_err(|e| AppError::Other(e.to_string()))?;

  // Raise the cover window in the last moment we control. The updater calls
  // this immediately before `std::process::exit(0)`, which is the only hook
  // that runs late enough to be sure the install is really happening and early
  // enough to still be alive to spawn anything.
  #[cfg(windows)]
  let builder = {
    let app = app.clone();
    builder.on_before_exit(move || {
      if let Err(e) = spawn_update_cover(&app) {
        // Non-fatal: the update still installs, just without the cover.
        log::warn!("update cover window did not start: {e}");
      }
    })
  };

  builder.build().map_err(|e| AppError::Other(e.to_string()))
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

/// Event name carrying toolset download progress.
pub const TOOLSET_PROGRESS_EVENT: &str = "toolset://progress";

/// State of the git/gpg toolset, for the frontend to show and act on.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolsetStatus {
  /// Version unpacked on disk, if any.
  pub installed: Option<String>,
  /// Version the CDN is serving, when it could be reached.
  pub available: Option<String>,
  /// Whether a download would change anything.
  pub update_available: bool,
}

/// What the toolset looks like right now, and whether it is behind.
///
/// Reaching the CDN can fail (offline, blocked); that is reported as "no
/// available version" rather than an error, because a stale-but-working toolset
/// is not a problem the user needs to be told about.
#[tauri::command]
#[specta::specta]
pub async fn toolset_status() -> Result<ToolsetStatus, AppError> {
  let installed = crate::git::toolset::installed_version();

  match crate::git::toolset_fetch::needs_update().await {
    Ok(Some(manifest)) => Ok(ToolsetStatus {
      installed,
      available: Some(manifest.version),
      update_available: true,
    }),
    Ok(None) => Ok(ToolsetStatus {
      available: installed.clone(),
      installed,
      update_available: false,
    }),
    Err(e) => {
      log::warn!("could not check the toolset manifest: {e}");
      Ok(ToolsetStatus {
        installed,
        available: None,
        update_available: false,
      })
    }
  }
}

/// Download and unpack the current toolset, reporting progress as it goes.
///
/// Returns the version installed, or None when it was already current.
#[tauri::command]
#[specta::specta]
pub async fn install_toolset(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
  let Some(manifest) = crate::git::toolset_fetch::needs_update().await? else {
    return Ok(None);
  };

  let version = manifest.version.clone();
  let progress_app = app.clone();
  let mut last_emit: u64 = 0;

  crate::git::toolset_fetch::install(&manifest, move |downloaded, total| {
    // Same coalescing rationale as the app updater: one IPC message per chunk
    // would cost more than the download.
    let complete = downloaded >= total;
    if downloaded - last_emit < PROGRESS_EMIT_BYTES && !complete {
      return;
    }
    last_emit = downloaded;
    let _ = progress_app.emit(
      TOOLSET_PROGRESS_EVENT,
      UpdateProgress {
        downloaded,
        total: Some(total),
      },
    );
  })
  .await?;

  Ok(Some(version))
}

/// Filename of the bundled update-cover helper, under the resources dir.
#[cfg(windows)]
const HELPER_EXE: &str = "gitwyrm-setup.exe";

/// Show the update-cover window, and report whether it started.
///
/// The gap this covers is the one between our process exiting and the updated
/// app reappearing: NSIS runs with `installMode: "quiet"`, so without this there
/// is simply nothing on screen for 20-40 seconds.
///
/// Two details are load-bearing:
///
/// - **Run from a temp copy.** NSIS is about to rewrite the install directory,
///   and a helper running from inside it would be locked or replaced mid-update.
/// - **Detached.** Our process is killed by `std::process::exit(0)` inside the
///   updater moments from now; a child sharing our console or job would go with
///   it, which is exactly when the cover is needed.
///
/// Failure here is deliberately non-fatal: a missing helper means the update
/// proceeds with the old blank gap, which is worse-looking but still correct.
#[cfg(windows)]
fn spawn_update_cover(app: &tauri::AppHandle) -> Result<(), String> {
  use std::os::windows::process::CommandExt;
  use tauri::Manager;

  // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: no inherited console, and no
  // console-close signal following us down when this process exits.
  const DETACHED_PROCESS: u32 = 0x0000_0008;
  const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

  let source = app
    .path()
    .resource_dir()
    .map_err(|e| format!("no resource dir: {e}"))?
    .join("resources")
    .join(HELPER_EXE);

  if !source.is_file() {
    return Err(format!("helper missing at {}", source.display()));
  }

  // Name the copy per-process so two updates racing cannot fight over one
  // file, and so a stale copy left by a killed run is never reused.
  let dest = std::env::temp_dir().join(format!("gitwyrm-update-{}.exe", std::process::id()));

  std::fs::copy(&source, &dest)
    .map_err(|e| format!("could not stage helper at {}: {e}", dest.display()))?;

  std::process::Command::new(&dest)
    .arg("--updating")
    .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
    .spawn()
    .map_err(|e| format!("could not start helper: {e}"))?;

  log::info!("update cover window started from {}", dest.display());
  Ok(())
}

/// Only emit once this many bytes have arrived since the last event.
///
/// `on_chunk` fires per HTTP chunk -- thousands of times across an installer --
/// and every emit crosses the IPC boundary to the webview. Coalescing to 256 KB
/// keeps the bar smooth (a hundred-odd updates over a typical installer) without
/// making the download compete with its own progress reporting.
const PROGRESS_EMIT_BYTES: u64 = 256 * 1024;

/// Download and install the pending update.
///
/// **This does not return on success.** The updater's Windows install path ends
/// in `std::process::exit(0)` after handing the installer to ShellExecute, so
/// the process is gone before this function's caller resumes. Anything that must
/// happen before the app dies belongs in the `on_before_exit` hook below, not
/// after the await in the frontend.
///
/// Progress is reported on `UPDATE_PROGRESS_EVENT` as the download runs, and the
/// event's absence afterwards is what tells the frontend the install phase has
/// begun.
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

  let mut downloaded: u64 = 0;
  let mut last_emit: u64 = 0;
  let progress_app = app.clone();

  let on_chunk = move |chunk: usize, total: Option<u64>| {
    // `on_chunk` hands us the size of *this* chunk, not a running total.
    downloaded = downloaded.saturating_add(chunk as u64);

    // Always emit the final byte so the bar lands on 100% rather than
    // stopping wherever the last threshold fell.
    let complete = total.is_some_and(|t| downloaded >= t);
    if downloaded - last_emit < PROGRESS_EMIT_BYTES && !complete {
      return;
    }
    last_emit = downloaded;

    let _ = progress_app.emit(UPDATE_PROGRESS_EVENT, UpdateProgress { downloaded, total });
  };

  update
    .download_and_install(on_chunk, || {})
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

  // Only reached if the platform's install path returns rather than exiting.
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
