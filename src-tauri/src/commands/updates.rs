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
use tauri::{Emitter, Manager};
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

  // Backstop only -- `install_update` raises the cover before it starts, and
  // `spawn_update_cover` is idempotent, so this fires for real only if that
  // earlier attempt failed. Keeping it means a cover that could not be staged
  // while the app was busy still gets one last chance at the moment of exit.
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

/// One line of a release's changelog.
///
/// Mirrors the website's stored shape. `section` is the commit-prefix category
/// (`feature`, `fix`, `change`, `docs`, `breaking`) and `tags` are the explicit
/// `[tag]`/`#tag` markers the commit author wrote, which the UI renders as
/// chips. Both arrive already parsed, so nothing here re-derives them from
/// markdown.
#[derive(Debug, Clone, Serialize, serde::Deserialize, specta::Type)]
pub struct ChangelogItem {
  pub section: String,
  pub text: String,
  #[serde(default)]
  pub tags: Vec<String>,
}

/// One release, with its notes.
#[derive(Debug, Clone, Serialize, serde::Deserialize, specta::Type)]
pub struct ChangelogEntry {
  pub version: String,
  pub released_at: Option<String>,
  #[serde(default)]
  pub items: Vec<ChangelogItem>,
}

#[derive(serde::Deserialize)]
struct ChangelogResponse {
  #[serde(default)]
  entries: Vec<ChangelogEntry>,
}

/// Structured release notes, newest first.
const CHANGELOG_URL: &str = "https://gitwyrm.com/api/v1/changelogs";

/// Release notes for everything newer than the running build.
///
/// Fetched here rather than in the webview because the page's CSP would have to
/// be widened to reach gitwyrm.com, and this keeps the network surface in one
/// place.
///
/// Someone updating 0.3.0 -> 0.5.0 skipped 0.4.x entirely and never saw those
/// notes, so the filter is "newer than what is running" rather than "the target
/// release" -- the modal is the only chance they get to read them.
///
/// A failure here is not an update failure: the caller shows the update without
/// notes rather than blocking on them.
#[tauri::command]
#[specta::specta]
pub async fn changelog_since(
  current: String,
  target: String,
) -> Result<Vec<ChangelogEntry>, AppError> {
  let response = reqwest::get(CHANGELOG_URL)
    .await
    .map_err(|e| AppError::Other(format!("could not reach the changelog: {e}")))?;

  if !response.status().is_success() {
    return Err(AppError::Other(format!(
      "changelog request failed: {}",
      response.status()
    )));
  }

  let body: ChangelogResponse = response
    .json()
    .await
    .map_err(|e| AppError::Other(format!("could not read the changelog: {e}")))?;

  // What counts as "newer" depends on which channel the user is coming from.
  //
  // A beta reads its own base version as the floor and ignores prerelease
  // entries. Someone on 0.8.1-beta.3 moving to stable 0.8.1 would otherwise see
  // nothing at all: 0.8.1 is not greater than 0.8.1-beta.3 once the suffix is
  // trimmed. Comparing on the base with `>=` gives them the FULL notes for the
  // release they land on, which is what they want -- the stable entry covers
  // every commit the betas did, since its range starts at the previous stable
  // tag. Listing the betas they already ran alongside it would repeat the same
  // lines under older version numbers.
  //
  // Beta-to-beta is the exception: there is no stable entry to fall back on
  // yet, so a tester moving 0.8.1-beta.1 -> 0.8.1-beta.4 keeps the prerelease
  // entries and reads them strictly-newer, as normal.
  let on_beta = is_prerelease(&current);
  let target_is_beta = is_prerelease(&target);
  let current_v = parse_version(&current);

  let mut entries: Vec<ChangelogEntry> = body
    .entries
    .into_iter()
    .filter(|e| {
      // Prerelease notes are only ever relevant while heading to another
      // prerelease; a stable target supersedes them.
      if is_prerelease(&e.version) && !target_is_beta {
        return false;
      }

      // Landing on stable from a beta includes the release matching the beta's
      // own base version, which strict `>` would exclude.
      if on_beta && !target_is_beta {
        parse_version(&e.version) >= current_v
      } else {
        // Full ordering, so two betas of the same base compare by their
        // prerelease number instead of both collapsing to the same triple.
        parse_version_full(&e.version) > parse_version_full(&current)
      }
    })
    .collect();

  // Newest first. The API already returns them that way, but sorting here means
  // the UI does not depend on that staying true.
  entries.sort_by(|a, b| parse_version(&b.version).cmp(&parse_version(&a.version)));

  Ok(entries)
}

/// A version as comparable parts, for ordering releases.
///
/// Deliberately lenient: anything unparseable becomes 0 so a malformed entry
/// sorts to the bottom instead of failing the whole request. A prerelease
/// suffix (`0.9.0-beta.1`) is trimmed, which orders it equal to its release --
/// good enough for "is this newer than what I am running", and the updater
/// itself is what decides which build is actually offered.
/// Whether a version string carries a prerelease suffix (`0.8.1-beta.3`).
///
/// Matches on the separator rather than the word "beta", so an alpha or rc
/// build is treated the same way without needing another arm here.
fn is_prerelease(v: &str) -> bool {
  v.trim_start_matches('v').contains('-')
}

fn parse_version(v: &str) -> (u32, u32, u32) {
  let core = v.trim_start_matches('v');
  let core = core.split(['-', '+']).next().unwrap_or(core);
  let mut parts = core.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
  (
    parts.next().unwrap_or(0),
    parts.next().unwrap_or(0),
    parts.next().unwrap_or(0),
  )
}

/// A version ordered with its prerelease number, for comparing two betas.
///
/// `parse_version` deliberately trims the suffix, which makes every beta of a
/// base version compare equal -- fine for "is this newer than the release I am
/// on", useless for ordering 0.8.1-beta.1 against 0.8.1-beta.4. The fourth
/// element carries the prerelease number, with a stable release taking u32::MAX
/// so it always sorts above every beta of the same base.
fn parse_version_full(v: &str) -> (u32, u32, u32, u32) {
  let (major, minor, patch) = parse_version(v);
  let core = v.trim_start_matches('v');

  let pre = match core.split_once('-') {
    // "beta.4" -> 4. An unnumbered or unparseable suffix sorts lowest rather
    // than being promoted above numbered builds of the same base.
    Some((_, suffix)) => suffix
      .rsplit('.')
      .next()
      .and_then(|n| n.parse::<u32>().ok())
      .unwrap_or(0),
    None => u32::MAX,
  };

  (major, minor, patch, pre)
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
/// Set once the cover has been started, so it is never started twice.
///
/// Two call sites race for it: `install_update` raises the cover up front, and
/// the updater's `on_before_exit` hook is still wired as a backstop. Without
/// this the common path would spawn two identical windows stacked on each
/// other, and the second would outlive the handover the first performed.
#[cfg(windows)]
static COVER_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(windows)]
fn spawn_update_cover(app: &tauri::AppHandle) -> Result<(), String> {
  use std::os::windows::process::CommandExt;
  use std::sync::atomic::Ordering;
  use tauri::Manager;

  // `swap` rather than a load-then-store: the two call sites can in principle
  // reach here on different threads.
  if COVER_STARTED.swap(true, Ordering::SeqCst) {
    return Ok(());
  }

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

  // Release the claim on any failure below, so a cover that could not be
  // staged now is still attempted by the `on_before_exit` backstop rather than
  // being suppressed by a flag set for an attempt that never produced a window.
  let start = || -> Result<(), String> {
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
  };

  match start() {
    Ok(()) => Ok(()),
    Err(e) => {
      COVER_STARTED.store(false, Ordering::SeqCst);
      Err(e)
    }
  }
}

/// Take the app's own windows off screen, now that the cover is up.
///
/// Only cosmetic, and deliberately so: the process carries on downloading and
/// hands off to the installer exactly as before. Hiding rather than closing
/// matters -- closing the last window runs the app's exit path, which would
/// tear down the very process that still has an installer to launch.
///
/// Failures are logged and ignored. A window that refuses to hide leaves the
/// old overlap, which is the behaviour we already had.
#[cfg(windows)]
fn hide_all_windows(app: &tauri::AppHandle) {
  for (label, window) in app.webview_windows() {
    if let Err(e) = window.hide() {
      log::warn!("could not hide window {label} for the update: {e}");
    }
  }
}

/// Only emit once this many bytes have arrived since the last event.
///
/// `on_chunk` fires per HTTP chunk -- thousands of times across an installer --
/// and every emit crosses the IPC boundary to the webview. Coalescing to 256 KB
/// keeps the bar smooth (a hundred-odd updates over a typical installer) without
/// making the download compete with its own progress reporting.
const PROGRESS_EMIT_BYTES: u64 = 256 * 1024;

/// An update downloaded and signature-checked, waiting to be installed.
///
/// The installer bytes live here between `download_update` and
/// `install_downloaded_update` so the user can read the changelog, or leave the
/// modal open, without the download being thrown away. Re-downloading ~100 MB
/// because they took a minute to decide would be worse than holding it.
///
/// Dropped on install, and never persisted -- a restart re-checks from scratch.
#[derive(Default)]
pub struct PendingUpdate(pub std::sync::Mutex<Option<PendingUpdateInner>>);

pub struct PendingUpdateInner {
  version: String,
  bytes: Vec<u8>,
}

/// Download the pending update and hold it, without installing.
///
/// Split from `install_update` so the UI can offer "Download" and "Restart to
/// update" as two steps: someone mid-task should be able to fetch an update now
/// and choose their own moment to restart.
///
/// The signature is verified inside `download`, so bytes reaching the state
/// below have already been checked.
#[tauri::command]
#[specta::specta]
pub async fn download_update(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
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
    downloaded = downloaded.saturating_add(chunk as u64);
    let complete = total.is_some_and(|t| downloaded >= t);
    if downloaded - last_emit < PROGRESS_EMIT_BYTES && !complete {
      return;
    }
    last_emit = downloaded;
    let _ = progress_app.emit(UPDATE_PROGRESS_EVENT, UpdateProgress { downloaded, total });
  };

  let bytes = update
    .download(on_chunk, || {})
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

  {
    let state = app.state::<PendingUpdate>();
    let mut slot = state.0.lock().map_err(|e| AppError::Other(e.to_string()))?;
    *slot = Some(PendingUpdateInner {
      version: version.clone(),
      bytes,
    });
  }

  Ok(Some(version))
}

/// Install an update already fetched by `download_update`, and restart.
///
/// **This does not return on success** -- same as `install_update`, the process
/// exits inside the installer handoff. Errors if nothing has been downloaded,
/// which would mean the UI offered a restart it had no bytes for.
#[tauri::command]
#[specta::specta]
pub async fn install_downloaded_update(app: tauri::AppHandle) -> Result<(), AppError> {
  let pending = {
    let state = app.state::<PendingUpdate>();
    let mut slot = state.0.lock().map_err(|e| AppError::Other(e.to_string()))?;
    slot.take()
  };

  let Some(pending) = pending else {
    return Err(AppError::Other(
      "no update has been downloaded yet".to_string(),
    ));
  };

  // Same handover as install_update: cover the screen before the installer
  // starts, since the app is about to disappear.
  #[cfg(windows)]
  {
    if let Err(e) = spawn_update_cover(&app) {
      log::warn!("update cover window did not start: {e}");
    } else {
      hide_all_windows(&app);
    }
  }

  let updater = updater_for_channel(&app).await?;
  let update = match updater.check().await {
    Ok(Some(update)) => update,
    Ok(None) => {
      return Err(AppError::Other(
        "the update is no longer being offered".to_string(),
      ))
    }
    Err(e) => return Err(AppError::Other(e.to_string())),
  };

  log::info!("installing downloaded update {}", pending.version);

  update
    .install(pending.bytes)
    .map_err(|e| AppError::Other(e.to_string()))?;

  Ok(())
}

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

  // Hand the screen over before the install starts, not as the process dies.
  //
  // The plugin's `on_before_exit` hook sounds like the right moment but runs far
  // too late: `install_inner` writes the ~100 MB installer out to a temp file
  // (unzipping it first, when the bundle is zipped) *before* calling the hook,
  // and only then exits. So the cover appeared several seconds after the user
  // clicked, with the app sitting there fully interactive in the meantime -- the
  // 5-10s of apparently-nothing-happening that this replaces.
  //
  // Raising the cover here and hiding our own window in the same breath makes
  // the swap immediate. The hook stays wired as a backstop; the flag inside
  // `spawn_update_cover` keeps it from producing a second window.
  #[cfg(windows)]
  {
    if let Err(e) = spawn_update_cover(&app) {
      log::warn!("update cover window did not start: {e}");
    } else {
      hide_all_windows(&app);
    }
  }

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
  fn versions_order_numerically_not_lexically() {
    // The bug this guards: as strings, "0.10.0" sorts BEFORE "0.9.0", which
    // would hide the newest release's notes exactly when they matter most.
    assert!(parse_version("0.10.0") > parse_version("0.9.0"));
    assert!(parse_version("1.0.0") > parse_version("0.99.99"));
    assert!(parse_version("0.5.0") > parse_version("0.4.1"));
  }

  #[test]
  fn a_leading_v_and_prerelease_suffix_are_ignored() {
    assert_eq!(parse_version("v0.8.0"), parse_version("0.8.0"));
    assert_eq!(parse_version("0.9.0-beta.1"), parse_version("0.9.0"));
  }

  #[test]
  fn an_unparseable_version_does_not_panic() {
    // A malformed entry must sort harmlessly rather than fail the request.
    assert_eq!(parse_version(""), (0, 0, 0));
    assert_eq!(parse_version("not-a-version"), (0, 0, 0));
  }

  #[test]
  fn a_skipped_release_still_counts_as_newer() {
    // Someone on 0.3.0 going to 0.5.0 must be offered 0.4.x notes too --
    // otherwise the intermediate releases are never read by anyone.
    let current = parse_version("0.3.0");
    for skipped in ["0.4.0", "0.4.1", "0.5.0"] {
      assert!(parse_version(skipped) > current, "{skipped} should be newer");
    }
    // The running version itself is not "newer", so it never appears.
    assert!(parse_version("0.3.0") <= current);
  }

  /// Mirrors the filter inside `changelog_since`, so the rules can be checked
  /// without a network call. Kept next to it deliberately: if one changes and
  /// the other does not, these tests stop describing real behaviour.
  fn visible(current: &str, target: &str, available: &[&str]) -> Vec<String> {
    let on_beta = is_prerelease(current);
    let target_is_beta = is_prerelease(target);
    let current_v = parse_version(current);

    available
      .iter()
      .filter(|v| {
        if is_prerelease(v) && !target_is_beta {
          return false;
        }
        if on_beta && !target_is_beta {
          parse_version(v) >= current_v
        } else {
          parse_version_full(v) > parse_version_full(current)
        }
      })
      .map(|v| v.to_string())
      .collect()
  }

  #[test]
  fn a_beta_landing_on_its_own_stable_sees_the_full_release() {
    // The case that motivated this: 0.8.1 is NOT > 0.8.1-beta.3 once the
    // suffix is trimmed, so strict comparison showed the user nothing at all.
    let seen = visible("0.8.1-beta.3", "0.8.1", &["0.8.1", "0.8.0"]);
    assert_eq!(seen, vec!["0.8.1"], "the release being installed must appear");
  }

  #[test]
  fn a_beta_jumping_past_its_base_sees_every_release_in_between() {
    let seen = visible("0.8.1-beta.3", "0.9.0", &["0.9.0", "0.8.1", "0.8.0"]);
    assert_eq!(seen, vec!["0.9.0", "0.8.1"]);
    assert!(!seen.contains(&"0.8.0".to_string()), "0.8.0 predates the beta");
  }

  #[test]
  fn prerelease_notes_are_hidden_when_landing_on_stable() {
    // The betas already run would otherwise repeat the release's own lines
    // under older version numbers.
    let seen = visible("0.8.1-beta.1", "0.8.1", &["0.8.1", "0.8.1-beta.3", "0.8.1-beta.2"]);
    assert_eq!(seen, vec!["0.8.1"]);
  }

  #[test]
  fn beta_to_beta_keeps_the_prerelease_notes() {
    // No stable entry exists yet, so these are the only notes there are.
    let seen = visible("0.8.1-beta.1", "0.8.1-beta.4", &["0.8.1-beta.4", "0.8.1-beta.2", "0.8.0"]);
    assert_eq!(seen, vec!["0.8.1-beta.4", "0.8.1-beta.2"]);
  }

  #[test]
  fn a_stable_user_is_unaffected_by_the_beta_rules() {
    // Strict `>`, and prereleases never shown.
    let seen = visible("0.8.0", "0.9.0", &["0.9.0", "0.8.1", "0.8.1-beta.2", "0.8.0"]);
    assert_eq!(seen, vec!["0.9.0", "0.8.1"]);
  }

  #[test]
  fn a_stable_release_outranks_every_beta_of_the_same_base() {
    assert!(parse_version_full("0.8.1") > parse_version_full("0.8.1-beta.9"));
    assert!(parse_version_full("0.8.1-beta.4") > parse_version_full("0.8.1-beta.3"));
    // Double digits must not sort as text, where "10" < "9".
    assert!(parse_version_full("0.8.1-beta.10") > parse_version_full("0.8.1-beta.9"));
  }

  #[test]
  fn prerelease_detection_covers_alpha_and_rc() {
    assert!(is_prerelease("0.8.1-beta.3"));
    assert!(is_prerelease("0.8.1-alpha.1"));
    assert!(is_prerelease("v0.8.1-rc.2"));
    assert!(!is_prerelease("0.8.1"));
    assert!(!is_prerelease("v0.8.1"));
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
