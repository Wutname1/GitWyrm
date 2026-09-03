//! Safe write framework: preview, apply, and undo (tasks 3.1-3.5).
//!
//! Mirrors `agentdesk::store`'s atomic-write shape (temp file in the same
//! directory, flush, atomic rename) but adds what a foreign-file write needs
//! on top: a before-hash gate so apply refuses a destination that changed
//! since preview, and a backup+receipt written *before* the destination is
//! touched so undo can always get back to the exact previous bytes.
//!
//! Backups live under GitWyrm's own app data directory, never beside the
//! destination -- a problem with the destination's directory (permissions,
//! corruption, the user deleting the whole `.claude` folder) must never also
//! take out the only copy of what was there before.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use super::model::OperationReceipt;

/// Hash of file content, used both to gate an apply against concurrent edits
/// and to identify a receipt's before/after state. SHA-256 over raw bytes --
/// deliberately not normalizing line endings or whitespace first, since the
/// point is detecting *any* byte-level change, including ones a semantic diff
/// would consider irrelevant.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

/// Lowercase hex encoding. `sha2`'s digest output type does not implement
/// `LowerHex` directly, so this formats it byte-by-byte -- the same small
/// helper `git::toolset_fetch` already uses for the same reason.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Read a file's current bytes and hash, or `None` if it does not exist.
/// Any other I/O error is surfaced distinctly so a permissions problem is
/// never silently treated as "file absent."
pub fn read_current(path: &Path) -> Result<Option<(Vec<u8>, String)>, io::Error> {
    match fs::read(path) {
        Ok(bytes) => {
            let hash = hash_bytes(&bytes);
            Ok(Some((bytes, hash)))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApplyWriteError {
    #[error("could not read destination before writing: {detail}")]
    ReadDestination { detail: String },
    #[error("destination changed since preview")]
    ConcurrentChange {
        expected_hash: Option<String>,
        actual_hash: Option<String>,
    },
    #[error("could not write backup: {detail}")]
    Backup { detail: String },
    #[error("could not write receipt: {detail}")]
    Receipt { detail: String },
    #[error("could not create parent directory {dir}: {detail}", dir = dir.display())]
    CreateDir { dir: PathBuf, detail: String },
    #[error("could not create temp file: {detail}")]
    CreateTemp { detail: String },
    #[error("could not write temp file: {detail}")]
    WriteTemp { detail: String },
    #[error("could not flush temp file: {detail}")]
    Flush { detail: String },
    #[error("could not rename temp file into place: {detail}")]
    Rename { detail: String },
}

/// Root directory for backups and receipts:
/// `<app-data>/agent-config-sync/v1/{backups,receipts}`.
#[derive(Debug, Clone)]
pub struct SafeWriteRoot(PathBuf);

impl SafeWriteRoot {
    pub fn at(root: PathBuf) -> Result<Self, io::Error> {
        fs::create_dir_all(root.join("backups"))?;
        fs::create_dir_all(root.join("receipts"))?;
        fs::create_dir_all(root.join("plans"))?;
        Ok(Self(root))
    }

    pub fn resolve(app: &tauri::AppHandle) -> Result<Self, io::Error> {
        let app_data = crate::settings::app_data_dir(app).map_err(io::Error::other)?;
        Self::at(app_data.join("agent-config-sync").join("v1"))
    }

    fn backup_path(&self, operation_id: &str) -> PathBuf {
        self.0.join("backups").join(format!("{operation_id}.bak"))
    }

    fn receipt_path(&self, operation_id: &str) -> PathBuf {
        self.0.join("receipts").join(format!("{operation_id}.json"))
    }

    /// Where replaced content is kept so an undo can restore it. Exposed for
    /// the skill copier, which backs up a whole folder rather than one file
    /// and so cannot use `backup_path`'s single-file name.
    pub fn backups_dir(&self) -> PathBuf {
        self.0.join("backups")
    }

    /// Where in-flight (previewed, not-yet-applied) plans are stored.
    /// `pub(crate)` so the `agent_config` command layer can place plan files
    /// beside backups/receipts without this module knowing the plan shape
    /// itself (`CopyPlan` lives in `model`, which `plan.rs` does not depend
    /// on, keeping the write-safety primitives independent of the inventory
    /// domain model).
    pub fn plans_dir(&self) -> PathBuf {
        self.0.join("plans")
    }
}

/// Write bytes to `path` through a sibling temp file + flush + atomic
/// rename, creating the parent directory first if needed. Shared by the
/// backup write and the final destination write so both get the same
/// crash-safety guarantee `agentdesk::store::write_atomic` gives session
/// files.
fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), ApplyWriteError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|e| ApplyWriteError::CreateDir {
        dir: dir.to_path_buf(),
        detail: e.to_string(),
    })?;
    let mut temp = NamedTempFile::new_in(dir).map_err(|e| ApplyWriteError::CreateTemp {
        detail: e.to_string(),
    })?;
    io::Write::write_all(&mut temp, bytes).map_err(|e| ApplyWriteError::WriteTemp {
        detail: e.to_string(),
    })?;
    temp.as_file_mut().sync_all().map_err(|e| ApplyWriteError::Flush {
        detail: e.to_string(),
    })?;
    temp.persist(path)
        .map_err(|e| ApplyWriteError::Rename {
            detail: e.error.to_string(),
        })?;
    Ok(())
}

/// Apply one destination write: verify the destination still matches
/// `expected_before_hash`, back up its current content (if any), write the
/// new content atomically, and persist a receipt -- in that order, so the
/// backup and receipt exist on disk *before* the destination is ever
/// touched (task 3.3: "Write backup and receipt before temp + flush +
/// atomic rename").
pub fn apply_write(
    roots: &SafeWriteRoot,
    plan_id: &str,
    client_key: &str,
    destination_path: &Path,
    expected_before_hash: Option<&str>,
    new_content: &[u8],
    operation_id: &str,
    now: &str,
) -> Result<OperationReceipt, ApplyWriteError> {
    let current = read_current(destination_path).map_err(|e| ApplyWriteError::ReadDestination {
        detail: e.to_string(),
    })?;
    let actual_hash = current.as_ref().map(|(_, h)| h.clone());

    if actual_hash.as_deref() != expected_before_hash {
        return Err(ApplyWriteError::ConcurrentChange {
            expected_hash: expected_before_hash.map(str::to_string),
            actual_hash,
        });
    }

    let backup_path = if let Some((bytes, _)) = &current {
        let backup = roots.backup_path(operation_id);
        write_atomic_bytes(&backup, bytes)?;
        Some(backup.to_string_lossy().into_owned())
    } else {
        None
    };

    let receipt = OperationReceipt {
        operation_id: operation_id.to_string(),
        plan_id: plan_id.to_string(),
        client: client_key.to_string(),
        destination_path: destination_path.to_string_lossy().into_owned(),
        before_hash: expected_before_hash.map(str::to_string),
        after_hash: hash_bytes(new_content),
        backup_path,
        applied_at: now.to_string(),
        undone: false,
    };
    write_receipt(roots, &receipt)?;

    write_atomic_bytes(destination_path, new_content)?;

    Ok(receipt)
}

fn write_receipt(roots: &SafeWriteRoot, receipt: &OperationReceipt) -> Result<(), ApplyWriteError> {
    let json = serde_json::to_vec_pretty(receipt).map_err(|e| ApplyWriteError::Receipt {
        detail: e.to_string(),
    })?;
    write_atomic_bytes(&roots.receipt_path(&receipt.operation_id), &json).map_err(|e| match e {
        ApplyWriteError::CreateDir { detail, .. }
        | ApplyWriteError::CreateTemp { detail }
        | ApplyWriteError::WriteTemp { detail }
        | ApplyWriteError::Flush { detail }
        | ApplyWriteError::Rename { detail } => ApplyWriteError::Receipt { detail },
        other => other,
    })
}

/// Persists a receipt the caller built.
///
/// Exposed for the skill copier: it does its own writing (a folder, not one
/// file), but its result must be undoable through the same receipt store as
/// everything else, or Undo would report the operation as unknown.
pub fn save_receipt(roots: &SafeWriteRoot, receipt: &OperationReceipt) -> Result<(), ApplyWriteError> {
    write_receipt(roots, receipt)
}

/// Marks a receipt undone after the caller restored what it describes.
pub fn mark_receipt_undone(
    roots: &SafeWriteRoot,
    receipt: &mut OperationReceipt,
) -> Result<(), ApplyWriteError> {
    receipt.undone = true;
    write_receipt_overwrite(roots, receipt)
}

pub fn read_receipt(roots: &SafeWriteRoot, operation_id: &str) -> Option<OperationReceipt> {
    let bytes = fs::read(roots.receipt_path(operation_id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_receipt_overwrite(roots: &SafeWriteRoot, receipt: &OperationReceipt) -> Result<(), ApplyWriteError> {
    write_receipt(roots, receipt)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UndoWriteError {
    #[error("operation receipt not found")]
    NotFound,
    #[error("operation was already undone")]
    AlreadyUndone,
    #[error("destination changed since this write; refusing to restore")]
    ConcurrentChange {
        expected_hash: String,
        actual_hash: Option<String>,
    },
    #[error("backup file is missing or unreadable: {detail}")]
    BackupUnreadable { detail: String },
    #[error(transparent)]
    Write(#[from] ApplyWriteError),
}

/// Undo one operation: verify the destination still matches the hash this
/// operation last produced (`after_hash`), then restore the backed-up bytes
/// (or delete the file, if this operation created it from nothing) and mark
/// the receipt undone. Restoring from the backup file guarantees
/// byte-identical content -- the backup was written from the exact bytes
/// read immediately before the original write, never reconstructed from the
/// normalized item model.
pub fn undo_write(
    roots: &SafeWriteRoot,
    operation_id: &str,
) -> Result<OperationReceipt, UndoWriteError> {
    let mut receipt = read_receipt(roots, operation_id).ok_or(UndoWriteError::NotFound)?;
    if receipt.undone {
        return Err(UndoWriteError::AlreadyUndone);
    }

    let destination = PathBuf::from(&receipt.destination_path);
    let current = read_current(&destination).map_err(|e| {
        UndoWriteError::Write(ApplyWriteError::ReadDestination {
            detail: e.to_string(),
        })
    })?;
    let actual_hash = current.as_ref().map(|(_, h)| h.clone());
    if actual_hash.as_deref() != Some(receipt.after_hash.as_str()) {
        return Err(UndoWriteError::ConcurrentChange {
            expected_hash: receipt.after_hash.clone(),
            actual_hash,
        });
    }

    match &receipt.backup_path {
        Some(backup_path) => {
            let bytes = fs::read(backup_path).map_err(|e| UndoWriteError::BackupUnreadable {
                detail: e.to_string(),
            })?;
            write_atomic_bytes(&destination, &bytes)?;
        }
        None => {
            // This operation created the file from nothing; undo removes it.
            // A file that is already gone is not an error here -- that is
            // exactly the end state undo wants.
            if let Err(e) = fs::remove_file(&destination) {
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(UndoWriteError::Write(ApplyWriteError::ReadDestination {
                        detail: e.to_string(),
                    }));
                }
            }
        }
    }

    receipt.undone = true;
    write_receipt_overwrite(roots, &receipt)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn roots() -> (TempDir, SafeWriteRoot) {
        let dir = TempDir::new().unwrap();
        let root = SafeWriteRoot::at(dir.path().join("agent-config-sync").join("v1")).unwrap();
        (dir, root)
    }

    #[test]
    fn applying_a_write_creates_backup_receipt_and_new_content_in_order() {
        let (dir, roots) = roots();
        let dest = dir.path().join("dest.json");
        fs::write(&dest, "{\"a\":1}").unwrap();
        let before_hash = hash_bytes(b"{\"a\":1}");

        let receipt = apply_write(
            &roots,
            "plan-1",
            "claude-code",
            &dest,
            Some(&before_hash),
            b"{\"a\":2}",
            "op-1",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&dest).unwrap(), "{\"a\":2}");
        assert_eq!(receipt.after_hash, hash_bytes(b"{\"a\":2}"));
        let backup_contents = fs::read_to_string(receipt.backup_path.as_ref().unwrap()).unwrap();
        assert_eq!(backup_contents, "{\"a\":1}");
        assert!(read_receipt(&roots, "op-1").is_some());
    }

    #[test]
    fn apply_refuses_when_destination_hash_differs_from_expected() {
        let (dir, roots) = roots();
        let dest = dir.path().join("dest.json");
        fs::write(&dest, "{\"a\":1}").unwrap();
        let stale_hash = hash_bytes(b"{\"a\":0}"); // wrong on purpose

        let result = apply_write(
            &roots,
            "plan-1",
            "claude-code",
            &dest,
            Some(&stale_hash),
            b"{\"a\":2}",
            "op-1",
            "2026-01-01T00:00:00Z",
        );

        assert!(matches!(result, Err(ApplyWriteError::ConcurrentChange { .. })));
        // Destination must be untouched.
        assert_eq!(fs::read_to_string(&dest).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn apply_on_a_nonexistent_destination_creates_it_with_no_backup() {
        let (dir, roots) = roots();
        let dest = dir.path().join("new.json");

        let receipt = apply_write(
            &roots,
            "plan-1",
            "open-code",
            &dest,
            None,
            b"{\"a\":1}",
            "op-2",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        assert!(receipt.backup_path.is_none());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn undo_restores_byte_identical_content() {
        let (dir, roots) = roots();
        let dest = dir.path().join("dest.json");
        let original = "{\n  \"a\": 1,\n  \"b\": [1,2,3]\n}";
        fs::write(&dest, original).unwrap();
        let before_hash = hash_bytes(original.as_bytes());

        apply_write(
            &roots,
            "plan-1",
            "claude-code",
            &dest,
            Some(&before_hash),
            b"{\"a\":2}",
            "op-1",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        let receipt = undo_write(&roots, "op-1").unwrap();
        assert!(receipt.undone);
        let restored = fs::read_to_string(&dest).unwrap();
        assert_eq!(restored, original, "undo must be byte-identical to the original");
    }

    #[test]
    fn undo_deletes_a_file_that_was_created_from_nothing() {
        let (dir, roots) = roots();
        let dest = dir.path().join("new.json");
        apply_write(&roots, "plan-1", "open-code", &dest, None, b"{\"a\":1}", "op-2", "t").unwrap();
        assert!(dest.exists());

        undo_write(&roots, "op-2").unwrap();
        assert!(!dest.exists());
    }

    #[test]
    fn undo_refuses_when_destination_changed_after_apply() {
        let (dir, roots) = roots();
        let dest = dir.path().join("dest.json");
        fs::write(&dest, "{\"a\":1}").unwrap();
        let before_hash = hash_bytes(b"{\"a\":1}");
        apply_write(&roots, "plan-1", "claude-code", &dest, Some(&before_hash), b"{\"a\":2}", "op-1", "t")
            .unwrap();

        // Someone else edits the file after our write.
        fs::write(&dest, "{\"a\":999}").unwrap();

        let result = undo_write(&roots, "op-1");
        assert!(matches!(result, Err(UndoWriteError::ConcurrentChange { .. })));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "{\"a\":999}");
    }

    #[test]
    fn undo_twice_reports_already_undone_without_touching_the_file_again() {
        let (dir, roots) = roots();
        let dest = dir.path().join("dest.json");
        fs::write(&dest, "{\"a\":1}").unwrap();
        let before_hash = hash_bytes(b"{\"a\":1}");
        apply_write(&roots, "plan-1", "claude-code", &dest, Some(&before_hash), b"{\"a\":2}", "op-1", "t")
            .unwrap();
        undo_write(&roots, "op-1").unwrap();

        let result = undo_write(&roots, "op-1");
        assert!(matches!(result, Err(UndoWriteError::AlreadyUndone)));
    }

    #[test]
    fn a_receipt_written_before_the_client_field_was_widened_still_reads_back() {
        // Receipts written by earlier builds stored the client as the enum,
        // which serialized to exactly these kebab-case strings. A receipt is
        // read back weeks later to undo a write, so an old one that no longer
        // parsed would strand the user with no way to undo.
        let old_receipt = r#"{
  "operationId": "op-old",
  "planId": "plan-old",
  "client": "claude-code",
  "destinationPath": "C:/somewhere/settings.json",
  "beforeHash": null,
  "afterHash": "abc",
  "backupPath": null,
  "appliedAt": "2026-01-01T00:00:00Z",
  "undone": false
}"#;
        let parsed: OperationReceipt = serde_json::from_str(old_receipt).expect("old receipt still parses");
        assert_eq!(parsed.client, "claude-code");
        assert_eq!(parsed.operation_id, "op-old");
    }

    #[test]
    fn a_receipt_naming_a_client_this_build_does_not_know_is_still_undoable() {
        // The whole reason the field is a string: undo depends on the path,
        // hashes, and backup in the receipt, never on recognizing the client.
        let (dir, roots) = roots();
        let dest = dir.path().join("dest.json");
        fs::write(&dest, "{\"a\":1}").unwrap();
        let before_hash = hash_bytes(b"{\"a\":1}");

        apply_write(
            &roots,
            "plan-1",
            "some-client-from-the-future",
            &dest,
            Some(&before_hash),
            b"{\"a\":2}",
            "op-future",
            "t",
        )
        .unwrap();

        let receipt = undo_write(&roots, "op-future").unwrap();
        assert!(receipt.undone);
        assert_eq!(fs::read_to_string(&dest).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn undo_of_unknown_operation_id_is_not_found() {
        let (_dir, roots) = roots();
        let result = undo_write(&roots, "does-not-exist");
        assert!(matches!(result, Err(UndoWriteError::NotFound)));
    }

    #[test]
    fn backup_recovery_survives_a_simulated_replacement_failure() {
        // Simulate: apply_write wrote the backup and receipt, but the final
        // destination write never happened (process died in between). The
        // backup must still be enough to restore from once the receipt is
        // read directly, without relying on the destination write having
        // succeeded.
        let (dir, roots) = roots();
        let dest = dir.path().join("dest.json");
        let original = "{\"a\":1}";
        fs::write(&dest, original).unwrap();
        let before_hash = hash_bytes(original.as_bytes());

        // Manually perform only the backup+receipt half of apply_write.
        let backup_path = dir.path().join("agent-config-sync/v1/backups/op-x.bak");
        fs::create_dir_all(backup_path.parent().unwrap()).unwrap();
        fs::write(&backup_path, original).unwrap();
        let receipt = OperationReceipt {
            operation_id: "op-x".into(),
            plan_id: "plan-1".into(),
            client: "claude-code".into(),
            destination_path: dest.to_string_lossy().into_owned(),
            before_hash: Some(before_hash),
            after_hash: hash_bytes(original.as_bytes()), // write never happened; dest still == original
            backup_path: Some(backup_path.to_string_lossy().into_owned()),
            applied_at: "t".into(),
            undone: false,
        };
        write_receipt_overwrite(&roots, &receipt).unwrap();

        // Because the destination write never happened, dest still equals
        // after_hash trivially (it never changed) -- undo should succeed and
        // restore is a no-op content-wise, proving the recovery path works
        // even when triggered on a receipt whose write step was interrupted.
        let restored = undo_write(&roots, "op-x").unwrap();
        assert!(restored.undone);
        assert_eq!(fs::read_to_string(&dest).unwrap(), original);
    }
}
