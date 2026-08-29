//! Listing the AI tools GitWyrm can drive, for the picker.
//!
//! One command. It answers the three things a picker has to show for each
//! tool: what it is called, whether it is actually installed right now, and
//! whether it can be trusted with work that must not change anything.
//!
//! The last one is the reason this exists as a real command rather than a
//! hardcoded list in the frontend. A tool that cannot be told to refuse a
//! write is refused for read-only work (see `ai::agent::select`), and a picker
//! that offered it anyway would let the user choose an option that then fails
//! at launch. Better to show it greyed out with the reason attached.

use crate::ai::agent::copilot_cli::{detect_agent, CliState};
use crate::ai::agent::registry::{self, Denial};

/// One row in the provider picker.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentProvider {
    /// Stable id, the value to pass back as a provider override.
    pub id: String,
    /// What to show the user. The tool's own spelling of its name.
    pub display_name: String,
    /// Whether this is the tool used when nothing is chosen.
    pub is_default: bool,
    /// Installed and new enough to drive right now.
    pub installed: bool,
    /// The version string the tool reported, when it is installed.
    pub version: Option<String>,
    /// Set when the tool was found but is older than the floor GitWyrm has
    /// checked against. Distinct from not installed: updating fixes it.
    pub too_old: bool,
    /// Whether this tool can be told to leave files alone, which decides
    /// whether it may run Ask, Explain, Review, Summarize, or a Plan before
    /// Start.
    pub can_do_read_only_work: bool,
    /// Why it cannot, in words the picker can show directly. `None` when it
    /// can.
    pub read_only_limit: Option<String>,
}

/// Every tool this build knows how to drive, with its current install state.
///
/// Probing is per-tool and cached only when a tool is found, so a user who
/// installs one while the picker is open sees it appear on the next open
/// rather than after a restart.
#[tauri::command]
#[specta::specta]
pub async fn agent_providers_list() -> Result<Vec<AgentProvider>, crate::error::AppError> {
    tauri::async_runtime::spawn_blocking(list)
        .await
        .map_err(|e| crate::error::AppError::Other(e.to_string()))
}

fn list() -> Vec<AgentProvider> {
    registry::AGENTS.iter().map(row).collect()
}

fn row(spec: &'static registry::AgentSpec) -> AgentProvider {
    let probe = detect_agent(spec);
    let (installed, version, too_old) = match &probe.state {
        CliState::Ready { version, .. } => (true, Some(short_version(version)), false),
        CliState::TooOld { version, .. } => (false, Some(short_version(version)), true),
        CliState::NotFound => (false, None, false),
    };

    AgentProvider {
        id: spec.id.to_string(),
        display_name: spec.display_name.to_string(),
        is_default: spec.id == registry::DEFAULT_AGENT_ID,
        installed,
        version,
        too_old,
        can_do_read_only_work: spec.can_guarantee_read_only(),
        read_only_limit: read_only_limit(spec),
    }
}

/// The first line of whatever the tool printed, trimmed.
///
/// These tools do not agree on how much to say. `opencode --version` prints
/// `1.18.10` and nothing else, while `copilot version` prints its name, its
/// version, a blank line, and a sentence about whether an update is available.
/// The picker shows this in a small monospace note beside the tool's name, so
/// the whole answer would wrap into a paragraph there.
///
/// Only the first line is kept, and only up to a sane width: everything after
/// it is advice for a terminal, not an identity. Nothing is parsed out of it
/// -- the string is the tool's to format, and guessing at a version number
/// inside it would break the moment one of them reworded its output.
fn short_version(raw: &str) -> String {
    let first = raw.lines().next().unwrap_or("").trim();
    const MAX: usize = 40;
    if first.chars().count() <= MAX {
        return first.to_string();
    }
    let cut: String = first.chars().take(MAX - 1).collect();
    format!("{}…", cut.trim_end())
}

/// The sentence explaining a read-only limit, or `None` when there is none.
///
/// Written for the picker rather than reused from `select::refuse_read_only`:
/// that one is an error shown after a refusal ("pick a different tool"), while
/// this is a label on the option itself, where telling the user to pick
/// something else would be redundant with the list they are looking at.
fn read_only_limit(spec: &registry::AgentSpec) -> Option<String> {
    match spec.denial {
        Denial::None => Some(format!(
            "{} has no way to be told to leave your files alone, so it cannot be used for \
             chats that are only supposed to look and not change anything.",
            spec.display_name
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_tool_appears_once() {
        let rows = list();
        assert_eq!(rows.len(), registry::AGENTS.len());
        for spec in registry::AGENTS {
            assert!(
                rows.iter().any(|r| r.id == spec.id),
                "{} is missing from the picker",
                spec.id
            );
        }
    }

    #[test]
    fn exactly_one_tool_is_the_default() {
        let rows = list();
        assert_eq!(rows.iter().filter(|r| r.is_default).count(), 1);
        assert!(rows.iter().any(|r| r.is_default && r.id == "copilot"));
    }

    #[test]
    fn a_tool_that_cannot_refuse_a_write_says_so_in_words() {
        // opencode is the one in the table today. The picker has to be able
        // to explain the limit at the point of choosing, not only after a
        // refusal at launch.
        let rows = list();
        let opencode = rows.iter().find(|r| r.id == "opencode").expect("opencode is listed");
        assert!(!opencode.can_do_read_only_work);
        let limit = opencode.read_only_limit.as_ref().expect("the limit must be explained");
        assert!(limit.contains("opencode"), "the sentence must name the tool: {limit}");
        assert!(!limit.is_empty());
    }

    #[test]
    fn a_tool_that_can_refuse_a_write_carries_no_limit_text() {
        let rows = list();
        for id in ["copilot", "gemini", "claude"] {
            let row = rows.iter().find(|r| r.id == id).expect("listed");
            assert!(row.can_do_read_only_work, "{id} can enforce read-only");
            assert!(
                row.read_only_limit.is_none(),
                "{id} must not carry a limit sentence"
            );
        }
    }

    #[test]
    fn a_chatty_version_string_is_cut_down_to_one_line() {
        // Copilot really does answer with three lines, the last of which is
        // advice about updating. The picker shows this in a small monospace
        // note, so the whole thing would wrap into a paragraph there.
        let raw = "GitHub Copilot CLI 1.0.81\n\nYou are running the latest version.";
        let short = short_version(raw);
        assert_eq!(short, "GitHub Copilot CLI 1.0.81");
        assert!(!short.contains('\n'));
    }

    #[test]
    fn a_short_version_is_left_exactly_as_the_tool_wrote_it() {
        // opencode answers with a bare number. Nothing is parsed or
        // reformatted: the string belongs to the tool.
        assert_eq!(short_version("1.18.10"), "1.18.10");
        assert_eq!(short_version("  1.18.10  "), "1.18.10");
    }

    #[test]
    fn a_very_long_first_line_is_truncated_rather_than_wrapped() {
        let long = "x".repeat(200);
        let short = short_version(&long);
        assert!(short.chars().count() <= 40, "got {} chars", short.chars().count());
        assert!(short.ends_with('…'));
    }

    #[test]
    fn a_tool_that_printed_nothing_does_not_panic() {
        assert_eq!(short_version(""), "");
        assert_eq!(short_version("

"), "");
    }

    #[test]
    fn a_missing_tool_is_reported_as_not_installed_rather_than_too_old() {
        // The two states need different words -- one asks for an install, the
        // other for an update -- so they must never collapse into each other.
        let rows = list();
        for row in &rows {
            assert!(
                !(row.installed && row.too_old),
                "{} cannot be both usable and too old",
                row.id
            );
            if row.too_old {
                assert!(row.version.is_some(), "{} must report what it found", row.id);
            }
        }
    }
}

/// Prints the real picker rows for this machine.
///
/// Ignored by default because it spawns every installed tool and its answer
/// depends on what is on the box. Run it deliberately with
/// `cargo test --lib real_picker_rows -- --ignored --nocapture` to see exactly
/// what the picker will show before trusting a screenshot.
#[cfg(test)]
#[test]
#[ignore]
fn real_picker_rows_on_this_machine() {
    for row in list() {
        println!(
            "{:<16} installed={:<5} tooOld={:<5} readOnlyOk={:<5} version={:?}",
            row.id, row.installed, row.too_old, row.can_do_read_only_work, row.version
        );
        if let Some(limit) = &row.read_only_limit {
            println!("{:<16}   limit: {limit}", "");
        }
    }
}
