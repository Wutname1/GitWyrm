//! What GitWyrm remembers about the agent tools between runs.
//!
//! Everything learnt about a tool -- that it is installed, what version it
//! printed, which models it offers, whether a newer release exists -- lived
//! only in memory and died with the process. So every launch spawned a version
//! probe per tool (measured at about four seconds for five tools), asked Codex
//! for its models over a fresh child process, and asked a package registry
//! about five packages, with the picker showing nothing until it finished. The
//! next launch then did all of it again, having learnt nothing.
//!
//! This writes that down so a relaunch can answer immediately and check again
//! quietly afterwards.
//!
//! **The identity rule.** A remembered answer is only about the install that
//! produced it. Stored alongside every entry are the executable's path and the
//! version string it printed, and a mismatch on either throws the entry away
//! rather than showing it. Without that, updating a tool would be the one
//! moment its remembered answer is most wrong and most trusted -- which is
//! precisely the staleness this area keeps producing.
//!
//! **Failure is never remembered.** Only a positive answer is written, for the
//! reason `copilot_cli` gives for never caching "not installed": somebody who
//! reads that a tool is missing, installs it, and comes back must not be told
//! the old answer until they restart.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One tool's remembered answer, and what install it was about.
///
/// `PartialEq` without `Eq` from here down: model prices are floats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remembered {
    /// Where the executable was when this was learnt.
    ///
    /// Part of the identity, not a convenience: a second install on the PATH
    /// is a different tool with its own version and its own models, and
    /// showing one's answer for the other would be wrong in a way nobody could
    /// see.
    pub path: String,
    /// Exactly what the tool printed when asked its version.
    pub version: String,
    /// The models it offered, when it was asked and answered.
    #[serde(default)]
    pub models: Vec<RememberedModel>,
    /// The newest published version, when that was learnt.
    ///
    /// Stored as what the source said rather than as a verdict, so the
    /// comparison is redone against the version installed now. A stored
    /// verdict would go stale the moment the tool is updated.
    #[serde(default)]
    pub latest_release: Option<String>,
    /// When this was written, as seconds since the epoch.
    ///
    /// `u32` deliberately: `specta` refuses 64-bit integers and silently drops
    /// the command that carries them. Nothing here crosses to TypeScript
    /// today, and this keeps it free to.
    pub written_at: u32,
}

/// One model, flattened for storage.
///
/// Not `CodexModel` itself: that type is what the live protocol produced and
/// is free to change with it, while this is a file format that has to keep
/// reading what an older GitWyrm wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberedModel {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub efforts: Vec<String>,
    #[serde(default)]
    pub default_effort: Option<String>,
    /// Premium-request multiplier, for tools that publish one (Copilot).
    ///
    /// The three price fields are `default` so a file written before they
    /// existed still loads; an older entry simply has no price to show.
    #[serde(default)]
    pub multiplier: Option<f32>,
    #[serde(default)]
    pub credits_per_million: Option<crate::ai::copilot_sdk::TokenCredits>,
    #[serde(default)]
    pub context_window: Option<u32>,
}

/// Everything remembered, keyed by agent id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    /// The format this file was written in.
    ///
    /// A file from a newer GitWyrm is ignored rather than guessed at: reading
    /// a shape that has moved on is how a cache starts answering confidently
    /// about something it does not understand.
    pub version: u32,
    #[serde(default)]
    pub tools: HashMap<String, Remembered>,
}

/// The only shape this build writes, and the only one it reads.
pub const FORMAT_VERSION: u32 = 1;

impl Memory {
    fn current() -> Self {
        Self { version: FORMAT_VERSION, tools: HashMap::new() }
    }
}

/// Whether a remembered answer is about the install in front of us now.
///
/// Both halves matter and neither is enough alone. The path catches a second
/// install taking over the PATH; the version catches an update in place, which
/// is the common case and the one that makes a remembered model list wrong.
pub fn is_about(entry: &Remembered, path: &str, version: &str) -> bool {
    // Windows paths differ in case without being different paths.
    entry.path.eq_ignore_ascii_case(path) && entry.version == version
}

/// Where the file lives.
pub fn memory_path(app_data: PathBuf) -> PathBuf {
    app_data.join("agent-desk").join("v1").join("tool-memory.json")
}

/// Reads what was written, or an empty memory.
///
/// Every failure answers empty rather than refusing: a memory that cannot be
/// read costs the four seconds it was there to save, while a hard failure
/// would cost the picker. The distinction that matters here is not
/// missing-vs-damaged but remembered-vs-not, and both of those are "not".
pub fn load(path: &std::path::Path) -> Memory {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Memory::current(),
        Err(e) => {
            log::info!("tool memory: could not read {}: {e}", path.display());
            return Memory::current();
        }
    };
    match serde_json::from_str::<Memory>(&raw) {
        Ok(m) if m.version == FORMAT_VERSION => m,
        Ok(m) => {
            log::info!(
                "tool memory: ignoring a file written in format {} (this build reads {FORMAT_VERSION})",
                m.version
            );
            Memory::current()
        }
        Err(e) => {
            log::info!("tool memory: could not read what was remembered ({e}); starting over");
            Memory::current()
        }
    }
}

/// Writes the memory, creating its directory.
///
/// A failure is logged and swallowed: nothing a person does should fail
/// because GitWyrm could not write a cache.
pub fn save(path: &std::path::Path, memory: &Memory) {
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::info!("tool memory: could not make {}: {e}", dir.display());
            return;
        }
    }
    if let Err(e) = crate::agentdesk::store::write_atomic(path, memory) {
        log::info!("tool memory: could not write {}: {e}", path.display());
    }
}

/// Throws away everything remembered, so the next run starts from nothing.
///
/// This is the other half of the Refresh button. Refresh already drops what is
/// held in memory, but the file outlives the process, so leaving it alone
/// meant the very next launch restored the answer the person had just asked
/// GitWyrm to forget -- a button that appeared to work and then undid itself
/// overnight.
///
/// A file that is already gone is a success: "there is nothing remembered" is
/// what was asked for, and both routes arrive at it.
pub fn forget_all(path: &std::path::Path) {
    match std::fs::remove_file(path) {
        Ok(()) => log::info!("tool memory: cleared {}", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::info!("tool memory: could not clear {}: {e}", path.display()),
    }
}

/// Seconds since the epoch, clamped into `u32`.
///
/// Only ever compared against itself to answer "how old is this", so the 2106
/// ceiling is not a correctness question -- and `specta` refuses the 64-bit
/// type that would avoid it.
pub fn now_secs() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, version: &str) -> Remembered {
        Remembered {
            path: path.to_string(),
            version: version.to_string(),
            models: vec![RememberedModel {
                id: "gpt-5.6-sol".into(),
                display_name: "GPT-5.6-Sol".into(),
                description: String::new(),
                is_default: false,
                efforts: vec!["low".into()],
                default_effort: None,
                multiplier: Some(0.33),
                credits_per_million: Some(crate::ai::copilot_sdk::TokenCredits {
                    input: 1_000.0,
                    cached_input: Some(100.0),
                    output: 5_000.0,
                }),
                context_window: Some(400_000),
            }],
            latest_release: Some("0.154.0".into()),
            written_at: 1_700_000_000,
        }
    }

    #[test]
    fn an_answer_about_this_exact_install_is_used() {
        let e = entry(r"C:\tools\codex.cmd", "codex-cli 0.154.0");
        assert!(is_about(&e, r"C:\tools\codex.cmd", "codex-cli 0.154.0"));
    }

    /// The case the whole identity rule exists for. Updating a tool is the
    /// moment its remembered model list is most wrong and would be most
    /// trusted -- a stale list surviving an update is exactly the fault this
    /// area keeps producing.
    #[test]
    fn an_answer_from_before_an_update_is_thrown_away() {
        let e = entry(r"C:\tools\codex.cmd", "codex-cli 0.149.0");
        assert!(!is_about(&e, r"C:\tools\codex.cmd", "codex-cli 0.154.0"));
    }

    /// A second install taking over the PATH is a different tool with its own
    /// version and its own models, even when the two happen to print the same
    /// version string.
    #[test]
    fn an_answer_about_a_different_executable_is_thrown_away() {
        let e = entry(r"C:\tools\codex.cmd", "codex-cli 0.154.0");
        assert!(!is_about(&e, r"D:\other\codex.cmd", "codex-cli 0.154.0"));
    }

    /// Windows paths differ in case without being different paths, and
    /// treating them as different would throw away every answer on the
    /// platform GitWyrm ships first.
    #[test]
    fn the_same_windows_path_in_a_different_case_is_the_same_path() {
        let e = entry(r"C:\Tools\Codex.cmd", "codex-cli 0.154.0");
        assert!(is_about(&e, r"c:\tools\codex.cmd", "codex-cli 0.154.0"));
    }

    #[test]
    fn a_missing_file_reads_as_nothing_remembered() {
        let dir = tempfile::tempdir().expect("temp dir");
        let m = load(&dir.path().join("does-not-exist.json"));
        assert_eq!(m.version, FORMAT_VERSION);
        assert!(m.tools.is_empty());
    }

    /// A damaged file costs the time it was there to save, and nothing more.
    /// It must never take the picker with it.
    #[test]
    fn a_damaged_file_reads_as_nothing_remembered_rather_than_failing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("tool-memory.json");
        std::fs::write(&path, "{ this is not json").expect("write");
        assert!(load(&path).tools.is_empty());
    }

    /// A file from a newer GitWyrm is ignored rather than guessed at.
    #[test]
    fn a_file_from_a_newer_build_is_ignored() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("tool-memory.json");
        let mut future = Memory::current();
        future.version = FORMAT_VERSION + 1;
        future.tools.insert("codex".into(), entry("x", "y"));
        save(&path, &future);
        assert!(load(&path).tools.is_empty(), "a shape this build does not know must not be read");
    }

    /// The built-in list must never reach this file.
    ///
    /// If it did, it would come back as the tool's own answer on the next run
    /// -- a hand-written list laundered into real provenance by a round trip
    /// through disk, which is exactly the fault `ModelSource` exists to make
    /// impossible. `remember_learned` enforces this by storing models only
    /// from a `Live` catalog; this pins the shape that enforcement produces.
    #[test]
    fn an_entry_with_no_models_is_still_useful_for_its_release_check() {
        let mut e = entry(r"C:\tools\codex.cmd", "codex-cli 0.154.0");
        e.models.clear();
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("tool-memory.json");
        let mut m = Memory::current();
        m.tools.insert("codex".into(), e);
        save(&path, &m);
        let back = load(&path);
        // No models remembered is a legal state, not a corrupt one: it is what
        // a tool GitWyrm could not ask for models but could check for a
        // release looks like.
        assert!(back.tools["codex"].models.is_empty());
        assert_eq!(back.tools["codex"].latest_release.as_deref(), Some("0.154.0"));
    }

    /// The Refresh button's other half. Dropping only the in-memory caches
    /// left the file behind, so the next launch restored the very answer the
    /// person had just asked GitWyrm to forget.
    #[test]
    fn clearing_leaves_nothing_to_restore() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("tool-memory.json");
        let mut m = Memory::current();
        m.tools.insert("codex".into(), entry(r"C:\tools\codex.cmd", "codex-cli 0.154.0"));
        save(&path, &m);
        assert!(!load(&path).tools.is_empty(), "the entry must be there to be cleared");

        forget_all(&path);

        assert!(!path.exists(), "the file itself must be gone, not just emptied");
        assert!(
            load(&path).tools.is_empty(),
            "a cleared memory must read as nothing remembered"
        );
    }

    /// Clearing what is already clear is what was asked for, not a failure.
    #[test]
    fn clearing_nothing_is_not_a_failure() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("never-written.json");
        forget_all(&path);
        assert!(load(&path).tools.is_empty());
    }

    #[test]
    fn what_is_written_is_what_comes_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("tool-memory.json");
        let mut m = Memory::current();
        m.tools.insert("codex".into(), entry(r"C:\tools\codex.cmd", "codex-cli 0.154.0"));
        save(&path, &m);
        let back = load(&path);
        assert_eq!(back, m);
        assert_eq!(back.tools["codex"].models[0].id, "gpt-5.6-sol");
    }

    /// A file written before models carried prices must still load, with the
    /// prices simply absent. Failing here would throw away every remembered
    /// answer on the first launch after an update.
    #[test]
    fn a_file_from_before_prices_still_loads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("tool-memory.json");
        let old = r#"{
            "version": 1,
            "tools": {
                "copilot": {
                    "path": "C:/tools/copilot.cmd",
                    "version": "GitHub Copilot CLI 1.0.81",
                    "models": [
                        { "id": "gpt-5.5", "displayName": "GPT-5.5", "isDefault": false, "efforts": ["low"] }
                    ],
                    "latestRelease": null,
                    "writtenAt": 1700000000
                }
            }
        }"#;
        std::fs::write(&path, old).expect("write");
        let back = load(&path);
        let model = &back.tools["copilot"].models[0];
        assert_eq!(model.id, "gpt-5.5");
        assert!(model.multiplier.is_none());
        assert!(model.credits_per_million.is_none());
        assert!(model.context_window.is_none());
    }
}
