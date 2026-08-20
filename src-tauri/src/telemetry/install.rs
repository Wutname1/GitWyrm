//! Counts deployed builds: one ping per install, at most once a day.
//!
//! # Why this exists
//!
//! Downloads stopped being a usable proxy for installs the moment GitWyrm
//! shipped Linux packages. A `.deb` can be mirrored, an AppImage passed around,
//! and a distro repository serves copies nobody here ever sees. Without a count
//! there is no way to tell a platform with a handful of users from one with
//! none -- which is the difference between a Fedora-only bug being worth a
//! release and being invisible.
//!
//! # What is sent
//!
//! An install id, the version, channel, OS and architecture, and whether this is
//! the install's first launch. Nothing else: no repository paths, no branch or
//! remote names, no account, no file contents, no identifier derived from the
//! machine or the user. The server hashes the id before storing it (see the
//! website's `/api/v1/installs` route), so even the stored row cannot be walked
//! back to the value held here.
//!
//! The id identifies *an installation*, not a person. It is a random v4 UUID, so
//! two installs by the same person are two ids, and the same id appears nowhere
//! else in the app or in any request it makes.
//!
//! # Rules this follows
//!
//! - **Off means off.** Gated on `TelemetryLevel::reports_installs`, checked
//!   before any state is created or any request built.
//! - **Once a day, by not asking.** The last ping date is stored locally; if it
//!   is today, no request is made at all. The dedupe is the absence of a call,
//!   not a call the server throws away.
//! - **Unless the build changed.** The daily dedupe is also keyed on the version
//!   and channel that were last reported. Otherwise an install that updates
//!   after its ping keeps being counted on the old version until tomorrow, and a
//!   release looks like it has no users on the day it ships.
//! - **Never blocks or interrupts.** Spawned detached, short timeout, every
//!   failure swallowed at debug level. A telemetry endpoint being down must
//!   never be something the user can perceive.
//! - **Never in development.** Debug builds return before doing anything, so
//!   local runs stay out of the counts.

use crate::settings::{self, TelemetryLevel, UpdateChannel};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Where the ping goes. Same host as the changelog API the app already reads.
const ENDPOINT: &str = "https://gitwyrm.com/api/v1/installs";

/// How long to wait before giving up. Short: nothing depends on the result, and
/// a hung request should not keep a task alive across a whole session.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The file holding the install id and the last ping date.
///
/// Deliberately *not* `settings.json`. That file is rebuilt wholesale by the
/// frontend on every preference change, and a value the user never chose has no
/// business riding along in it: one mistake there would either export the id
/// with someone's settings or regenerate it on an unrelated save.
const STATE_FILE: &str = "install-id.json";

/// The persisted state: who this install is, and when it last checked in.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct InstallState {
    /// Random v4 UUID, generated once on first launch.
    id: String,
    /// `YYYY-MM-DD` of the last successful ping, or `None` if none has landed.
    #[serde(default)]
    last_ping: Option<String>,
    /// Whether the install event has been reported. Distinct from `last_ping`
    /// being `None`, which is also the state after a first ping that failed.
    #[serde(default)]
    first_reported: bool,
    /// The version reported by the last successful ping. `None` for state written
    /// before this field existed, which counts as "changed" so the first launch
    /// after updating re-reports.
    #[serde(default)]
    last_version: Option<String>,
    /// The channel reported by the last successful ping.
    #[serde(default)]
    last_channel: Option<String>,
}

/// The payload. Flat and small on purpose: every field is one someone could
/// reasonably expect in a count of deployed builds, and adding one that is not
/// would make the privacy page inaccurate.
#[derive(Debug, Serialize)]
struct Ping<'a> {
    /// Random per-install id. Hashed server-side before storage.
    install_id: &'a str,
    /// App version, e.g. `0.9.3`.
    version: &'a str,
    /// `stable` or `beta`.
    channel: &'a str,
    /// `windows`, `linux`, or `macos`.
    os: &'a str,
    /// `x86_64` or `aarch64`.
    arch: &'a str,
    /// True only on the very first ping from this install, which is what
    /// separates an install count from an active count.
    first_run: bool,
}

/// Today's date as `YYYY-MM-DD`, UTC.
///
/// UTC rather than local time so someone crossing a date line cannot ping twice
/// in a day, and so the server's 30-day active window compares like with like.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    civil_from_days((secs / 86_400) as i64)
}

/// Civil date from a Unix day number, formatted `YYYY-MM-DD`.
///
/// Hinnant's algorithm. Inlined rather than pulling in a date crate for a single
/// format call.
fn civil_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn state_path(dir: &Path) -> PathBuf {
    dir.join(STATE_FILE)
}

/// Read the stored state, or `None` if this install has never had one.
fn read_state(dir: &Path) -> Option<InstallState> {
    let raw = std::fs::read_to_string(state_path(dir)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_state(dir: &Path, state: &InstallState) {
    let Ok(json) = serde_json::to_string_pretty(state) else {
        return;
    };
    // A failure here means the next launch pings again and the server sees a
    // second ping for the day. Harmless, and not worth surfacing.
    let _ = std::fs::write(state_path(dir), json);
}

/// A fresh random v4 UUID.
fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn arch_name() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    }
}

/// Send the launch ping if one is due, in the background.
///
/// Returns immediately. Call once during setup, after the app data directory
/// exists.
pub fn ping_on_launch(app: &tauri::AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(dir) = settings::app_data_dir(app) else {
        return;
    };
    let level = settings::telemetry_level();
    let channel = match settings::read_settings(app).map(|s| s.update_channel) {
        Ok(UpdateChannel::Beta) => "beta",
        _ => "stable",
    };

    tauri::async_runtime::spawn(async move {
        send_if_due(&dir, level, channel).await;
    });
}

/// Whether a ping should be sent, given what was last reported.
///
/// Split out from the request so the dedupe can be tested without a network:
/// this is the whole of the rule, and `send_if_due` does nothing but obey it.
///
/// An app restarted six times a day is one active install, and the way to make
/// it one is to not make the other five requests. An update is the one thing
/// that beats that: the count exists to say which build is deployed, and after
/// an update the stored answer is wrong for the rest of the day.
fn is_due(state: &InstallState, today: &str, version: &str, channel: &str) -> bool {
    let same_build = state.last_version.as_deref() == Some(version)
        && state.last_channel.as_deref() == Some(channel);
    !same_build || state.last_ping.as_deref() != Some(today)
}

/// The decision and the request, kept separate from the Tauri handle so the
/// gating and dedupe can be tested against a temporary directory.
async fn send_if_due(dir: &Path, level: TelemetryLevel, channel: &str) {
    if !level.reports_installs() {
        return;
    }

    let today = today();
    let version = env!("CARGO_PKG_VERSION");
    let mut state = read_state(dir).unwrap_or_else(|| InstallState {
        id: new_id(),
        last_ping: None,
        first_reported: false,
        last_version: None,
        last_channel: None,
    });

    if !is_due(&state, &today, version, channel) {
        return;
    }

    let payload = Ping {
        install_id: &state.id,
        version,
        channel,
        os: os_name(),
        arch: arch_name(),
        first_run: !state.first_reported,
    };

    let Ok(client) = reqwest::Client::builder().timeout(TIMEOUT).build() else {
        return;
    };
    match client.post(ENDPOINT).json(&payload).send().await {
        Ok(response) if response.status().is_success() => {
            state.last_ping = Some(today);
            state.first_reported = true;
            state.last_version = Some(version.to_owned());
            state.last_channel = Some(channel.to_owned());
            write_state(dir, &state);
        }
        // Not retried: the next launch tries again, and a missed count is not worth
        // a backoff loop running behind someone's session.
        Ok(response) => log::debug!("install ping returned {}", response.status()),
        Err(error) => log::debug!("install ping could not be sent: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), "1970-01-01");
        assert_eq!(civil_from_days(19_723), "2024-01-01");
        // A leap day, which is where an off-by-one in the algorithm would show.
        assert_eq!(civil_from_days(19_782), "2024-02-29");
    }

    #[test]
    fn today_is_an_iso_date() {
        let value = today();
        assert_eq!(value.len(), 10, "expected YYYY-MM-DD, got {value}");
        let year: i64 = value[..4].parse().expect("year should parse");
        assert!(year >= 2024, "clock-derived year looks wrong: {value}");
    }

    #[tokio::test]
    async fn off_creates_no_state_at_all() {
        let dir = tempfile::tempdir().expect("temp dir");
        send_if_due(dir.path(), TelemetryLevel::Off, "stable").await;
        assert!(
            read_state(dir.path()).is_none(),
            "opting out must not even generate an install id"
        );
    }

    /// State that looks like a successful ping earlier today on this build.
    fn pinged_today(channel: &str) -> InstallState {
        InstallState {
            id: "fixed-id".to_owned(),
            last_ping: Some(today()),
            first_reported: true,
            last_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            last_channel: Some(channel.to_owned()),
        }
    }

    #[tokio::test]
    async fn a_ping_already_sent_today_is_not_repeated() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_state(dir.path(), &pinged_today("stable"));

        // No network in tests: what is being asserted is that this returns without
        // attempting one, leaving the stored state untouched.
        send_if_due(dir.path(), TelemetryLevel::Full, "stable").await;

        let after = read_state(dir.path()).expect("state should survive");
        assert_eq!(after.id, "fixed-id");
        assert_eq!(after.last_ping.as_deref(), Some(today().as_str()));
    }

    #[test]
    fn a_second_launch_on_the_same_build_is_not_due() {
        let state = pinged_today("stable");
        assert!(!is_due(
            &state,
            &today(),
            env!("CARGO_PKG_VERSION"),
            "stable"
        ));
    }

    #[test]
    fn an_update_is_due_the_same_day() {
        let mut state = pinged_today("stable");
        state.last_version = Some("0.0.1-stale".to_owned());
        assert!(
            is_due(&state, &today(), env!("CARGO_PKG_VERSION"), "stable"),
            "an install that updated today must re-report, not wait for tomorrow"
        );
    }

    #[test]
    fn switching_channel_is_due_the_same_day() {
        let state = pinged_today("beta");
        assert!(
            is_due(&state, &today(), env!("CARGO_PKG_VERSION"), "stable"),
            "moving off beta must show up on the same day"
        );
    }

    #[test]
    fn a_new_day_is_due_on_an_unchanged_build() {
        let mut state = pinged_today("stable");
        state.last_ping = Some("2000-01-01".to_owned());
        assert!(is_due(
            &state,
            &today(),
            env!("CARGO_PKG_VERSION"),
            "stable"
        ));
    }

    #[test]
    fn state_from_before_version_tracking_is_due() {
        // Every install in the field right now has state with no last_version, and
        // it must re-report on its next launch rather than wait for a version bump.
        let legacy = r#"{"id":"a","last_ping":"2026-08-20","first_reported":true}"#;
        let state: InstallState = serde_json::from_str(legacy).expect("legacy state should parse");
        assert!(is_due(
            &state,
            "2026-08-20",
            env!("CARGO_PKG_VERSION"),
            "stable"
        ));
    }

    #[test]
    fn ids_are_unique_and_well_formed() {
        let (a, b) = (new_id(), new_id());
        assert_ne!(a, b, "each install must get its own id");
        assert!(
            uuid::Uuid::parse_str(&a).is_ok(),
            "expected a UUID, got {a}"
        );
    }
}
