//! Account-wide usage limits for the AI tools Agent Desk drives.
//!
//! Each tool rations its plan in windows (five hours, a week, a month), and
//! running into one mid-task is the most confusing way to find out it exists.
//! This module reads where each tool already is in those windows so Agent Desk
//! can show a bar per window with its reset time.
//!
//! Every source is different, and none of them is an official, stable API:
//!
//! - **Codex** writes its limits into the session logs it keeps on disk, so
//!   reading them needs no network at all. The catch is that the numbers are
//!   only as fresh as the last Codex chat.
//! - **Claude Code** keeps an OAuth token on disk that answers an unofficial
//!   usage endpoint. That token is borrowed, never refreshed: refreshing
//!   rotates it and signs the person out of the Claude CLI.
//! - **GitHub Copilot** answers through the Copilot SDK's experimental
//!   `account.getQuota` RPC, which costs a subprocess per call.
//!
//! Because all three can break without notice, a failure is never an error
//! here. It becomes a plain sentence in `unavailable_reason`, or, when an
//! earlier answer exists, that earlier answer with its `fetched_at` showing
//! its age.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use github_copilot_sdk::rpc::{AccountGetQuotaRequest, AccountQuotaSnapshot};
use github_copilot_sdk::{Client, ClientOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Where one AI tool stands against its plan's limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    /// Agent registry id: `"claude"`, `"codex"` or `"copilot"`.
    pub provider: String,
    pub display_name: String,
    /// The plan name as the tool reports it, e.g. "Max 20x" or "Plus".
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    /// When these numbers were read (RFC 3339). For Codex this is when Codex
    /// itself last reported them, which can be well before this call.
    pub fetched_at: Option<String>,
    /// Plain-language reason the limits could not be shown. Set only when
    /// there is no earlier answer to fall back on.
    pub unavailable_reason: Option<String>,
}

/// One limit window, drawn as one progress bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub label: String,
    pub kind: UsageWindowKind,
    /// 0..=100.
    pub used_percent: f32,
    /// When the window starts over (RFC 3339), if the tool says.
    pub resets_at: Option<String>,
    /// Extra words for the bar, e.g. "212 of 300 used".
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UsageWindowKind {
    FiveHour,
    Weekly,
    Monthly,
    Other,
}

const CLAUDE_ID: &str = "claude";
const CODEX_ID: &str = "codex";
const COPILOT_ID: &str = "copilot";

/// Codex costs a directory walk and nothing else, so it can be read often.
const CODEX_INTERVAL: Duration = Duration::from_secs(30);
/// The Claude endpoint is unofficial and rate limited; asking every few
/// seconds is the quickest way to lose it.
const CLAUDE_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// Each Copilot read starts a CLI subprocess.
const COPILOT_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// After a 429 the endpoint has told us to back off. `force` does not skip
/// this: a refresh button pressed in a loop would only extend the ban.
const CLAUDE_BACKOFF: Duration = Duration::from_secs(15 * 60);

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_TIMEOUT: Duration = Duration::from_secs(10);
/// Covers the CLI's first-run self-extraction as well as the RPC itself.
const COPILOT_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounds the directory walk on machines with years of Codex history.
const CODEX_MAX_FILES: usize = 40;

const CODEX_NOT_YET: &str =
    "Codex has not reported its limits yet. They appear after your next Codex chat.";
const CLAUDE_SIGN_IN: &str = "Sign in to Claude Code again to see your limits.";
const CLAUDE_EXPIRED: &str = "Open Claude Code once to refresh its sign-in, then check again.";
const CLAUDE_BUSY: &str =
    "Claude asked us to slow down. Your limits will show again in a few minutes.";
const CLAUDE_FAILED: &str = "Could not read your Claude limits right now.";
const COPILOT_FAILED: &str = "Could not read your Copilot limits right now.";

// ---------------------------------------------------------------------------
// Entry point and cache
// ---------------------------------------------------------------------------

/// Every provider that looks set up on this machine, read concurrently.
///
/// A provider with no trace on the machine is left out entirely, so the
/// panel does not nag about tools the person never installed.
pub async fn all(app: &tauri::AppHandle, force: bool) -> Vec<ProviderUsage> {
    let (claude, codex, copilot) =
        tokio::join!(claude_usage(force), codex_usage(force), copilot_usage(app.clone(), force));
    let now = now_unix();
    [claude, codex, copilot]
        .into_iter()
        .flatten()
        .map(|mut usage| {
            // A cached answer can outlive its windows.
            let read_at = usage.fetched_at.as_deref().and_then(unix_from_rfc3339);
            for w in &mut usage.windows {
                roll_over(w, now, read_at);
            }
            usage
        })
        .collect()
}

struct Slot {
    /// What the last call returned, served again until the interval passes.
    answer: ProviderUsage,
    at: Instant,
    /// The last answer that actually had numbers, kept for when a later read
    /// fails.
    good: Option<ProviderUsage>,
}

static CACHE: LazyLock<Mutex<HashMap<&'static str, Slot>>> = LazyLock::new(Default::default);
static CLAUDE_BACKOFF_UNTIL: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(Default::default);

/// One read at a time per provider, so two panels opening together start one
/// Copilot subprocess rather than two. Each caller re-checks the cache after
/// taking the lock and finds the first caller's answer.
static CLAUDE_FETCH: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);
static CODEX_FETCH: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);
static COPILOT_FETCH: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);

fn cache() -> std::sync::MutexGuard<'static, HashMap<&'static str, Slot>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

fn fresh_cached(id: &'static str, interval: Duration, force: bool) -> Option<ProviderUsage> {
    if force {
        return None;
    }
    let cache = cache();
    let slot = cache.get(id)?;
    (slot.at.elapsed() < interval).then(|| slot.answer.clone())
}

fn last_good(id: &'static str) -> Option<ProviderUsage> {
    cache().get(id).and_then(|s| s.good.clone())
}

/// Stores a new reading and returns what the caller should see: the reading
/// itself, or the last good one when this reading failed.
fn remember(id: &'static str, reading: ProviderUsage) -> ProviderUsage {
    let mut cache = cache();
    let previous_good = cache.get(id).and_then(|s| s.good.clone());
    let (answer, good) = if reading.unavailable_reason.is_none() {
        (reading.clone(), Some(reading))
    } else {
        match previous_good {
            Some(good) => (good.clone(), Some(good)),
            None => (reading, None),
        }
    };
    cache.insert(id, Slot { answer: answer.clone(), at: Instant::now(), good });
    answer
}

fn unavailable(id: &str, display_name: &str, plan: Option<String>, reason: &str) -> ProviderUsage {
    ProviderUsage {
        provider: id.to_string(),
        display_name: display_name.to_string(),
        plan,
        windows: Vec::new(),
        fetched_at: None,
        unavailable_reason: Some(reason.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn now_rfc3339() -> Option<String> {
    OffsetDateTime::now_utc().format(&Rfc3339).ok()
}

fn rfc3339_from_unix(secs: i64) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(secs).ok()?.format(&Rfc3339).ok()
}

fn unix_from_rfc3339(s: &str) -> Option<i64> {
    OffsetDateTime::parse(s, &Rfc3339).ok().map(|d| d.unix_timestamp())
}

/// Epoch seconds or milliseconds, told apart by size: no reset is ever in the
/// year 5138, which is where a millisecond value would land as seconds.
fn rfc3339_from_epoch(n: f64) -> Option<String> {
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    let secs = if n > 1e11 { n / 1000.0 } else { n };
    rfc3339_from_unix(secs as i64)
}

/// An ISO 8601 timestamp, a bare date, or an epoch number, as RFC 3339.
fn flexible_time(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => rfc3339_from_epoch(n.as_f64()?),
        Value::String(s) => {
            let s = s.trim();
            if let Some(secs) = unix_from_rfc3339(s) {
                return rfc3339_from_unix(secs);
            }
            if let Ok(n) = s.parse::<f64>() {
                return rfc3339_from_epoch(n);
            }
            let date_only = time::macros::format_description!("[year]-[month]-[day]");
            let date = time::Date::parse(s, &date_only).ok()?;
            date.midnight().assume_utc().format(&Rfc3339).ok()
        }
        _ => None,
    }
}

fn clamp_percent(v: f64) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 100.0) as f32
    } else {
        0.0
    }
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A window whose reset time has passed has started over, whatever the last
/// reading said. Without this a Codex bar would sit at 90% for days after the
/// week rolled over, because Codex only reports when it is used.
///
/// Only a reset that came AFTER the reading counts. Copilot's free plan sends
/// the moment of the request as its "reset date", which is always already
/// past by the time it is read -- treating that as a rollover zeroed every
/// bar. A reset time at or before the reading is not a reset at all, so it is
/// dropped and the numbers are kept. `read_at` is `None` when the reading
/// time is unknown, which keeps the plain "has it passed?" rule.
fn roll_over(window: &mut UsageWindow, now: i64, read_at: Option<i64>) {
    let Some(at) = window.resets_at.as_deref().and_then(unix_from_rfc3339) else {
        return;
    };
    if read_at.is_some_and(|read| at <= read) {
        window.resets_at = None;
        return;
    }
    if at <= now {
        window.used_percent = 0.0;
        window.resets_at = None;
        window.detail = None;
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// What one usable `token_count` line says.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CodexReading {
    pub windows: Vec<UsageWindow>,
    pub plan: Option<String>,
    /// The line's own timestamp: when Codex learned these numbers.
    pub reported_at: Option<String>,
}

async fn codex_usage(force: bool) -> Option<ProviderUsage> {
    let codex_home = home_dir()?.join(".codex");
    if !codex_home.is_dir() {
        return None;
    }
    let _guard = CODEX_FETCH.lock().await;
    if let Some(cached) = fresh_cached(CODEX_ID, CODEX_INTERVAL, force) {
        return Some(cached);
    }

    let sessions = codex_home.join("sessions");
    let reading = tauri::async_runtime::spawn_blocking(move || {
        scan_codex_sessions(&sessions, now_unix())
    })
    .await
    .unwrap_or_else(|e| {
        log::warn!("provider usage: Codex session scan did not finish: {e}");
        None
    });

    let usage = match reading {
        Some(r) => ProviderUsage {
            provider: CODEX_ID.to_string(),
            display_name: "Codex".to_string(),
            plan: r.plan,
            windows: r.windows,
            fetched_at: r.reported_at.or_else(now_rfc3339),
            unavailable_reason: None,
        },
        None => {
            log::info!("provider usage: no Codex rate limits found in recent sessions");
            unavailable(CODEX_ID, "Codex", None, CODEX_NOT_YET)
        }
    };
    Some(remember(CODEX_ID, usage))
}

/// The newest rate limits Codex recorded under `sessions/YYYY/MM/DD/`.
///
/// Files are ordered by modification time rather than by folder, because a
/// long session started yesterday lives in yesterday's folder but may hold
/// today's numbers. Files written by the desktop app often carry
/// `rate_limits: null`, so the walk keeps going until a file has real ones.
pub(crate) fn scan_codex_sessions(sessions: &Path, now: i64) -> Option<CodexReading> {
    let mut files: Vec<PathBuf> = Vec::new();
    'outer: for year in dirs_newest_first(sessions) {
        for month in dirs_newest_first(&year) {
            for day in dirs_newest_first(&month) {
                files.extend(rollout_files(&day));
                if files.len() >= CODEX_MAX_FILES {
                    break 'outer;
                }
            }
        }
    }

    let mut dated: Vec<(std::time::SystemTime, PathBuf)> = files
        .into_iter()
        .map(|p| {
            let mtime = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (mtime, p)
        })
        .collect();
    dated.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    dated
        .into_iter()
        .take(CODEX_MAX_FILES)
        .find_map(|(_, path)| last_reading_in_file(&path, now))
}

fn dirs_newest_first(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Zero-padded YYYY / MM / DD names sort chronologically as text.
    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    dirs
}

fn rollout_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        })
        .collect()
}

/// The last usable line in one rollout file: limits only grow during a
/// session, so the last report is the current one.
fn last_reading_in_file(path: &Path, now: i64) -> Option<CodexReading> {
    let file = std::fs::File::open(path).ok()?;
    let mut last = None;
    for line in BufReader::new(file).split(b'\n').map_while(Result::ok) {
        // Most lines are chat content; skip them before paying for a parse.
        if !contains(&line, b"rate_limits") {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&line) else {
            continue;
        };
        if let Some(reading) = parse_codex_line(text, now) {
            last = Some(reading);
        }
    }
    last
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// One rollout line, if it is a `token_count` event carrying real limits.
pub(crate) fn parse_codex_line(line: &str, now: i64) -> Option<CodexReading> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = v.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let limits = payload.get("rate_limits")?;
    if !limits.is_object() {
        return None;
    }
    let windows: Vec<UsageWindow> = ["primary", "secondary"]
        .iter()
        .filter_map(|key| limits.get(*key).and_then(|w| codex_window(w, now)))
        .collect();
    if windows.is_empty() {
        return None;
    }
    let plan = limits
        .get("plan_type")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(capitalise);
    let reported_at = v
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(unix_from_rfc3339)
        .and_then(rfc3339_from_unix);
    Some(CodexReading { windows, plan, reported_at })
}

fn codex_window(w: &Value, now: i64) -> Option<UsageWindow> {
    if !w.is_object() {
        return None;
    }
    let used = w.get("used_percent")?.as_f64()?;
    let minutes = w.get("window_minutes").and_then(Value::as_f64).map(|m| m as i64);
    let (kind, label) = match minutes {
        Some(300) => (UsageWindowKind::FiveHour, "5-hour limit".to_string()),
        Some(10080) => (UsageWindowKind::Weekly, "Weekly limit".to_string()),
        Some(m) if m > 0 => (UsageWindowKind::Other, duration_label(m)),
        _ => (UsageWindowKind::Other, "Usage limit".to_string()),
    };
    let mut window = UsageWindow {
        label,
        kind,
        used_percent: clamp_percent(used),
        resets_at: w.get("resets_at").and_then(Value::as_f64).and_then(rfc3339_from_epoch),
        detail: None,
    };
    roll_over(&mut window, now, None);
    Some(window)
}

fn duration_label(minutes: i64) -> String {
    if minutes % 1440 == 0 {
        format!("{}-day limit", minutes / 1440)
    } else if minutes % 60 == 0 {
        format!("{}-hour limit", minutes / 60)
    } else {
        format!("{minutes}-minute limit")
    }
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

/// The parts of `~/.claude/.credentials.json` this needs. No `Debug`: the
/// token must never reach a log line by accident.
pub(crate) struct ClaudeCreds {
    pub access_token: Option<String>,
    pub expires_at_ms: Option<f64>,
    pub plan: Option<String>,
}

enum ClaudeFetch {
    Windows(Vec<UsageWindow>),
    RateLimited,
    Failed(&'static str),
}

async fn claude_usage(force: bool) -> Option<ProviderUsage> {
    let path = home_dir()?.join(".claude").join(".credentials.json");
    let creds = tauri::async_runtime::spawn_blocking(move || {
        let raw = std::fs::read_to_string(&path).ok()?;
        Some(parse_claude_credentials(&raw))
    })
    .await
    .ok()
    .flatten()?;

    let _guard = CLAUDE_FETCH.lock().await;
    if let Some(cached) = fresh_cached(CLAUDE_ID, CLAUDE_INTERVAL, force) {
        return Some(cached);
    }

    let plan = creds.as_ref().and_then(|c| c.plan.clone());
    let backing_off = CLAUDE_BACKOFF_UNTIL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some_and(|until| Instant::now() < until);
    if backing_off {
        return Some(
            last_good(CLAUDE_ID)
                .unwrap_or_else(|| unavailable(CLAUDE_ID, "Claude Code", plan, CLAUDE_BUSY)),
        );
    }

    let Some(token) = creds.as_ref().and_then(|c| c.access_token.clone()) else {
        log::info!("provider usage: Claude credentials have no usable sign-in");
        return Some(remember(CLAUDE_ID, unavailable(CLAUDE_ID, "Claude Code", plan, CLAUDE_SIGN_IN)));
    };
    let expired = creds
        .as_ref()
        .and_then(|c| c.expires_at_ms)
        .is_some_and(|ms| ms / 1000.0 <= now_unix() as f64);
    if expired {
        // Refreshing would rotate the token and sign the Claude CLI out, so
        // the person refreshes it by opening Claude Code instead.
        log::info!("provider usage: Claude sign-in has expired; not refreshing it");
        return Some(remember(CLAUDE_ID, unavailable(CLAUDE_ID, "Claude Code", plan, CLAUDE_EXPIRED)));
    }

    let usage = match fetch_claude_usage(&token).await {
        ClaudeFetch::Windows(windows) => ProviderUsage {
            provider: CLAUDE_ID.to_string(),
            display_name: "Claude Code".to_string(),
            plan,
            windows,
            fetched_at: now_rfc3339(),
            unavailable_reason: None,
        },
        ClaudeFetch::RateLimited => {
            *CLAUDE_BACKOFF_UNTIL.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(Instant::now() + CLAUDE_BACKOFF);
            unavailable(CLAUDE_ID, "Claude Code", plan, CLAUDE_BUSY)
        }
        ClaudeFetch::Failed(reason) => unavailable(CLAUDE_ID, "Claude Code", plan, reason),
    };
    Some(remember(CLAUDE_ID, usage))
}

pub(crate) fn parse_claude_credentials(raw: &str) -> Option<ClaudeCreds> {
    let v: Value = serde_json::from_str(raw).ok()?;
    let oauth = v.get("claudeAiOauth")?;
    let access_token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let expires_at_ms = oauth.get("expiresAt").and_then(|e| match e {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    });
    let plan = claude_plan(
        oauth.get("subscriptionType").and_then(Value::as_str),
        oauth.get("rateLimitTier").and_then(Value::as_str),
    );
    Some(ClaudeCreds { access_token, expires_at_ms, plan })
}

/// "max" with tier "default_claude_max_20x" reads as "Max 20x".
fn claude_plan(subscription: Option<&str>, tier: Option<&str>) -> Option<String> {
    let base = capitalise(subscription.filter(|s| !s.is_empty())?);
    let multiplier = tier
        .and_then(|t| t.rsplit('_').next())
        .filter(|seg| {
            seg.len() > 1
                && seg.ends_with('x')
                && seg[..seg.len() - 1].chars().all(|c| c.is_ascii_digit())
        });
    Some(match multiplier {
        Some(m) => format!("{base} {m}"),
        None => base,
    })
}

async fn fetch_claude_usage(token: &str) -> ClaudeFetch {
    let client = match reqwest::Client::builder().timeout(CLAUDE_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("provider usage: could not build an HTTP client: {e}");
            return ClaudeFetch::Failed(CLAUDE_FAILED);
        }
    };
    let res = client
        .get(CLAUDE_USAGE_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", "GitWyrm")
        .send()
        .await;
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            log::warn!("provider usage: Claude usage request failed: {e}");
            return ClaudeFetch::Failed(CLAUDE_FAILED);
        }
    };
    let status = res.status();
    if status.as_u16() == 429 {
        log::warn!("provider usage: Claude usage endpoint answered 429; backing off");
        return ClaudeFetch::RateLimited;
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        log::info!("provider usage: Claude usage endpoint answered {status}");
        return ClaudeFetch::Failed(CLAUDE_SIGN_IN);
    }
    if !status.is_success() {
        log::warn!("provider usage: Claude usage endpoint answered {status}");
        return ClaudeFetch::Failed(CLAUDE_FAILED);
    }
    let body: Value = match res.json().await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("provider usage: Claude usage reply was not JSON: {e}");
            return ClaudeFetch::Failed(CLAUDE_FAILED);
        }
    };
    let windows = parse_claude_usage(&body);
    if windows.is_empty() {
        log::warn!("provider usage: Claude usage reply had no limit windows");
        return ClaudeFetch::Failed(CLAUDE_FAILED);
    }
    ClaudeFetch::Windows(windows)
}

/// The usage reply's windows, in the order the bars should appear.
pub(crate) fn parse_claude_usage(body: &Value) -> Vec<UsageWindow> {
    const WINDOWS: [(&str, UsageWindowKind, &str); 4] = [
        ("five_hour", UsageWindowKind::FiveHour, "5-hour limit"),
        ("seven_day", UsageWindowKind::Weekly, "Weekly limit"),
        ("seven_day_opus", UsageWindowKind::Weekly, "Weekly Opus limit"),
        ("seven_day_sonnet", UsageWindowKind::Weekly, "Weekly Sonnet limit"),
    ];
    let raw: Vec<(UsageWindowKind, &str, f64, Option<String>)> = WINDOWS
        .iter()
        .filter_map(|(key, kind, label)| {
            let w = body.get(*key).filter(|w| w.is_object())?;
            let utilization = w.get("utilization").and_then(Value::as_f64)?;
            let resets_at = w.get("resets_at").and_then(flexible_time);
            Some((*kind, *label, utilization, resets_at))
        })
        .collect();

    // The endpoint reports percents today. If it ever switches to fractions,
    // every value drops to 1.0 or below and at least one lands strictly
    // between 0 and 1. Requiring that second part keeps a genuine "1% used"
    // from being read as 100%.
    let fractions = raw.iter().all(|(_, _, u, _)| *u <= 1.0)
        && raw.iter().any(|(_, _, u, _)| *u > 0.0 && *u < 1.0);
    let scale = if fractions { 100.0 } else { 1.0 };

    raw.into_iter()
        .map(|(kind, label, utilization, resets_at)| UsageWindow {
            label: label.to_string(),
            kind,
            used_percent: clamp_percent(utilization * scale),
            resets_at,
            detail: None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// GitHub Copilot
// ---------------------------------------------------------------------------

async fn copilot_usage(app: tauri::AppHandle, force: bool) -> Option<ProviderUsage> {
    let token = tauri::async_runtime::spawn_blocking(move || {
        match super::auth::get(&app, super::copilot_sdk::PROVIDER_ID) {
            Ok(Some(super::auth::AuthInfo::Api { key })) => Some(key),
            Ok(Some(super::auth::AuthInfo::Oauth { refresh, .. })) => Some(refresh),
            Ok(None) => None,
            Err(e) => {
                log::warn!("provider usage: could not read the saved Copilot sign-in: {e}");
                None
            }
        }
    })
    .await
    .ok()
    .flatten()
    .filter(|t| !t.is_empty())?;

    let _guard = COPILOT_FETCH.lock().await;
    if let Some(cached) = fresh_cached(COPILOT_ID, COPILOT_INTERVAL, force) {
        return Some(cached);
    }

    let usage = match fetch_copilot_quota(token).await {
        Some(windows) if !windows.is_empty() => ProviderUsage {
            provider: COPILOT_ID.to_string(),
            display_name: "GitHub Copilot".to_string(),
            plan: None,
            windows,
            fetched_at: now_rfc3339(),
            unavailable_reason: None,
        },
        Some(_) => {
            log::warn!("provider usage: Copilot quota reply had no snapshots");
            unavailable(COPILOT_ID, "GitHub Copilot", None, COPILOT_FAILED)
        }
        None => unavailable(COPILOT_ID, "GitHub Copilot", None, COPILOT_FAILED),
    };
    Some(remember(COPILOT_ID, usage))
}

/// Starts the bundled CLI, asks for the quota, and always stops the CLI
/// again, including when the call fails or runs out of time.
async fn fetch_copilot_quota(token: String) -> Option<Vec<UsageWindow>> {
    let deadline = tokio::time::Instant::now() + COPILOT_TIMEOUT;
    let client = match tokio::time::timeout_at(deadline, Client::start(ClientOptions::default())).await {
        Ok(Ok(client)) => client,
        Ok(Err(e)) => {
            log::warn!("provider usage: could not start the Copilot CLI: {e}");
            return None;
        }
        Err(_) => {
            log::warn!("provider usage: the Copilot CLI took too long to start");
            return None;
        }
    };

    let result = tokio::time::timeout_at(
        deadline,
        client.rpc().account().get_quota_with_params(AccountGetQuotaRequest {
            git_hub_token: Some(token),
            selection_id: None,
        }),
    )
    .await;
    // Bounded so a wedged CLI cannot hold the panel open forever.
    let _ = tokio::time::timeout(Duration::from_secs(5), client.stop()).await;

    match result {
        Ok(Ok(quota)) => {
            // Counts and dates only: enough to explain a wrong bar, nothing private.
            for (key, snap) in &quota.quota_snapshots {
                log::info!(
                    "copilot quota {key}: entitlement={} used={} remaining={}% unlimited={} reset={:?}",
                    snap.entitlement_requests,
                    snap.used_requests,
                    snap.remaining_percentage,
                    snap.is_unlimited_entitlement,
                    snap.reset_date
                );
            }
            Some(copilot_windows(&quota.quota_snapshots))
        }
        Ok(Err(e)) => {
            log::warn!("provider usage: Copilot quota call failed: {e}");
            None
        }
        Err(_) => {
            log::warn!("provider usage: Copilot quota call timed out");
            None
        }
    }
}

/// Premium requests always show (even when unlimited, so the person sees
/// they have no cap). Chat and completions only show when they are capped,
/// which in practice means the free plan.
pub(crate) fn copilot_windows(snapshots: &HashMap<String, AccountQuotaSnapshot>) -> Vec<UsageWindow> {
    let mut windows = Vec::new();
    // A plan with no premium requests at all (the free plan reports 0 of 0)
    // has nothing to fill; a bar there would read as "used up".
    if let Some(snap) = snapshots
        .get("premium_interactions")
        .filter(|s| copilot_unlimited(s) || s.entitlement_requests > 0)
    {
        windows.push(copilot_window("Premium requests", snap));
    }
    for (key, label) in [("chat", "Chat messages"), ("completions", "Code completions")] {
        if let Some(snap) = snapshots.get(key).filter(|s| !copilot_unlimited(s)) {
            windows.push(copilot_window(label, snap));
        }
    }
    windows
}

/// "2,000" rather than "2000".
fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn copilot_unlimited(snap: &AccountQuotaSnapshot) -> bool {
    snap.is_unlimited_entitlement || snap.entitlement_requests < 0
}

pub(crate) fn copilot_window(label: &str, snap: &AccountQuotaSnapshot) -> UsageWindow {
    let resets_at = snap
        .reset_date
        .as_deref()
        .and_then(|d| flexible_time(&Value::String(d.to_string())));
    if copilot_unlimited(snap) {
        return UsageWindow {
            label: label.to_string(),
            kind: UsageWindowKind::Monthly,
            used_percent: 0.0,
            resets_at,
            detail: Some("Unlimited".to_string()),
        };
    }
    let mut detail = format!("{} of {} used", thousands(snap.used_requests), thousands(snap.entitlement_requests));
    if snap.overage.is_finite() && snap.overage >= 1.0 {
        detail.push_str(&format!(", plus {} extra", snap.overage.round() as i64));
    }
    UsageWindow {
        label: label.to_string(),
        kind: UsageWindowKind::Monthly,
        used_percent: clamp_percent(100.0 - snap.remaining_percentage),
        resets_at,
        detail: Some(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 2026-10-06T00:00:00Z.
    const NOW: i64 = 1_791_244_800;

    fn codex_line(rate_limits: Value) -> String {
        json!({
            "timestamp": "2026-10-05T23:00:00.500Z",
            "type": "event_msg",
            "payload": { "type": "token_count", "info": null, "rate_limits": rate_limits }
        })
        .to_string()
    }

    fn live_limits(used: f64) -> Value {
        json!({
            "limit_id": "codex",
            "limit_name": null,
            "primary": { "used_percent": used, "window_minutes": 300, "resets_at": NOW + 3600 },
            "secondary": { "used_percent": 7.0, "window_minutes": 10080, "resets_at": NOW + 86_400 },
            "plan_type": "plus"
        })
    }

    #[test]
    fn codex_line_gives_five_hour_and_weekly_windows() {
        let r = parse_codex_line(&codex_line(live_limits(43.0)), NOW).expect("usable line");
        assert_eq!(r.plan.as_deref(), Some("Plus"));
        assert_eq!(r.windows.len(), 2);
        assert_eq!(r.windows[0].kind, UsageWindowKind::FiveHour);
        assert_eq!(r.windows[0].label, "5-hour limit");
        assert_eq!(r.windows[0].used_percent, 43.0);
        assert_eq!(r.windows[0].resets_at, rfc3339_from_unix(NOW + 3600));
        assert_eq!(r.windows[1].kind, UsageWindowKind::Weekly);
        assert_eq!(r.windows[1].label, "Weekly limit");
        assert_eq!(r.reported_at.as_deref().and_then(unix_from_rfc3339), Some(NOW - 3600));
    }

    #[test]
    fn codex_window_past_its_reset_reads_as_empty() {
        let mut limits = live_limits(90.0);
        limits["primary"]["resets_at"] = json!(NOW - 60);
        let r = parse_codex_line(&codex_line(limits), NOW).expect("usable line");
        assert_eq!(r.windows[0].used_percent, 0.0);
        assert_eq!(r.windows[0].resets_at, None);
        assert_eq!(r.windows[1].used_percent, 7.0);
    }

    #[test]
    fn codex_line_without_limits_is_skipped() {
        assert!(parse_codex_line(&codex_line(Value::Null), NOW).is_none());
        let other = json!({ "type": "event_msg", "payload": { "type": "agent_message", "rate_limits": live_limits(1.0) } });
        assert!(parse_codex_line(&other.to_string(), NOW).is_none());
        assert!(parse_codex_line("not json at all", NOW).is_none());
    }

    #[test]
    fn codex_odd_window_lengths_get_a_duration_label() {
        let mut limits = live_limits(10.0);
        limits["primary"]["window_minutes"] = json!(1440);
        limits["secondary"]["window_minutes"] = json!(120);
        let r = parse_codex_line(&codex_line(limits), NOW).unwrap();
        assert_eq!(r.windows[0].kind, UsageWindowKind::Other);
        assert_eq!(r.windows[0].label, "1-day limit");
        assert_eq!(r.windows[1].label, "2-hour limit");
    }

    #[test]
    fn codex_scan_skips_files_without_limits_and_takes_the_last_line() {
        let dir = tempfile::TempDir::new().unwrap();
        let sessions = dir.path().join("sessions");
        let older = sessions.join("2026").join("10").join("04");
        let newer = sessions.join("2026").join("10").join("05");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::create_dir_all(&newer).unwrap();

        let with_limits = format!(
            "{}\n{{\"type\":\"session_meta\"}}\n{}\n",
            codex_line(live_limits(20.0)),
            codex_line(live_limits(55.0))
        );
        std::fs::write(older.join("rollout-2026-10-04T10-00-00-a.jsonl"), with_limits).unwrap();
        // Written after the older file, so it is checked first, and its
        // limits are null the way the desktop app writes them.
        std::fs::write(
            newer.join("rollout-2026-10-05T10-00-00-b.jsonl"),
            format!("{}\n", codex_line(Value::Null)),
        )
        .unwrap();
        std::fs::write(newer.join("notes.txt"), codex_line(live_limits(99.0))).unwrap();

        let r = scan_codex_sessions(&sessions, NOW).expect("finds the older file");
        assert_eq!(r.windows[0].used_percent, 55.0);
    }

    #[test]
    fn codex_scan_of_a_missing_folder_finds_nothing() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(scan_codex_sessions(&dir.path().join("sessions"), NOW).is_none());
    }

    #[test]
    fn claude_usage_maps_windows_and_skips_nulls() {
        let body = json!({
            "five_hour": { "utilization": 37.0, "resets_at": "2026-10-06T05:00:00.123456+00:00" },
            "seven_day": { "utilization": 12.0, "resets_at": null },
            "seven_day_opus": null,
            "seven_day_sonnet": { "utilization": 150.0, "resets_at": 1_791_300_000_000_i64 }
        });
        let w = parse_claude_usage(&body);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].kind, UsageWindowKind::FiveHour);
        assert_eq!(w[0].used_percent, 37.0);
        assert_eq!(w[0].resets_at.as_deref().and_then(unix_from_rfc3339), Some(NOW + 5 * 3600));
        assert_eq!(w[1].label, "Weekly limit");
        assert_eq!(w[1].resets_at, None);
        assert_eq!(w[2].label, "Weekly Sonnet limit");
        assert_eq!(w[2].used_percent, 100.0);
        assert_eq!(w[2].resets_at, rfc3339_from_unix(1_791_300_000));
    }

    #[test]
    fn claude_fractions_are_scaled_but_small_percents_are_not() {
        let fractions = json!({
            "five_hour": { "utilization": 0.25, "resets_at": 1_791_300_000 },
            "seven_day": { "utilization": 1.0, "resets_at": null }
        });
        let w = parse_claude_usage(&fractions);
        assert_eq!(w[0].used_percent, 25.0);
        assert_eq!(w[1].used_percent, 100.0);
        assert_eq!(w[0].resets_at, rfc3339_from_unix(1_791_300_000));

        let percents = json!({
            "five_hour": { "utilization": 0.0, "resets_at": null },
            "seven_day": { "utilization": 1.0, "resets_at": null }
        });
        let w = parse_claude_usage(&percents);
        assert_eq!(w[1].used_percent, 1.0);
    }

    #[test]
    fn claude_credentials_give_plan_and_expiry() {
        let raw = json!({
            "claudeAiOauth": {
                "accessToken": "fixture",
                "expiresAt": 1_791_244_800_000_i64,
                "subscriptionType": "max",
                "rateLimitTier": "default_claude_max_20x"
            }
        })
        .to_string();
        let c = parse_claude_credentials(&raw).unwrap();
        assert!(c.access_token.is_some());
        assert_eq!(c.expires_at_ms, Some(1_791_244_800_000.0));
        assert_eq!(c.plan.as_deref(), Some("Max 20x"));
        assert_eq!(claude_plan(Some("pro"), Some("default_claude_ai")).as_deref(), Some("Pro"));
        assert!(parse_claude_credentials("{}").is_none());
    }

    fn snapshot(entitlement: i64, used: i64, remaining: f64, unlimited: bool) -> AccountQuotaSnapshot {
        AccountQuotaSnapshot {
            entitlement_requests: entitlement,
            is_unlimited_entitlement: unlimited,
            used_requests: used,
            remaining_percentage: remaining,
            reset_date: Some("2026-11-01T00:00:00Z".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn copilot_premium_snapshot_becomes_a_monthly_window() {
        let w = copilot_window("Premium requests", &snapshot(300, 212, 29.333, false));
        assert_eq!(w.kind, UsageWindowKind::Monthly);
        assert!((w.used_percent - 70.667).abs() < 0.01);
        assert_eq!(w.detail.as_deref(), Some("212 of 300 used"));
        assert_eq!(w.resets_at.as_deref(), Some("2026-11-01T00:00:00Z"));

        let mut over = snapshot(300, 300, 0.0, false);
        over.overage = 12.0;
        assert_eq!(
            copilot_window("Premium requests", &over).detail.as_deref(),
            Some("300 of 300 used, plus 12 extra")
        );
    }

    #[test]
    fn a_reset_time_from_before_the_reading_is_not_a_rollover() {
        let mut w = UsageWindow {
            label: "Chat messages".into(),
            kind: UsageWindowKind::Monthly,
            used_percent: 2.0,
            resets_at: rfc3339_from_unix(NOW - 60),
            detail: Some("4 of 200 used".into()),
        };
        roll_over(&mut w, NOW, Some(NOW - 30));
        assert_eq!(w.used_percent, 2.0, "the numbers are kept");
        assert_eq!(w.detail.as_deref(), Some("4 of 200 used"));
        assert!(w.resets_at.is_none(), "a meaningless reset time is dropped");

        let mut stale = UsageWindow { resets_at: rfc3339_from_unix(NOW - 60), ..w.clone() };
        stale.used_percent = 90.0;
        roll_over(&mut stale, NOW, Some(NOW - 3_600));
        assert_eq!(stale.used_percent, 0.0, "a reset after an old reading is a real rollover");
    }

    #[test]
    fn counts_read_with_thousands_separators() {
        assert_eq!(thousands(2000), "2,000");
        assert_eq!(thousands(12), "12");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn copilot_free_plan_hides_premium_requests_it_does_not_have() {
        let mut snaps = HashMap::new();
        snaps.insert("premium_interactions".to_string(), snapshot(0, 0, 0.0, false));
        snaps.insert("chat".to_string(), snapshot(200, 4, 97.9, false));
        let w = copilot_windows(&snaps);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].label, "Chat messages");
    }

    #[test]
    fn copilot_shows_unlimited_premium_and_hides_unlimited_chat() {
        let mut snaps = HashMap::new();
        snaps.insert("premium_interactions".to_string(), snapshot(-1, 40, 100.0, true));
        snaps.insert("chat".to_string(), snapshot(-1, 0, 100.0, true));
        snaps.insert("completions".to_string(), snapshot(2000, 500, 75.0, false));
        let w = copilot_windows(&snaps);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].label, "Premium requests");
        assert_eq!(w[0].used_percent, 0.0);
        assert_eq!(w[0].detail.as_deref(), Some("Unlimited"));
        assert_eq!(w[1].label, "Code completions");
        assert_eq!(w[1].used_percent, 25.0);
    }

    #[test]
    fn date_only_reset_reads_as_midnight_utc() {
        assert_eq!(
            flexible_time(&json!("2026-11-01")).as_deref(),
            Some("2026-11-01T00:00:00Z")
        );
    }
}
