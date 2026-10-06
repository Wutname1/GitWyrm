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

/// What happened when an install was attempted.
///
/// A plain error is the wrong shape for the Linux package case. Installing a
/// .deb or .rpm needs root, which the updater asks for via pkexec, then a
/// graphical sudo, then a terminal sudo. When all three are unavailable or the
/// user dismisses the prompt, nothing is broken and nothing is wrong with the
/// download - the app simply cannot install itself, and the honest answer is to
/// point at the download page rather than show the user a dpkg error.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallOutcome {
    /// No update was on offer.
    UpToDate,
    /// The install is underway; on most platforms the process exits inside it.
    Installing { version: String },
    /// A newer version exists but this install cannot apply it itself. The user
    /// has to download it. Not an error: there is nothing for them to retry.
    ManualRequired { version: String, url: String },
}

/// Whether this installation can replace itself with the updater artifact.
///
/// AppImages are single writable files and Tauri can update them directly.
/// A deb/rpm installation belongs to the system package manager instead; the
/// app should announce the release and show package-manager instructions
/// without downloading an AppImage or asking for root access.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateInstallMode {
    SelfUpdate,
    SystemPackage,
}

#[tauri::command]
#[specta::specta]
pub fn update_install_mode() -> UpdateInstallMode {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if std::env::var_os("APPIMAGE").is_none() {
            return UpdateInstallMode::SystemPackage;
        }
    }

    UpdateInstallMode::SelfUpdate
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

/// How long to wait for the update server to accept a connection.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// How long a download may go without a single byte arriving before it counts
/// as stalled.
///
/// An idle limit, not a total one: a 60 MB installer over a slow link can
/// legitimately take minutes, but a connection that has been silent this long is
/// not coming back. The plugin builds its client with no timeout at all, so
/// before this a stalled connection hung the launch update indefinitely, with
/// the splash (or the update cover) left on screen and nothing in the log.
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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
        .map_err(|e| AppError::Other(e.to_string()))?
        .configure_client(|client| {
            client
                .connect_timeout(CONNECT_TIMEOUT)
                .read_timeout(READ_TIMEOUT)
        });

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

/// Structured release notes, newest first. The API also holds other products'
/// releases, so the product is named explicitly.
const CHANGELOG_URL: &str = "https://gitwyrm.com/api/v1/changelogs?product=GitWyrm";

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

/// Prefix of the per-process temp copies of the helper. The stale sweep matches
/// on it, so the name it stages and the name it deletes cannot drift apart.
#[cfg(windows)]
const HELPER_COPY_PREFIX: &str = "gitwyrm-update-";

/// The update cover, while one is up.
///
/// Held so an install that fails can take the cover down again. The helper is
/// detached, so without this it sat on "Preparing update" for its full
/// ten-minute watch, over an app that had given up and was running invisibly
/// behind it.
#[cfg(windows)]
struct Cover {
    child: std::process::Child,
    exe: std::path::PathBuf,
    /// Windows that were on screen when the cover went up, so exactly those
    /// come back if the update does not happen.
    hidden: Vec<String>,
}

/// The cover, once started, so it is never started twice.
///
/// Two call sites race for it: the install commands raise the cover just before
/// installing, and the updater's `on_before_exit` hook is still wired as a
/// backstop. Without this the common path would spawn two identical windows
/// stacked on each other, and the second would outlive the handover the first
/// performed.
#[cfg(windows)]
static COVER: std::sync::Mutex<Option<Cover>> = std::sync::Mutex::new(None);

/// The cover slot. A panic elsewhere while it was held leaves nothing
/// half-written worth refusing over, so a poisoned lock is simply taken.
#[cfg(windows)]
fn cover_slot() -> std::sync::MutexGuard<'static, Option<Cover>> {
    COVER.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Start the update-cover window, unless it is already up.
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

    // Held across the spawn, so the two call sites cannot both get past the
    // check. A failed attempt leaves the slot empty, so the `on_before_exit`
    // backstop still gets its chance.
    let mut slot = cover_slot();
    if slot.is_some() {
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

    if !source.is_file() {
        return Err(format!("helper missing at {}", source.display()));
    }

    // Name the copy per-process so two updates racing cannot fight over one
    // file, and so a stale copy left by a killed run is never reused.
    let exe = std::env::temp_dir().join(format!(
        "{HELPER_COPY_PREFIX}{}.exe",
        std::process::id()
    ));

    std::fs::copy(&source, &exe)
        .map_err(|e| format!("could not stage helper at {}: {e}", exe.display()))?;

    let child = std::process::Command::new(&exe)
        .arg("--updating")
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map_err(|e| format!("could not start helper: {e}"))?;

    log::info!("update cover window started from {}", exe.display());
    *slot = Some(Cover {
        child,
        exe,
        hidden: Vec::new(),
    });
    Ok(())
}

/// Put the update cover up and take the app's own windows off screen.
///
/// Called once the installer is downloaded and verified, immediately before it
/// is handed over. Raising it any earlier puts the cover over the download: the
/// splash's progress line is hidden behind a card that only says "Preparing
/// update", and a slow or stalled download looks like the update has hung.
///
/// Hiding rather than closing matters -- closing the last window runs the app's
/// exit path, which would tear down the process that still has an installer to
/// launch. A cover that fails to start leaves the windows alone: the update
/// still installs, just with the old blank gap.
#[cfg(windows)]
fn raise_update_cover(app: &tauri::AppHandle) {
    if let Err(e) = spawn_update_cover(app) {
        log::warn!("update cover window did not start: {e}");
        return;
    }

    let mut hidden = Vec::new();
    for (label, window) in app.webview_windows() {
        // Assume visible if we cannot tell: hiding one extra window is the old
        // behaviour, while skipping one would leave it over the cover.
        let was_visible = window.is_visible().unwrap_or(true);
        match window.hide() {
            Ok(()) if was_visible => hidden.push(label),
            Ok(()) => {}
            Err(e) => log::warn!("could not hide window {label} for the update: {e}"),
        }
    }

    if let Some(cover) = cover_slot().as_mut() {
        cover.hidden = hidden;
    }
}

/// Undo `raise_update_cover` after an install that did not go ahead.
///
/// The install call only returns on failure, so reaching this means the app is
/// staying: close the cover and bring the hidden windows back, so the person is
/// looking at their app and the error rather than a card waiting for a restart
/// that will never come.
#[cfg(windows)]
fn lower_update_cover(app: &tauri::AppHandle) {
    let Some(mut cover) = cover_slot().take() else {
        return;
    };

    if let Err(e) = cover.child.kill() {
        log::warn!("could not close the update cover: {e}");
    }
    // Reap it so the copy is no longer locked, then delete it now rather than
    // leaving it for the next launch's sweep.
    let _ = cover.child.wait();
    let _ = std::fs::remove_file(&cover.exe);

    for label in &cover.hidden {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        if let Err(e) = window.show() {
            log::warn!("could not show window {label} after the update failed: {e}");
            continue;
        }
        let _ = window.set_focus();
    }

    log::info!(
        "update did not install; closed the cover and restored {} window(s)",
        cover.hidden.len()
    );
}

#[cfg(not(windows))]
fn raise_update_cover(_app: &tauri::AppHandle) {}

#[cfg(not(windows))]
fn lower_update_cover(_app: &tauri::AppHandle) {}

/// Delete helper copies that earlier updates left in the temp folder.
///
/// Every update stages its own `gitwyrm-update-<pid>.exe`, and a successful one
/// never comes back to remove it: the process that made it is gone. They piled
/// up at about 3 MB per update. The copy covering the update that just relaunched
/// us may still be running when this runs, and Windows will not delete a running
/// exe; that one is left for the next launch.
#[cfg(windows)]
pub fn sweep_stale_update_helpers() {
    let removed = sweep_helper_copies(&std::env::temp_dir());
    if removed > 0 {
        log::info!("removed {removed} leftover update helper(s) from the temp folder");
    }
}

/// Delete every helper copy in `dir` that can be deleted, and count them.
#[cfg(windows)]
fn sweep_helper_copies(dir: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(is_helper_copy_name))
        .filter(|entry| std::fs::remove_file(entry.path()).is_ok())
        .count()
}

/// Whether a temp-folder file name is one of our staged helper copies.
///
/// Strict on purpose: the temp folder is shared with every other program, so
/// only the exact `gitwyrm-update-<digits>.exe` shape is ever touched.
#[cfg(windows)]
fn is_helper_copy_name(name: &str) -> bool {
    name.strip_prefix(HELPER_COPY_PREFIX)
        .and_then(|rest| rest.strip_suffix(".exe"))
        .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
}

/// Only emit once this many bytes have arrived since the last event.
///
/// `on_chunk` fires per HTTP chunk -- thousands of times across an installer --
/// and every emit crosses the IPC boundary to the webview. Coalescing to 256 KB
/// keeps the bar smooth (a hundred-odd updates over a typical installer) without
/// making the download compete with its own progress reporting.
const PROGRESS_EMIT_BYTES: u64 = 256 * 1024;

/// Shown when an update download goes quiet for `READ_TIMEOUT`.
pub(crate) const DOWNLOAD_STALLED_MESSAGE: &str =
    "The download stopped responding. Check your internet connection and try again.";

/// Shown when the update server cannot be reached at all.
pub(crate) const DOWNLOAD_UNREACHABLE_MESSAGE: &str =
    "Could not reach the update server. Check your internet connection and try again.";

/// Say what went wrong with a download in words the user can act on.
///
/// Only the two network conditions are reworded; anything else (a bad
/// signature, a server error) keeps the plugin's own text, since those are
/// faults worth reporting as they are. A connect timeout is both a connect and a
/// timeout error, and "could not reach" is the truer description of it.
fn describe_download_error(e: &tauri_plugin_updater::Error) -> String {
    if let tauri_plugin_updater::Error::Reqwest(inner) = e {
        if inner.is_connect() {
            return DOWNLOAD_UNREACHABLE_MESSAGE.to_owned();
        }
        if inner.is_timeout() {
            return DOWNLOAD_STALLED_MESSAGE.to_owned();
        }
    }
    e.to_string()
}

/// Download an update's installer, with progress events and a log trail.
///
/// Shared by both install paths. The download used to log nothing at all, so
/// the only record of an update that hung was a log that stopped.
async fn download_installer(
    app: &tauri::AppHandle,
    update: &tauri_plugin_updater::Update,
) -> Result<Vec<u8>, AppError> {
    use std::sync::atomic::{AtomicU64, Ordering};

    log::info!(
        "update {}: downloading from {}",
        update.version,
        update.download_url.host_str().unwrap_or("unknown host")
    );

    let started = std::time::Instant::now();
    // Outside the closure so a failure can say how far the download got.
    let received = AtomicU64::new(0);
    let mut last_emit: u64 = 0;

    let on_chunk = |chunk: usize, total: Option<u64>| {
        // `on_chunk` hands us the size of *this* chunk, not a running total.
        let downloaded = received.fetch_add(chunk as u64, Ordering::Relaxed) + chunk as u64;

        // Always emit the final byte so the bar lands on 100% rather than
        // stopping wherever the last threshold fell.
        let complete = total.is_some_and(|t| downloaded >= t);
        if downloaded - last_emit < PROGRESS_EMIT_BYTES && !complete {
            return;
        }
        last_emit = downloaded;

        let _ = app.emit(UPDATE_PROGRESS_EVENT, UpdateProgress { downloaded, total });
    };

    let result = update.download(on_chunk, || {}).await;

    let mb = received.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let secs = started.elapsed().as_secs_f64();
    match result {
        Ok(bytes) => {
            log::info!("update {}: downloaded {mb:.1} MB in {secs:.1}s", update.version);
            Ok(bytes)
        }
        Err(e) => {
            log::warn!(
                "update {}: download failed after {mb:.1} MB in {secs:.1}s: {e}",
                update.version
            );
            Err(AppError::Other(describe_download_error(&e)))
        }
    }
}

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
    let bytes = download_installer(&app, &update).await?;

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
    let state = app.state::<PendingUpdate>();

    let has_pending = state
        .0
        .lock()
        .map_err(|e| AppError::Other(e.to_string()))?
        .is_some();
    if !has_pending {
        return Err(AppError::Other(
            "no update has been downloaded yet".to_string(),
        ));
    }

    // Re-check before taking anything off screen or out of the slot. It is a
    // network call, and a failure here must leave the app as it was, with the
    // download still held for another try.
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

    let pending = state
        .0
        .lock()
        .map_err(|e| AppError::Other(e.to_string()))?
        .take();
    let Some(pending) = pending else {
        return Err(AppError::Other(
            "no update has been downloaded yet".to_string(),
        ));
    };

    raise_update_cover(&app);
    log::info!("installing downloaded update {}", pending.version);

    // Does not return on success: the process exits inside the handoff.
    if let Err(e) = update.install(&pending.bytes) {
        log::warn!("update {}: install failed: {e}", pending.version);
        lower_update_cover(&app);
        // Put the bytes back so "Restart to update" can be tried again without
        // downloading everything a second time.
        if let Ok(mut slot) = state.0.lock() {
            *slot = Some(pending);
        }
        return Err(AppError::Other(e.to_string()));
    }

    Ok(())
}

/// Download and install the pending update.
///
/// **This does not return on success.** The updater's Windows install path ends
/// in `std::process::exit(0)` after handing the installer to ShellExecute, so
/// the process is gone before this function's caller resumes. Anything that must
/// happen before the app dies belongs in the `on_before_exit` hook, not after
/// the await in the frontend.
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
pub async fn install_update(app: tauri::AppHandle) -> Result<InstallOutcome, AppError> {
    let updater = updater_for_channel(&app).await?;

    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return Ok(InstallOutcome::UpToDate),
        Err(e) => return Err(AppError::Other(e.to_string())),
    };

    let version = update.version.clone();

    // Download with the app still on screen, so the splash or the toast can
    // narrate it. A failure here leaves nothing to undo.
    let bytes = download_installer(&app, &update).await?;

    // Now hand the screen over, before the install rather than as the process
    // dies. The plugin's `on_before_exit` hook sounds like the right moment but
    // runs too late: `install` writes the installer out to a temp file first
    // (unzipping it, when the bundle is zipped), so the cover used to appear
    // several seconds after the download finished, with the app sitting there
    // fully interactive in the meantime. The hook stays wired as a backstop.
    raise_update_cover(&app);
    log::info!("installing update {version}");

    // On Windows this does not return on success.
    if let Err(e) = update.install(&bytes) {
        lower_update_cover(&app);

        // On Linux a package install needs root. The updater tries pkexec, then a
        // graphical sudo, then a terminal sudo; when every one is missing or the
        // user dismisses the prompt, this is not a failure they can act on by
        // retrying. Hand back the download page instead of the raw dpkg text.
        #[cfg(all(unix, not(target_os = "macos")))]
        if is_privilege_failure(&e) {
            log::info!("cannot self-install on this Linux package: {e}");
            return Ok(InstallOutcome::ManualRequired {
                version,
                url: RELEASES_PAGE.to_owned(),
            });
        }

        log::warn!("update {version}: install failed: {e}");
        return Err(AppError::Other(e.to_string()));
    }

    // Only reached if the platform's install path returns rather than exiting.
    Ok(InstallOutcome::Installing { version })
}

/// Where someone is sent when the app cannot install its own update.
#[cfg(all(unix, not(target_os = "macos")))]
const RELEASES_PAGE: &str = "https://github.com/Wutname1/GitWyrm/releases/latest";

/// Whether this failure is "we could not become root", rather than a broken
/// download or a bad signature - which the user does need to hear about.
///
/// Matched on the message because the plugin's error enum is not exhaustive for
/// our purposes and these variants carry no data. The strings come from
/// tauri-plugin-updater 2.10.1: `AuthenticationFailed` ("Authentication failed
/// or was cancelled") and the per-format install failures raised once every
/// escalation path has been exhausted.
#[cfg(all(unix, not(target_os = "macos")))]
fn is_privilege_failure(e: &tauri_plugin_updater::Error) -> bool {
    let text = e.to_string().to_lowercase();
    text.contains("authentication failed")
        || text.contains("failed to install .deb")
        || text.contains("failed to install .rpm")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn only_staged_helper_copies_match() {
        assert!(is_helper_copy_name("gitwyrm-update-23040.exe"));
        assert!(is_helper_copy_name("gitwyrm-update-8.exe"));

        // The temp folder is shared: anything that is not exactly our shape stays.
        assert!(!is_helper_copy_name("gitwyrm-update-.exe"));
        assert!(!is_helper_copy_name("gitwyrm-update-123.exe.tmp"));
        assert!(!is_helper_copy_name("gitwyrm-update-12a.exe"));
        assert!(!is_helper_copy_name("gitwyrm-update-123.dll"));
        assert!(!is_helper_copy_name("gitwyrm-setup.exe"));
        assert!(!is_helper_copy_name("GitWyrm-Setup.log"));
        assert!(!is_helper_copy_name("other-gitwyrm-update-123.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn sweep_removes_helper_copies_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["gitwyrm-update-1.exe", "gitwyrm-update-22.exe", "keep.exe", "gitwyrm-update-x.exe"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        // A directory with a matching name is not a file we staged.
        std::fs::create_dir(dir.path().join("gitwyrm-update-3.exe")).unwrap();

        assert_eq!(sweep_helper_copies(dir.path()), 2);

        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["gitwyrm-update-3.exe", "gitwyrm-update-x.exe", "keep.exe"]);
    }

    #[cfg(windows)]
    #[test]
    fn sweep_of_a_missing_folder_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(sweep_helper_copies(&dir.path().join("gone")), 0);
    }

    #[test]
    fn install_mode_is_a_known_value() {
        assert!(matches!(
            update_install_mode(),
            UpdateInstallMode::SelfUpdate | UpdateInstallMode::SystemPackage
        ));
    }

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
        assert!(
            beta.contains("beta"),
            "beta endpoint should name its channel"
        );
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
            assert!(
                parse_version(skipped) > current,
                "{skipped} should be newer"
            );
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
        assert_eq!(
            seen,
            vec!["0.8.1"],
            "the release being installed must appear"
        );
    }

    #[test]
    fn a_beta_jumping_past_its_base_sees_every_release_in_between() {
        let seen = visible("0.8.1-beta.3", "0.9.0", &["0.9.0", "0.8.1", "0.8.0"]);
        assert_eq!(seen, vec!["0.9.0", "0.8.1"]);
        assert!(
            !seen.contains(&"0.8.0".to_string()),
            "0.8.0 predates the beta"
        );
    }

    #[test]
    fn prerelease_notes_are_hidden_when_landing_on_stable() {
        // The betas already run would otherwise repeat the release's own lines
        // under older version numbers.
        let seen = visible(
            "0.8.1-beta.1",
            "0.8.1",
            &["0.8.1", "0.8.1-beta.3", "0.8.1-beta.2"],
        );
        assert_eq!(seen, vec!["0.8.1"]);
    }

    #[test]
    fn beta_to_beta_keeps_the_prerelease_notes() {
        // No stable entry exists yet, so these are the only notes there are.
        let seen = visible(
            "0.8.1-beta.1",
            "0.8.1-beta.4",
            &["0.8.1-beta.4", "0.8.1-beta.2", "0.8.0"],
        );
        assert_eq!(seen, vec!["0.8.1-beta.4", "0.8.1-beta.2"]);
    }

    #[test]
    fn a_stable_user_is_unaffected_by_the_beta_rules() {
        // Strict `>`, and prereleases never shown.
        let seen = visible(
            "0.8.0",
            "0.9.0",
            &["0.9.0", "0.8.1", "0.8.1-beta.2", "0.8.0"],
        );
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
