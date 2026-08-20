//! Per-adapter import bookkeeping: which external sessions/messages have
//! already been imported (task 2.3, dedup) and an incremental refresh cursor
//! (task 3.7 / architecture.md's `imports/<adapter-id>.json`).
//!
//! This file lives entirely under GitWyrm's own app-data directory
//! (`<store-root>/imports/<adapter-id>.json`, a sibling of `sessions/` --
//! see [`super::store::SessionStoreRoot`]) -- **never** inside a detected
//! external client's directory. That placement is itself part of the
//! read-only guarantee: GitWyrm-side state about an import has nowhere to
//! go except GitWyrm's own storage.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::store::SessionStoreRoot;

/// One external session already imported: which GitWyrm session it became,
/// and the newest external message ID seen so far (dedup/incremental-refresh
/// anchor -- a re-scan only appends messages *after* this one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSessionRecord {
    pub gitwyrm_session_id: String,
    pub last_imported_external_message_id: String,
    /// RFC 3339 UTC timestamp of the external session's own `updated_at` as
    /// of the last import -- used to skip a session entirely on re-scan when
    /// the external client reports no change at all, without even opening
    /// it.
    pub last_seen_external_updated_at: String,
}

/// One adapter's whole import ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterImportLedger {
    /// Keyed by external session ID.
    pub sessions: BTreeMap<String, ImportedSessionRecord>,
}

fn imports_dir(root: &SessionStoreRoot) -> PathBuf {
    root.root_path().join("imports")
}

fn ledger_path(root: &SessionStoreRoot, adapter_id: &str) -> PathBuf {
    imports_dir(root).join(format!("{adapter_id}.json"))
}

/// Read one adapter's ledger. A missing or unreadable file is treated as an
/// empty ledger -- there is nothing to lose by rebuilding it from scratch on
/// the next import, and refusing to scan because of a damaged bookkeeping
/// file would be strictly worse than re-importing (which dedups against the
/// GitWyrm session store itself as a second line of defense, not just this
/// ledger).
pub fn read_ledger(root: &SessionStoreRoot, adapter_id: &str) -> AdapterImportLedger {
    let path = ledger_path(root, adapter_id);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return AdapterImportLedger::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerWriteError {
    #[error("could not serialize import ledger: {0}")]
    Serialize(String),
    #[error("could not write import ledger: {0}")]
    Write(String),
}

/// Write one adapter's ledger atomically (temp file + rename), same
/// discipline as `store::write_atomic` -- a crash mid-write must never
/// corrupt the previous, still-good ledger.
pub fn write_ledger(
    root: &SessionStoreRoot,
    adapter_id: &str,
    ledger: &AdapterImportLedger,
) -> Result<(), LedgerWriteError> {
    let dir = imports_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| LedgerWriteError::Write(e.to_string()))?;
    let path = ledger_path(root, adapter_id);
    let json = serde_json::to_vec_pretty(ledger)
        .map_err(|e| LedgerWriteError::Serialize(e.to_string()))?;

    let mut temp =
        NamedTempFile::new_in(&dir).map_err(|e| LedgerWriteError::Write(e.to_string()))?;
    temp.write_all(&json)
        .map_err(|e| LedgerWriteError::Write(e.to_string()))?;
    temp.as_file_mut()
        .sync_all()
        .map_err(|e| LedgerWriteError::Write(e.to_string()))?;
    temp.persist(&path)
        .map_err(|e| LedgerWriteError::Write(e.error.to_string()))?;
    Ok(())
}

/// Whether a given external message ID has already been imported for this
/// external session, per the ledger's dedup anchor. This is a *cheap* check
/// used only to short-circuit obviously-already-imported sessions during a
/// scan; the authoritative dedup for an individual message happens against
/// the actual `SessionMessage::import` provenance already written into the
/// GitWyrm session (see `commands::agent_import::merge_new_messages`), so a
/// stale or lost ledger degrades to "re-checks messages it already has,"
/// never to "silently duplicates."
pub fn already_imported_session<'a>(
    ledger: &'a AdapterImportLedger,
    external_session_id: &str,
) -> Option<&'a ImportedSessionRecord> {
    ledger.sessions.get(external_session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_root() -> (TempDir, SessionStoreRoot) {
        let dir = TempDir::new().unwrap();
        let root = SessionStoreRoot::at(dir.path().join("agent-desk").join("v1")).unwrap();
        (dir, root)
    }

    #[test]
    fn a_missing_ledger_reads_as_empty() {
        let (_dir, root) = temp_root();
        let ledger = read_ledger(&root, "codex");
        assert!(ledger.sessions.is_empty());
    }

    #[test]
    fn a_written_ledger_reads_back_identical() {
        let (_dir, root) = temp_root();
        let mut ledger = AdapterImportLedger::default();
        ledger.sessions.insert(
            "ext-1".into(),
            ImportedSessionRecord {
                gitwyrm_session_id: "sess-1".into(),
                last_imported_external_message_id: "msg-3".into(),
                last_seen_external_updated_at: "2026-01-01T00:00:00Z".into(),
            },
        );
        write_ledger(&root, "codex", &ledger).unwrap();
        let back = read_ledger(&root, "codex");
        assert_eq!(back, ledger);
    }

    #[test]
    fn a_corrupt_ledger_file_reads_as_empty_rather_than_failing_the_scan() {
        let (_dir, root) = temp_root();
        std::fs::create_dir_all(imports_dir(&root)).unwrap();
        std::fs::write(imports_dir(&root).join("codex.json"), b"not json").unwrap();
        let ledger = read_ledger(&root, "codex");
        assert!(ledger.sessions.is_empty());
    }

    #[test]
    fn ledgers_for_different_adapters_do_not_collide() {
        let (_dir, root) = temp_root();
        let mut codex_ledger = AdapterImportLedger::default();
        codex_ledger.sessions.insert(
            "ext-1".into(),
            ImportedSessionRecord {
                gitwyrm_session_id: "sess-codex".into(),
                last_imported_external_message_id: "m1".into(),
                last_seen_external_updated_at: "2026-01-01T00:00:00Z".into(),
            },
        );
        write_ledger(&root, "codex", &codex_ledger).unwrap();

        let claude_ledger = read_ledger(&root, "claude-code");
        assert!(claude_ledger.sessions.is_empty());
        let codex_back = read_ledger(&root, "codex");
        assert_eq!(codex_back, codex_ledger);
    }
}
