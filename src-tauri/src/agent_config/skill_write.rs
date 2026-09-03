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

    let mut written = 0usize;
    for (rel, file) in &tree.files {
        let target = safe_join(&plan.destination_dir, rel)?;
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

    Ok(SkillCopyReceipt {
        destination_dir: plan.destination_dir.to_string_lossy().into_owned(),
        backup_dir,
        files_written: written,
    })
}

/// Puts back whatever the copy replaced.
///
/// A receipt with no backup means the folder did not exist before, so undo
/// removes what was copied rather than leaving an empty skill behind.
pub fn undo_skill_copy(receipt: &SkillCopyReceipt) -> Result<(), SkillCopyError> {
    let destination = PathBuf::from(&receipt.destination_dir);
    if destination.is_dir() {
        std::fs::remove_dir_all(&destination).map_err(|e| SkillCopyError::WriteFailed {
            path: receipt.destination_dir.clone(),
            detail: e.to_string(),
        })?;
    }
    let Some(backup) = &receipt.backup_dir else {
        return Ok(());
    };
    copy_tree(Path::new(backup), &destination)
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), SkillCopyError> {
    let tree = read_skill_tree(from)?;
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
    }
    Ok(())
}

/// Joins a relative key under `root`, refusing anything that would escape it.
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
