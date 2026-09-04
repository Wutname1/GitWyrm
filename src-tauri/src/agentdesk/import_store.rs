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
    read_ledger_checked(root, adapter_id).unwrap_or_default()
}

/// The ledger, or why it could not be read.
///
/// `read_ledger` answers the same question with a default, which is right for
/// a file that is simply absent -- nothing has been imported for that tool yet
/// -- and wrong for one that exists and cannot be parsed. Every write path is
/// a read-modify-write, so treating a damaged ledger as empty meant the next
/// import wrote an EMPTY ledger over it: every record of what had already been
/// brought in, gone, with nothing said. Callers that are about to WRITE must
/// use this one and refuse rather than overwrite.
pub fn read_ledger_checked(
    root: &SessionStoreRoot,
    adapter_id: &str,
) -> Result<AdapterImportLedger, LedgerReadError> {
    let path = ledger_path(root, adapter_id);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        // Absent is a real answer: nothing has been imported for this tool.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AdapterImportLedger::default())
        }
        Err(e) => {
            return Err(LedgerReadError::Io {
                detail: e.to_string(),
            })
        }
    };
    serde_json::from_str(&raw).map_err(|e| LedgerReadError::Parse {
        detail: e.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerReadError {
    #[error("could not read import ledger: {detail}")]
    Io { detail: String },
    #[error("import ledger is damaged: {detail}")]
    Parse { detail: String },
}

impl LedgerReadError {
    fn detail(&self) -> &str {
        match self {
            LedgerReadError::Io { detail } | LedgerReadError::Parse { detail } => detail,
        }
    }
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
/// stale or lost ledger degrades to "re-checks messages it already has"
/// -- **for a session already on the ledger**.
///
/// A FIRST import has no such fallback. The create path
/// (`commands::agent_import`, the `None` arm) consults only this ledger and
/// then builds a session with a fresh id, never reading the one already on
/// disk -- so a lost write there does duplicate the whole chat. That is why
/// the create path appends `LEDGER_NOT_SAVED_NOTE` to the session when this
/// ledger cannot be saved.
///
/// The qualification is here rather than only at the call site because this
/// doc is where a reader learns the contract, and an unqualified "never
/// silently duplicates" would make that mitigation look like unnecessary
/// belt-and-braces worth deleting.
pub fn already_imported_session<'a>(
    ledger: &'a AdapterImportLedger,
    external_session_id: &str,
) -> Option<&'a ImportedSessionRecord> {
    ledger.sessions.get(external_session_id)
}

/// The link between one GitWyrm session and the external session it was
/// imported from, as the ledgers record it. Returned by
/// [`find_link_for_session`] so unlink (task 4.3) can name the adapter in UI
/// copy and remove exactly one ledger entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLink {
    pub adapter_id: String,
    pub external_session_id: String,
    pub record: ImportedSessionRecord,
}

/// Every adapter that has a ledger file on disk. Read from the directory
/// rather than the adapter registry so a ledger left behind by an adapter
/// that was later removed from the registry is still found (and can still
/// be unlinked) instead of becoming an orphan nobody can clear.
pub fn ledger_adapter_ids(root: &SessionStoreRoot) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(imports_dir(root)) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    ids.sort();
    ids
}

/// Find which external session (if any) a GitWyrm session was imported
/// from. Ledgers are keyed the other way round (external id -> GitWyrm id)
/// because import and re-scan look up by external id; unlink is the one
/// caller that starts from the GitWyrm side, and it is rare enough that a
/// linear scan over every ledger is fine.
pub fn find_link_for_session(root: &SessionStoreRoot, session_id: &str) -> Option<SessionLink> {
    for adapter_id in ledger_adapter_ids(root) {
        let ledger = read_ledger(root, &adapter_id);
        if let Some((external_session_id, record)) = ledger
            .sessions
            .iter()
            .find(|(_, r)| r.gitwyrm_session_id == session_id)
        {
            return Some(SessionLink {
                adapter_id,
                external_session_id: external_session_id.clone(),
                record: record.clone(),
            });
        }
    }
    None
}

/// Remove one link from an adapter's ledger. Returns `Ok(false)` when there
/// was nothing to remove so the caller can report "not linked" instead of
/// pretending something changed. Only the ledger is touched here: the
/// GitWyrm session and its imported messages are left exactly as they are.
pub fn remove_link(
    root: &SessionStoreRoot,
    adapter_id: &str,
    external_session_id: &str,
) -> Result<bool, LedgerWriteError> {
    // Checked, because this writes back what it reads. With the defaulting
    // read, a damaged ledger became an empty one and this call then wrote that
    // emptiness over it -- unlinking one chat would have silently discarded
    // every other import link for the tool.
    let mut ledger = read_ledger_checked(root, adapter_id).map_err(|e| LedgerWriteError::Write(e.detail().to_string()))?;
    if ledger.sessions.remove(external_session_id).is_none() {
        return Ok(false);
    }
    write_ledger(root, adapter_id, &ledger)?;
    Ok(true)
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

    /// A ledger that cannot be parsed must not read as "nothing imported".
    ///
    /// The ledger records which external chats have already been brought in.
    /// `read_ledger` returned `AdapterImportLedger::default()` for an
    /// unparseable file, and every write path is a read-modify-write -- so one
    /// damaged ledger meant the next import silently wrote an EMPTY ledger
    /// over it, permanently losing every link for that tool. Before that, the
    /// picker would offer already-imported chats as new.
    ///
    /// A file that is simply absent is genuinely empty, which is a different
    /// answer and stays a normal one.
    #[test]
    fn a_damaged_ledger_is_not_reported_as_nothing_imported() {
        let (_dir, root) = temp_root();
        let path = ledger_path(&root, "codex");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();

        assert!(
            read_ledger_checked(&root, "codex").is_err(),
            "an unparseable ledger must be reported, not treated as empty"
        );
    }

    /// The data-loss path, closed.
    ///
    /// `remove_link` is a read-modify-write. With the defaulting read, a
    /// damaged ledger became an empty one and this call wrote that emptiness
    /// back -- unlinking one chat silently discarded every other import link
    /// for that tool. It must refuse and leave the file exactly as it found it.
    #[test]
    fn unlinking_against_a_damaged_ledger_refuses_and_writes_nothing() {
        let (_dir, root) = temp_root();
        let path = ledger_path(&root, "codex");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();

        let result = remove_link(&root, "codex", "ext-1");
        assert!(result.is_err(), "must refuse rather than overwrite");

        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "{ not json", "the damaged file must be left untouched");
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

    fn record(session_id: &str) -> ImportedSessionRecord {
        ImportedSessionRecord {
            gitwyrm_session_id: session_id.into(),
            last_imported_external_message_id: "m1".into(),
            last_seen_external_updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn find_link_for_session_searches_every_adapter_ledger() {
        let (_dir, root) = temp_root();
        let mut codex = AdapterImportLedger::default();
        codex.sessions.insert("ext-codex".into(), record("sess-a"));
        write_ledger(&root, "codex", &codex).unwrap();
        let mut claude = AdapterImportLedger::default();
        claude.sessions.insert("ext-claude".into(), record("sess-b"));
        write_ledger(&root, "claude-code", &claude).unwrap();

        let link = find_link_for_session(&root, "sess-b").expect("sess-b is linked");
        assert_eq!(link.adapter_id, "claude-code");
        assert_eq!(link.external_session_id, "ext-claude");
        assert!(find_link_for_session(&root, "sess-none").is_none());
    }

    #[test]
    fn remove_link_drops_only_that_entry_and_reports_a_second_removal_as_absent() {
        let (_dir, root) = temp_root();
        let mut ledger = AdapterImportLedger::default();
        ledger.sessions.insert("ext-1".into(), record("sess-1"));
        ledger.sessions.insert("ext-2".into(), record("sess-2"));
        write_ledger(&root, "codex", &ledger).unwrap();

        assert!(remove_link(&root, "codex", "ext-1").unwrap());
        let back = read_ledger(&root, "codex");
        assert!(!back.sessions.contains_key("ext-1"));
        assert!(back.sessions.contains_key("ext-2"));

        assert!(!remove_link(&root, "codex", "ext-1").unwrap());
        assert!(!remove_link(&root, "never-written", "ext-1").unwrap());
    }
}
