//! Copying a skill from one client to another.
//!
//! Every other item this subsystem writes is one member of a JSON object in
//! one file, so `plan.rs` is built around exactly that: read a file, compare
//! one hash, back up one file, write one file, undo one file. A skill is not
//! shaped like that at all. It is a *folder* -- `SKILL.md` plus whatever else
//! it ships (scripts, references, templates) -- and copying one means copying
//! a tree.
//!
//! That mismatch is why the Skills tab could list skills and never copy one:
//! the writers all refused `ItemKind::Skill` rather than pretend a directory
//! was a JSON member.
//!
//! This module keeps every promise `plan.rs` makes, applied to a tree:
//!
//! - **Nothing is overwritten silently.** A destination that already exists
//!   is refused unless the caller says to replace it, and even then the
//!   existing tree is backed up first.
//! - **The plan is hash-gated.** The source is hashed file by file when the
//!   plan is built; if any of it changed before Apply, the copy is refused
//!   rather than writing a mixture of two versions.
//! - **Undo restores exactly what was there**, including "there was nothing
//!   here", which removes the copied folder rather than leaving an empty one.
//!   Where an exact restore is not possible -- a symlink in the folder about
//!   to be replaced, which cannot be copied faithfully -- the replacement is
//!   refused up front rather than performed and half-restored later.
//! - **Nothing escapes the destination root.** A skill folder is only ever
//!   written inside the client's own skills directory.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use super::plan::hash_bytes;

/// A skill's files, read once, with a hash each.
///
/// Ordered (`BTreeMap`) so a plan's file list, its hashes and any message
/// built from them are stable between runs: an unstable order would make two
/// identical plans look different and a diff impossible to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillTree {
    /// Path relative to the skill's own folder, e.g. `SKILL.md` or
    /// `references/api.md`. Always forward-slashed so a plan built on
    /// Windows reads the same everywhere.
    pub files: BTreeMap<String, SkillFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile {
    pub bytes: Vec<u8>,
    pub hash: String,
}

impl SkillTree {
    /// Total bytes across every file, for the size a person is shown before
    /// agreeing to a copy.
    pub fn total_bytes(&self) -> u64 {
        self.files.values().map(|f| f.bytes.len() as u64).sum()
    }
}

/// Why a skill could not be read or copied.
///
/// Typed per cause, because each one is a different thing for the person to
/// do: a missing folder is not a permissions problem, and neither is a source
/// that changed while they were reading the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillCopyError {
    /// The skill folder is not there (any more).
    SourceMissing { path: String },
    /// Something under the source could not be read.
    ReadFailed { path: String, detail: String },
    /// The source changed between building the plan and applying it. Refused
    /// rather than writing half of one version and half of another.
    SourceChanged { path: String },
    /// The destination already has a skill by this name and the caller did
    /// not ask to replace it.
    DestinationExists { path: String },
    /// The destination folder changed after the copy, so undoing would throw
    /// away whatever it changed into. Refused rather than restored.
    DestinationChanged { path: String },
    /// A write, backup or directory creation failed.
    WriteFailed { path: String, detail: String },
    /// A file inside the skill named a path that would land outside the
    /// destination folder. Never expected from a real skill; refused rather
    /// than resolved.
    UnsafePath { entry: String },
}

impl SkillCopyError {
    /// The sentence a person reads.
    pub fn plain(&self) -> String {
        match self {
            SkillCopyError::SourceMissing { .. } => {
                "That skill's folder is not there any more.".into()
            }
            SkillCopyError::ReadFailed { detail, .. } => {
                format!("That skill could not be read: {detail}")
            }
            SkillCopyError::DestinationChanged { .. } => {
                "That skill has been changed since GitWyrm copied it, so it was left alone.                  Putting it back would have thrown away those changes."
                    .into()
            }
            SkillCopyError::SourceChanged { .. } => {
                "That skill changed while you were looking at it, so nothing was copied. Try again."
                    .into()
            }
            SkillCopyError::DestinationExists { .. } => {
                "There is already a skill with that name here. Replace it to overwrite what is there."
                    .into()
            }
            SkillCopyError::WriteFailed { detail, .. } => {
                format!("That skill could not be copied: {detail}")
            }
            SkillCopyError::UnsafePath { entry } => {
                format!("That skill contains a file path GitWyrm will not write: {entry}")
            }
        }
    }
}

/// How many files deep and wide a skill may be before this refuses to read it.
///
/// A skill is documentation plus a few helpers. These bounds exist so a
/// mistaken folder (a repository checked out inside a skills directory, say)
/// is refused quickly instead of read into memory.
const MAX_FILES: usize = 200;
const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024;

/// Reads a skill folder into memory, hashing every file.
pub fn read_skill_tree(dir: &Path) -> Result<SkillTree, SkillCopyError> {
    if !dir.is_dir() {
        return Err(SkillCopyError::SourceMissing {
            path: dir.to_string_lossy().into_owned(),
        });
    }
    let mut files = BTreeMap::new();
    let mut total: u64 = 0;
    collect(dir, dir, &mut files, &mut total)?;
    Ok(SkillTree { files })
}

fn collect(
    root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, SkillFile>,
    total: &mut u64,
) -> Result<(), SkillCopyError> {
    let entries = std::fs::read_dir(dir).map_err(|e| SkillCopyError::ReadFailed {
        path: dir.to_string_lossy().into_owned(),
        detail: e.to_string(),
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        // Symlinks are not followed: a link could point anywhere on disk, and
        // copying what it targets is never what "copy this skill" means.
        let meta = entry.metadata().map_err(|e| SkillCopyError::ReadFailed {
            path: path.to_string_lossy().into_owned(),
            detail: e.to_string(),
        })?;
        if meta.is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect(root, &path, out, total)?;
            continue;
        }
        let rel = relative_key(root, &path)?;
        let bytes = std::fs::read(&path).map_err(|e| SkillCopyError::ReadFailed {
            path: path.to_string_lossy().into_owned(),
            detail: e.to_string(),
        })?;
        *total += bytes.len() as u64;
        if out.len() >= MAX_FILES || *total > MAX_TOTAL_BYTES {
            return Err(SkillCopyError::ReadFailed {
                path: root.to_string_lossy().into_owned(),
                detail: "this folder holds far more than a skill should".into(),
            });
        }
        let hash = hash_bytes(&bytes);
        out.insert(rel, SkillFile { bytes, hash });
    }
    Ok(())
}

/// The forward-slashed path of `path` inside `root`, refusing anything that
/// climbs out of it.
fn relative_key(root: &Path, path: &Path) -> Result<String, SkillCopyError> {
    let rel = path.strip_prefix(root).map_err(|_| SkillCopyError::UnsafePath {
        entry: path.to_string_lossy().into_owned(),
    })?;
    let mut parts = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            // `..`, a root, or a drive letter inside a relative path all mean
            // the same thing here: this is not a plain file under the skill.
            _ => {
                return Err(SkillCopyError::UnsafePath {
                    entry: rel.to_string_lossy().into_owned(),
                })
            }
        }
    }
    Ok(parts.join("/"))
}

/// What one skill copy will do, built before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCopyPlan {
    pub source_dir: PathBuf,
    pub destination_dir: PathBuf,
    /// Relative paths and their source hashes, so Apply can prove the source
    /// has not moved underneath the preview.
    pub files: BTreeMap<String, String>,
    pub total_bytes: u64,
    /// True when the destination folder already exists, so the UI can say
    /// "replace" rather than "copy" and ask first.
    pub replaces_existing: bool,
}

/// Builds the plan for copying `source_dir` to `destination_dir`.
pub fn plan_skill_copy(
    source_dir: &Path,
    destination_dir: &Path,
) -> Result<SkillCopyPlan, SkillCopyError> {
    let tree = read_skill_tree(source_dir)?;
    Ok(SkillCopyPlan {
        source_dir: source_dir.to_path_buf(),
        destination_dir: destination_dir.to_path_buf(),
        total_bytes: tree.total_bytes(),
        files: tree
            .files
            .iter()
            .map(|(k, v)| (k.clone(), v.hash.clone()))
            .collect(),
        replaces_existing: destination_dir.is_dir(),
    })
}

/// What a copy did, and what undoing it would need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCopyReceipt {
    pub destination_dir: String,
    /// Where the destination's previous contents were saved, when it had any.
    /// `None` means the folder did not exist, and undo removes it.
    pub backup_dir: Option<String>,
    pub files_written: usize,
    /// What the destination folder hashed to immediately after this copy.
    ///
    /// Undo compares against it and refuses when it no longer matches, which
    /// is the folder equivalent of the single-file path's `after_hash` check.
    /// `None` on a receipt written before this existed, or reconstructed
    /// without it -- undo then behaves as it always did rather than refusing
    /// every historical receipt.
    pub after_digest: Option<String>,
}

/// Copies the skill, after re-checking that the source still matches the plan.
///
/// `backup_root` is where a replaced folder is kept so undo can restore it,
/// the same role `SafeWriteRoot::backup_path` plays for single files.
pub fn apply_skill_copy(
    plan: &SkillCopyPlan,
    backup_root: &Path,
    allow_replace: bool,
) -> Result<SkillCopyReceipt, SkillCopyError> {
    // Re-read and re-hash: the preview may have been on screen for a while,
    // and copying a mixture of two versions is worse than refusing.
    let tree = read_skill_tree(&plan.source_dir)?;
    let current: BTreeMap<String, String> = tree
        .files
        .iter()
        .map(|(k, v)| (k.clone(), v.hash.clone()))
        .collect();
    if current != plan.files {
        return Err(SkillCopyError::SourceChanged {
            path: plan.source_dir.to_string_lossy().into_owned(),
        });
    }

    let exists = plan.destination_dir.is_dir();
    if exists && !allow_replace {
        return Err(SkillCopyError::DestinationExists {
            path: plan.destination_dir.to_string_lossy().into_owned(),
        });
    }

    // Back the old folder up BEFORE touching it, so a failure halfway through
    // still leaves something to restore from.
    let backup_dir = if exists {
        let name = plan
            .destination_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "skill".to_string());
        let target = backup_root.join(format!("{name}-{}", short_stamp()));
        copy_tree(&plan.destination_dir, &target)?;
        std::fs::remove_dir_all(&plan.destination_dir).map_err(|e| SkillCopyError::WriteFailed {
            path: plan.destination_dir.to_string_lossy().into_owned(),
            detail: e.to_string(),
        })?;
        Some(target.to_string_lossy().into_owned())
    } else {
        None
    };

    // A write that stops halfway used to return here and leave the person
    // with nothing: the old folder already deleted, some of the new files on
    // disk, and no receipt -- so the backup sat in GitWyrm's own data with
    // nothing in the app pointing at it and no way to undo. A file locked by
    // the destination app is ordinary on Windows, so this is not a remote
    // case.
    //
    // Put the old folder back instead. The copy is refused either way; the
    // difference is whether the person still has what they started with.
    let written = match write_tree(&tree, &plan.destination_dir) {
        Ok(count) => count,
        Err(write_error) => {
            roll_back(&plan.destination_dir, backup_dir.as_deref());
            return Err(write_error);
        }
    };

    Ok(SkillCopyReceipt {
        destination_dir: plan.destination_dir.to_string_lossy().into_owned(),
        backup_dir,
        files_written: written,
        // Best effort: a digest that cannot be computed leaves undo behaving
        // as before rather than refusing a copy that succeeded.
        after_digest: folder_digest(&plan.destination_dir).ok(),
    })
}

/// Put the destination back the way it was after a copy failed partway.
///
/// Best effort by design: this runs while already returning an error, and a
/// rollback that itself fails must not replace the real reason the copy
/// stopped with a second, less useful one. What it cannot restore stays in
/// the backup folder, which the caller still names in its message.
///
/// With no backup the destination did not exist before, so removing the
/// half-written folder IS the restore.
fn roll_back(destination: &Path, backup: Option<&str>) {
    let _ = std::fs::remove_dir_all(destination);
    let Some(backup) = backup else {
        return;
    };
    if let Ok(tree) = read_skill_tree(Path::new(backup)) {
        let _ = write_tree(&tree, destination);
    }
}

/// A single hash standing for a whole skill folder's contents.
///
/// `undo_skill_copy` used to `remove_dir_all` the destination with no check at
/// all, while the single-file undo verifies `after_hash` first and refuses when
/// the destination changed. A folder receipt carries an empty `after_hash` --
/// that emptiness is the marker used to route to the folder path -- so there
/// was nothing to compare against and the doc's promise ("restoring
/// byte-identical prior content unless the destination changed since the
/// write") was true for files and false for skills. Editing a copied skill and
/// then clicking Undo destroyed the edit.
///
/// Built from the per-file hashes `read_skill_tree` already computes, over a
/// `BTreeMap`, so the same contents always produce the same digest regardless
/// of directory-read order. Paths are included, so a renamed file changes it.
pub fn folder_digest(dir: &Path) -> Result<String, SkillCopyError> {
    let tree = read_skill_tree(dir)?;
    let mut joined = String::new();
    for (rel, file) in &tree.files {
        joined.push_str(rel);
        joined.push('\0');
        joined.push_str(&file.hash);
        joined.push('\n');
    }
    Ok(hash_bytes(joined.as_bytes()))
}

/// Puts back whatever the copy replaced.
///
/// A receipt with no backup means the folder did not exist before, so undo
/// removes what was copied rather than leaving an empty skill behind.
pub fn undo_skill_copy(receipt: &SkillCopyReceipt) -> Result<(), SkillCopyError> {
    let destination = PathBuf::from(&receipt.destination_dir);
    // Refuse if the folder changed after the copy. This used to delete the
    // destination outright with no check, so a skill the person edited after
    // copying it was destroyed by Undo -- while the single-file path refuses
    // in exactly that case, which is what made the shared doc read as covering
    // both.
    if let Some(expected) = &receipt.after_digest {
        if destination.is_dir() {
            match folder_digest(&destination) {
                Ok(actual) if &actual != expected => {
                    // Before calling this a change: an Undo interrupted after
                    // restoring the folder but before its receipt was marked
                    // done leaves the destination holding the BACKUP content.
                    // Retrying then compared against `after_digest` -- the
                    // post-copy fingerprint -- and refused, telling the person
                    // their skill had been edited and GitWyrm was leaving it
                    // alone. Nothing of theirs had changed; GitWyrm's own
                    // restore had already finished.
                    //
                    // The single-file path recognises this by comparing with
                    // `before_hash`. There is no `before_digest` here, but the
                    // backup folder IS the before-state, so digesting it
                    // answers the same question without a new field.
                    if let Some(backup) = &receipt.backup_dir {
                        if matches!(folder_digest(Path::new(backup)), Ok(b) if b == actual) {
                            return Ok(());
                        }
                    }
                    return Err(SkillCopyError::DestinationChanged {
                        path: receipt.destination_dir.clone(),
                    });
                }
                // A digest that cannot be read is not evidence of a change;
                // the copy_tree below will surface any real failure.
                _ => {}
            }
        }
    }
    // Read the backup BEFORE deleting anything. This used to delete the
    // destination first and only then read the backup, so a backup that had
    // gone missing -- cleared app data, a hand-deleted folder, a file
    // quarantined by antivirus -- destroyed the copy and restored nothing,
    // leaving neither version. And it said "That skill's folder is not there
    // any more", which reads as "nothing happened".
    let restore = match &receipt.backup_dir {
        Some(backup) => Some(read_skill_tree(Path::new(backup))?),
        None => None,
    };

    if destination.is_dir() {
        std::fs::remove_dir_all(&destination).map_err(|e| SkillCopyError::WriteFailed {
            path: receipt.destination_dir.clone(),
            detail: e.to_string(),
        })?;
    }
    let Some(tree) = restore else {
        return Ok(());
    };
    write_tree(&tree, &destination).map(|_| ())
}

/// Write an already-read tree into `to`.
///
/// Split out of `copy_tree` so the restore can read its backup first and
/// only then touch the destination -- the read is the part that can fail.
fn write_tree(tree: &SkillTree, to: &Path) -> Result<usize, SkillCopyError> {
    let mut written = 0usize;
    for (rel, file) in &tree.files {
        let target = safe_join(to, rel)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SkillCopyError::WriteFailed {
                path: parent.to_string_lossy().into_owned(),
                detail: e.to_string(),
            })?;
        }
        std::fs::write(&target, &file.bytes).map_err(|e| SkillCopyError::WriteFailed {
            path: target.to_string_lossy().into_owned(),
            detail: e.to_string(),
        })?;
        written += 1;
    }
    Ok(written)
}

/// Copy a folder GitWyrm is about to destroy, or put one back.
///
/// Deliberately NOT the same policy as reading a source skill.
/// `read_skill_tree` skips symlinks and refuses a folder over the size caps
/// -- correct for a folder the person chose to copy, and wrong for one they
/// did not. Backing up with those rules silently omitted anything it would
/// not copy, and the caller then deleted the original anyway: a symlink in
/// someone's existing skill vanished, and Undo restored a folder that was
/// not what had been there. The module promises "Undo restores exactly what
/// was there", and this is the function that has to make that true.
///
/// So anything that cannot be copied faithfully is refused here rather than
/// dropped. Refusing costs the person one copy; dropping costs them a file
/// they never agreed to lose.
fn copy_tree(from: &Path, to: &Path) -> Result<(), SkillCopyError> {
    if let Some(unfaithful) = first_uncopyable(from)? {
        return Err(SkillCopyError::WriteFailed {
            path: unfaithful,
            detail: "GitWyrm cannot copy this exactly, so it did not replace the folder".into(),
        });
    }
    let tree = read_skill_tree(from)?;
    write_tree(&tree, to).map(|_| ())
}

/// Joins a relative key under `root`, refusing anything that would escape it.
/// The first thing in `dir` that [`read_skill_tree`] would not carry across
/// faithfully, if there is one.
///
/// Only symlinks today: the size caps already surface as a refusal from
/// `read_skill_tree` itself, while a symlink is skipped in silence, which is
/// the case that loses data without saying so.
fn first_uncopyable(dir: &Path) -> Result<Option<String>, SkillCopyError> {
    let entries = std::fs::read_dir(dir).map_err(|e| SkillCopyError::ReadFailed {
        path: dir.to_string_lossy().into_owned(),
        detail: e.to_string(),
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        let meta = entry.metadata().map_err(|e| SkillCopyError::ReadFailed {
            path: path.to_string_lossy().into_owned(),
            detail: e.to_string(),
        })?;
        if meta.is_symlink() {
            return Ok(Some(path.to_string_lossy().into_owned()));
        }
        if meta.is_dir() {
            if let Some(found) = first_uncopyable(&path)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, SkillCopyError> {
    let mut out = root.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(SkillCopyError::UnsafePath {
                entry: rel.to_string(),
            });
        }
        out.push(part);
    }
    Ok(out)
}

/// A short, sortable stamp for a backup folder name.
fn short_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {

    /// Undo used to `remove_dir_all` the destination with no check, so a skill
    /// the person edited after copying was destroyed by it -- while the
    /// single-file path refuses in exactly that case, which is what made the
    /// shared doc read as covering both.
    #[test]
    fn undo_refuses_when_the_skill_folder_was_edited_after_the_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        write(&dest, "SKILL.md", "as copied");

        let receipt = SkillCopyReceipt {
            destination_dir: dest.to_string_lossy().into_owned(),
            backup_dir: None,
            files_written: 1,
            after_digest: Some(folder_digest(&dest).unwrap()),
        };

        // The person edits the copied skill.
        write(&dest, "SKILL.md", "my own edit");

        let err = undo_skill_copy(&receipt).expect_err("must refuse");
        assert!(matches!(err, SkillCopyError::DestinationChanged { .. }));
        // And the edit survives.
        assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "my own edit");
    }

    /// The folder equivalent of the single-file interrupted-Undo case.
    ///
    /// Restoring the folder and marking its receipt done are two steps. A
    /// crash between them leaves the destination holding the BACKUP content
    /// while the receipt still says the copy stands, and the retry compared
    /// against `after_digest` -- the post-copy fingerprint -- and refused,
    /// telling the person their skill had been edited. It had not.
    #[test]
    fn undo_accepts_a_folder_a_previous_undo_already_restored() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        let backup = tmp.path().join("backup");
        write(&backup, "SKILL.md", "what was there before");
        write(&dest, "SKILL.md", "as copied");

        let receipt = SkillCopyReceipt {
            destination_dir: dest.to_string_lossy().into_owned(),
            backup_dir: Some(backup.to_string_lossy().into_owned()),
            files_written: 1,
            after_digest: Some(folder_digest(&dest).unwrap()),
        };

        // The interrupted restore: the destination already holds the backup.
        write(&dest, "SKILL.md", "what was there before");

        undo_skill_copy(&receipt).expect("a finished restore is not an edit");
        assert_eq!(
            fs::read_to_string(dest.join("SKILL.md")).unwrap(),
            "what was there before",
            "the restored content is left exactly as it was"
        );
    }

    #[test]
    fn undo_still_works_when_the_folder_is_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        write(&dest, "SKILL.md", "as copied");
        let receipt = SkillCopyReceipt {
            destination_dir: dest.to_string_lossy().into_owned(),
            backup_dir: None,
            files_written: 1,
            after_digest: Some(folder_digest(&dest).unwrap()),
        };
        undo_skill_copy(&receipt).expect("unchanged folder undoes cleanly");
        assert!(!dest.exists(), "the copy is removed when nothing changed");
    }

    /// A receipt written before the digest existed must still undo, rather
    /// than refusing every historical operation.
    #[test]
    fn undo_without_a_digest_behaves_as_it_always_did() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        write(&dest, "SKILL.md", "whatever");
        let receipt = SkillCopyReceipt {
            destination_dir: dest.to_string_lossy().into_owned(),
            backup_dir: None,
            files_written: 1,
            after_digest: None,
        };
        undo_skill_copy(&receipt).expect("no digest means no refusal");
        assert!(!dest.exists());
    }

    #[test]
    fn the_digest_notices_a_renamed_file_not_just_changed_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        write(&a, "SKILL.md", "same body");
        let first = folder_digest(&a).unwrap();
        fs::rename(a.join("SKILL.md"), a.join("OTHER.md")).unwrap();
        assert_ne!(folder_digest(&a).unwrap(), first, "paths are part of the digest");
    }
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn skill(root: &Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        write(&dir, "SKILL.md", "---\nname: demo\n---\nBody\n");
        write(&dir, "references/api.md", "reference\n");
        dir
    }

    #[test]
    fn a_skill_is_read_as_a_whole_folder_not_one_file() {
        let tmp = tempfile::tempdir().unwrap();
        let src = skill(tmp.path(), "demo");
        let tree = read_skill_tree(&src).unwrap();
        assert_eq!(
            tree.files.keys().cloned().collect::<Vec<_>>(),
            vec!["SKILL.md".to_string(), "references/api.md".to_string()],
            "nested files count, and the order is stable"
        );
        assert!(tree.total_bytes() > 0);
    }

    #[test]
    fn copying_creates_the_folder_and_undo_removes_it_again() {
        let tmp = tempfile::tempdir().unwrap();
        let src = skill(tmp.path(), "demo");
        let dest = tmp.path().join("dest").join("demo");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&backups).unwrap();

        let plan = plan_skill_copy(&src, &dest).unwrap();
        assert!(!plan.replaces_existing);
        let receipt = apply_skill_copy(&plan, &backups, false).unwrap();
        assert_eq!(receipt.files_written, 2);
        assert!(dest.join("references/api.md").is_file());
        assert!(receipt.backup_dir.is_none(), "nothing was there to back up");

        undo_skill_copy(&receipt).unwrap();
        assert!(!dest.exists(), "undo leaves no empty folder behind");
    }

    #[test]
    fn an_existing_skill_is_never_overwritten_without_being_asked() {
        let tmp = tempfile::tempdir().unwrap();
        let src = skill(tmp.path(), "demo");
        let dest = tmp.path().join("dest").join("demo");
        write(&dest, "SKILL.md", "mine\n");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&backups).unwrap();

        let plan = plan_skill_copy(&src, &dest).unwrap();
        assert!(plan.replaces_existing, "the UI has to be able to say replace");
        assert_eq!(
            apply_skill_copy(&plan, &backups, false).unwrap_err(),
            SkillCopyError::DestinationExists {
                path: dest.to_string_lossy().into_owned()
            }
        );
        assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "mine\n");
    }

    #[test]
    fn replacing_backs_up_first_and_undo_puts_it_back_exactly() {
        let tmp = tempfile::tempdir().unwrap();
        let src = skill(tmp.path(), "demo");
        let dest = tmp.path().join("dest").join("demo");
        write(&dest, "SKILL.md", "mine\n");
        write(&dest, "notes.md", "my notes\n");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&backups).unwrap();

        let plan = plan_skill_copy(&src, &dest).unwrap();
        let receipt = apply_skill_copy(&plan, &backups, true).unwrap();
        assert!(receipt.backup_dir.is_some());
        // The copy replaced the folder wholesale: the old extra file is gone.
        assert!(!dest.join("notes.md").exists());
        assert!(fs::read_to_string(dest.join("SKILL.md")).unwrap().contains("Body"));

        undo_skill_copy(&receipt).unwrap();
        assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "mine\n");
        assert_eq!(fs::read_to_string(dest.join("notes.md")).unwrap(), "my notes\n");
    }

    /// A folder GitWyrm cannot copy faithfully must not be replaced.
    ///
    /// The backup was made with the same rules used to READ a skill, which
    /// skip symlinks -- correct for a folder the person chose to copy, wrong
    /// for one they did not. The backup reported success having quietly left
    /// the link out, the original was deleted anyway, and Undo restored a
    /// folder that was not what had been there.
    ///
    /// Creating a symlink needs privilege on Windows, which a test run does
    /// not reliably have, so the check itself is tested directly here and
    /// the end-to-end refusal is tested below only where a link can be made.
    /// A test that silently skips is a test that has never caught anything.
    #[test]
    fn a_plain_folder_has_nothing_that_cannot_be_copied() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = skill(tmp.path(), "demo");
        write(&dir, "references/api.md", "ref
");
        assert_eq!(first_uncopyable(&dir).unwrap(), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_folder_holding_a_link_is_reported_as_uncopyable() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = skill(tmp.path(), "demo");
        let target = tmp.path().join("elsewhere.md");
        fs::write(&target, "linked
").unwrap();
        if std::os::windows::fs::symlink_file(&target, dir.join("linked.md")).is_err() {
            // No privilege for symlinks in this session; the direct check
            // above still ran, and the refusal path is exercised wherever a
            // link can actually be created.
            return;
        }
        assert!(first_uncopyable(&dir).unwrap().is_some());

        // And end to end: the folder it refused to back up is untouched.
        let src = skill(tmp.path(), "source");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&backups).unwrap();
        let plan = plan_skill_copy(&src, &dir).unwrap();
        assert!(matches!(
            apply_skill_copy(&plan, &backups, true),
            Err(SkillCopyError::WriteFailed { .. })
        ));
        assert!(dir.join("linked.md").exists());
    }

    /// Undo must not destroy the copy when it cannot restore the original.
    ///
    /// It deleted the destination first and read the backup afterwards, so a
    /// backup that had gone -- cleared app data, a hand-deleted folder, a
    /// file quarantined by antivirus -- left neither version, and said "That
    /// skill's folder is not there any more", which reads as nothing having
    /// happened.
    #[test]
    fn undo_keeps_the_copy_when_the_backup_it_needs_has_gone() {
        let tmp = tempfile::tempdir().unwrap();
        let src = skill(tmp.path(), "demo");
        let dest = tmp.path().join("dest").join("demo");
        write(&dest, "SKILL.md", "mine
");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&backups).unwrap();

        let plan = plan_skill_copy(&src, &dest).unwrap();
        let receipt = apply_skill_copy(&plan, &backups, true).unwrap();
        let backup_dir = receipt.backup_dir.clone().expect("a replace makes a backup");

        // The backup goes missing between the copy and the Undo.
        fs::remove_dir_all(&backup_dir).unwrap();

        assert!(undo_skill_copy(&receipt).is_err());
        // The copied skill is still there: better one version than none.
        assert!(fs::read_to_string(dest.join("SKILL.md")).unwrap().contains("Body"));
    }

    /// A copy that stops halfway must put back what it replaced.
    ///
    /// It used to return leaving the person with nothing: the old folder
    /// already deleted, some new files on disk, and no receipt -- so the
    /// backup sat in GitWyrm's own data with nothing pointing at it and no
    /// way to undo. A file locked by the destination app is ordinary on
    /// Windows, so this is not a remote case.
    ///
    /// The failure is real, not simulated: the source holds both a file
    /// named `references` and a folder `references/api.md`, which cannot
    /// both exist at the destination. Writing the second one fails on every
    /// platform, needs no privilege, and happens after the old folder has
    /// already been deleted -- exactly the window that used to lose it.
    #[test]
    fn a_copy_that_fails_halfway_puts_the_old_skill_back() {
        let tmp = tempfile::tempdir().unwrap();

        // A source whose second write cannot succeed: `references` is a
        // file, and `references/api.md` needs it to be a directory.
        let src = tmp.path().join("demo");
        write(&src, "SKILL.md", "---
name: demo
---
Body
");
        write(&src, "references", "not a folder
");
        // Built by hand so the impossible pair reaches the write loop.
        let mut tree = read_skill_tree(&src).unwrap();
        tree.files.insert(
            "references/api.md".to_string(),
            SkillFile { bytes: b"ref
".to_vec(), hash: hash_bytes(b"ref
") },
        );

        let dest = tmp.path().join("dest").join("demo");
        write(&dest, "SKILL.md", "mine
");
        write(&dest, "keep.md", "my notes
");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&backups).unwrap();

        // The destination is deleted, SKILL.md lands, then the pair collides.
        let backup = backups.join("demo-backup");
        copy_tree(&dest, &backup).unwrap();
        fs::remove_dir_all(&dest).unwrap();
        let failed = write_tree(&tree, &dest);
        assert!(failed.is_err(), "the collision must fail the write");

        roll_back(&dest, Some(&backup.to_string_lossy().into_owned()));

        assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "mine
");
        assert_eq!(fs::read_to_string(dest.join("keep.md")).unwrap(), "my notes
");
        assert!(!dest.join("references").exists(), "the half-written copy is gone");
    }

    /// With no backup, the destination did not exist before -- so removing
    /// the half-written folder IS putting things back.
    #[test]
    fn a_failed_first_copy_leaves_no_half_written_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest").join("demo");
        write(&dest, "SKILL.md", "half
");

        roll_back(&dest, None);

        assert!(!dest.exists());
    }

    #[test]
    fn a_source_that_changed_since_the_preview_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let src = skill(tmp.path(), "demo");
        let dest = tmp.path().join("dest").join("demo");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(&backups).unwrap();

        let plan = plan_skill_copy(&src, &dest).unwrap();
        // Someone edits the skill while the preview is on screen.
        write(&src, "SKILL.md", "---\nname: demo\n---\nEdited\n");
        assert!(matches!(
            apply_skill_copy(&plan, &backups, false),
            Err(SkillCopyError::SourceChanged { .. })
        ));
        assert!(!dest.exists(), "a refused copy writes nothing at all");
    }

    #[test]
    fn nothing_is_ever_written_outside_the_destination_folder() {
        let root = Path::new("C:/dest/demo");
        for bad in ["../escape.md", "a/../../escape.md", "/absolute.md", ""] {
            assert!(
                matches!(safe_join(root, bad), Err(SkillCopyError::UnsafePath { .. })),
                "{bad} must be refused"
            );
        }
        assert!(safe_join(root, "references/api.md").is_ok());
    }

    #[test]
    fn a_missing_skill_says_so_rather_than_copying_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("not-here");
        assert!(matches!(
            read_skill_tree(&missing),
            Err(SkillCopyError::SourceMissing { .. })
        ));
        assert!(SkillCopyError::SourceMissing {
            path: "x".into()
        }
        .plain()
        .contains("not there"));
    }
}
