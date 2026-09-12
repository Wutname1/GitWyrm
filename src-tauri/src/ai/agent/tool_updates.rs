//! Whether a newer release of an agent tool exists.
//!
//! Deliberately a different question from `CliState::TooOld`, and kept
//! separate everywhere. "Too old to work" is a refusal: GitWyrm has measured a
//! floor and will not drive the tool below it. "A newer one exists" is an FYI:
//! the tool works fine, and somebody may want to know anyway. Collapsing the
//! two would either nag people about working installs or quietly block them
//! over a version nobody has tested.
//!
//! The gap this closes: a codex-cli five releases behind reported installed,
//! not too old, and perfectly healthy -- while serving a model list three
//! generations stale. Nothing asked the only question that would have caught
//! it.
//!
//! Unknown stays unknown. A check that could not run answers
//! [`UpdateCheck::NotChecked`], never "up to date" -- the same rule
//! `UsageSource` follows for usage figures and the frontend enforces
//! mechanically for absences.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::registry::AgentSpec;

/// What GitWyrm knows about whether a tool could be newer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateCheck {
    /// The installed version is the newest published one.
    UpToDate,
    /// A newer release exists. `latest` is what the source published.
    ///
    /// Never a demand: the tool keeps working, and GitWyrm never updates it.
    NewerAvailable { latest: String },
    /// GitWyrm could not find out.
    ///
    /// Offline, the source refused, the tool prints a version nothing can
    /// read, or nobody has said where its releases are published. All of them
    /// mean the same thing to a reader -- nobody looked -- and none of them
    /// mean the tool is current.
    NotChecked,
}

/// npm's own "what is the newest published version" endpoint.
///
/// Chosen over scraping or over running each tool's updater: it is one small
/// JSON document per package, it is the registry every one of these tools
/// publishes to, and asking it changes nothing on the user's machine.
fn latest_url(package: &str) -> String {
    format!("https://registry.npmjs.org/{package}/latest")
}

/// Long enough that opening the picker repeatedly costs nothing, short enough
/// that somebody who updates a tool is not told the old answer all day.
const TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// A slow network must never hold up a picker.
const TIMEOUT: Duration = Duration::from_secs(6);

struct Cached {
    check: UpdateCheck,
    at: Instant,
}

static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Cached>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drops what was remembered, so the next read asks again.
pub fn forget_all_cached() {
    if let Ok(mut guard) = cache().lock() {
        guard.clear();
    }
}

/// Compares two versions, newest-wins, or `None` when either cannot be read.
///
/// Tuple comparison rather than a semver crate: these are the same
/// `major.minor.patch` triples `copilot_cli::parse_version` already pulls out
/// of four different version strings, and a pre-release suffix is not
/// something any of these tools has printed.
fn is_newer(latest: (u32, u32, u32), installed: (u32, u32, u32)) -> bool {
    latest > installed
}

/// Decides the answer from the two versions, with no network involved.
///
/// Split out so the decision is testable without reaching anything: every
/// state below is reachable in a unit test, which is the part that has to stay
/// honest.
pub(crate) fn compare(installed_raw: &str, published_raw: Option<&str>) -> UpdateCheck {
    let Some(published_raw) = published_raw else {
        return UpdateCheck::NotChecked;
    };
    let (Some(installed), Some(latest)) = (
        super::copilot_cli::parse_version(installed_raw),
        super::copilot_cli::parse_version(published_raw),
    ) else {
        // One of them is a string nothing can read. Saying "up to date" here
        // would be a claim about a comparison that never happened.
        return UpdateCheck::NotChecked;
    };
    if is_newer(latest, installed) {
        UpdateCheck::NewerAvailable { latest: published_raw.trim().to_string() }
    } else {
        UpdateCheck::UpToDate
    }
}

/// Whether a newer release of this tool exists.
///
/// `installed_version` is whatever the tool printed; `None` means it was not
/// asked or did not answer, which is already "not checked".
///
/// Only a real answer is remembered. A failure is retried next time, for the
/// reason `copilot_cli` never caches "not installed": somebody who fixes the
/// thing that was broken must not keep reading the old answer.
pub async fn check(spec: &'static AgentSpec, installed_version: Option<&str>) -> UpdateCheck {
    let (Some(package), Some(installed)) = (spec.release_package, installed_version) else {
        return UpdateCheck::NotChecked;
    };

    if let Ok(guard) = cache().lock() {
        if let Some(hit) = guard.get(package) {
            if hit.at.elapsed() < TTL {
                return hit.check.clone();
            }
        }
    }

    let published = match fetch_published(package).await {
        Ok(v) => v,
        Err(detail) => {
            log::info!("update check: could not ask about {package} ({detail})");
            return UpdateCheck::NotChecked;
        }
    };

    let check = compare(installed, Some(&published));
    if check != UpdateCheck::NotChecked {
        if let Ok(mut guard) = cache().lock() {
            guard.insert(package.to_string(), Cached { check: check.clone(), at: Instant::now() });
        }
    }
    check
}

/// One small GET, or a sentence saying why there was not one.
async fn fetch_published(package: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .get(latest_url(package))
        .header("User-Agent", concat!("GitWyrm/", env!("CARGO_PKG_VERSION")))
        // Plain JSON. The registry's abbreviated `install-v1+json` type is
        // only offered on the whole-package document, and asking for it here
        // is answered 406 -- which the live test caught, having been written
        // to prove the round trip rather than to assume it. This endpoint
        // already returns one small record.
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("the source answered {}", res.status()));
    }
    let body = res.text().await.map_err(|e| e.to_string())?;
    let doc: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    doc.get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "the source named no version".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::agent::registry;

    /// The three states, decided without touching the network.
    ///
    /// The version strings are the real shapes these tools print, not tidy
    /// ones: `copilot version` says "GitHub Copilot CLI 1.0.82", `claude
    /// --version` says "2.1.260 (Claude Code)", `codex --version` says
    /// "codex-cli 0.154.0", and opencode prints a bare number.
    #[test]
    fn a_newer_release_is_reported_as_available() {
        assert_eq!(
            compare("codex-cli 0.154.0", Some("0.160.0")),
            UpdateCheck::NewerAvailable { latest: "0.160.0".into() }
        );
        // The case that started this: five builds behind, and every existing
        // signal said the install was healthy.
        assert_eq!(
            compare("codex-cli 0.149.0", Some("0.154.0")),
            UpdateCheck::NewerAvailable { latest: "0.154.0".into() }
        );
    }

    #[test]
    fn the_newest_installed_version_is_up_to_date() {
        assert_eq!(compare("GitHub Copilot CLI 1.0.83", Some("1.0.83")), UpdateCheck::UpToDate);
        assert_eq!(compare("2.1.260 (Claude Code)", Some("2.1.260")), UpdateCheck::UpToDate);
    }

    /// Ahead of the registry is not behind it. A person running a build newer
    /// than the published one must not be told to update to an older release.
    #[test]
    fn a_version_ahead_of_the_published_one_is_not_behind() {
        assert_eq!(compare("1.18.30", Some("1.18.10")), UpdateCheck::UpToDate);
    }

    /// The rule this repo enforces everywhere: a check that could not run says
    /// so. Reading "up to date" here would be a claim about a comparison that
    /// never happened.
    #[test]
    fn a_check_that_could_not_run_is_not_up_to_date() {
        // Nobody answered about the newest release.
        assert_eq!(compare("codex-cli 0.154.0", None), UpdateCheck::NotChecked);
        // The tool printed something with no version in it.
        assert_eq!(compare("unknown", Some("0.154.0")), UpdateCheck::NotChecked);
        // The source answered with something unreadable.
        assert_eq!(compare("codex-cli 0.154.0", Some("latest")), UpdateCheck::NotChecked);
    }

    /// A tool with nowhere to ask is unchecked, and is never asked over the
    /// network at all -- so this needs no connection to pass.
    #[tokio::test]
    async fn a_tool_with_no_release_source_is_never_asked() {
        let spec = registry::find("codex").expect("codex row");
        assert_eq!(check(spec, None).await, UpdateCheck::NotChecked);
    }

    /// Every shipped tool names where its releases are published, and names
    /// the package that ships the binary GitWyrm actually probes.
    ///
    /// Claude Code is the one that can go wrong: its install hint is the ACP
    /// adapter (`@agentclientprotocol/claude-agent-acp`, 0.76.x) while the
    /// binary asked for a version is `claude` (Claude Code, 2.1.x). Comparing
    /// against the hint's package would report a current install as many
    /// majors behind.
    #[test]
    fn every_tool_names_where_its_releases_come_from() {
        for spec in registry::AGENTS.iter() {
            assert!(
                spec.release_package.is_some(),
                "{} has nowhere to check for a newer release",
                spec.id
            );
        }
        let claude = registry::find("claude").expect("claude row");
        assert_eq!(claude.release_package, Some("@anthropic-ai/claude-code"));
        assert!(
            !claude.install_hint.contains("@anthropic-ai/claude-code"),
            "this test is only meaningful while the hint and the release package differ"
        );
    }

    /// What the picker will actually say on this machine, for each installed
    /// tool. Ignored by default; run with
    /// `cargo test --lib real_update_states -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn real_update_states_on_this_machine() {
        use crate::ai::agent::copilot_cli::{detect_agent, CliState};
        for spec in registry::AGENTS.iter() {
            forget_all_cached();
            let installed = match detect_agent(spec).state {
                CliState::Ready { version, .. } => Some(version),
                _ => None,
            };
            let got = check(spec, installed.as_deref()).await;
            println!("{:<10} installed={:<28} -> {:?}", spec.id, installed.unwrap_or_else(|| "-".into()), got);
        }
    }

    /// Against the real registry. Ignored by default so CI and offline
    /// machines are not gated on a network round trip; run with
    /// `cargo test --lib tool_updates -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn the_real_registry_answers_for_every_shipped_tool() {
        for spec in registry::AGENTS.iter() {
            forget_all_cached();
            // A version that cannot exist, so a reachable source must answer
            // "newer available" -- proving the round trip, not the comparison.
            let got = check(spec, Some("0.0.1")).await;
            println!("{:<10} {:?}", spec.id, got);
            assert!(
                matches!(got, UpdateCheck::NewerAvailable { .. }),
                "{} could not be checked against {:?}",
                spec.id,
                spec.release_package
            );
        }
    }
}
