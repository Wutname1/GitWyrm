//! Worktrees: additional working folders for one repository.
//!
//! A linked worktree is a second checkout of the same repository, on its own
//! branch, in its own folder. Git stores the bookkeeping under
//! `.git/worktrees/<name>/` in the main repo and drops a `.git` *file* (not a
//! directory) in the linked folder pointing back at it.
//!
//! That two-ended link is what makes worktrees break in ways git reports badly:
//! delete the folder and the admin entry survives (prune it); move the folder
//! and the admin entry points nowhere (repair it); move the *main* repo and
//! every linked worktree loses its way home at once. Distinguishing those cases
//! is the whole point of [`WorktreeState`] -- the UI offers one action, and the
//! user is never asked to guess which.
//!
//! Reading goes through git2; anything that writes goes through shell git, per
//! the repo-wide split. `git worktree add` does a lot (creates the branch, the
//! admin files, and the `.git` link file, and runs a checkout) and
//! reimplementing it on top of git2 primitives would be a worse version of it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;
use crate::git::shell::run_git;

/// What kind of shape a worktree's folder is in.
///
/// Kept as three cases rather than a boolean because each has a different way
/// out: `Ok` needs nothing, `Missing` prunes, `Moved` repairs. Collapsing them
/// into "broken" would force the UI to offer both actions and let the user
/// pick, which is exactly the guess this type exists to remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum WorktreeState {
    /// The folder is there and is a real checkout.
    Ok,
    /// The folder is gone. The files are not on disk; pruning tidies the leftover
    /// reference and loses nothing that still exists.
    Missing,
    /// The admin entry points at something that is no longer a checkout, but the
    /// folder was not simply deleted -- typically it was moved. Repair points git
    /// at wherever it went.
    Moved,
}

/// One checkout of the repository: the main one, or a linked worktree.
///
/// The main checkout is included in listings so the section shows the whole
/// picture rather than "the other ones", which reads as if the folder the user
/// is looking at is not a worktree at all.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Worktree {
    /// Git's name for the worktree (its folder under `.git/worktrees`). Empty for
    /// the main checkout, which git does not name.
    pub name: String,
    /// The folder name shown in the list -- the last component of `path`.
    pub folder_name: String,
    /// Absolute path of the working folder.
    pub path: String,
    /// The branch checked out here, short form. None when HEAD is detached.
    pub branch: Option<String>,
    /// Short sha of HEAD. Shown instead of a branch when detached.
    pub head_sha: Option<String>,
    /// True for the repository's original checkout.
    pub is_main: bool,
    /// True when this is the checkout the caller is looking at.
    pub is_current: bool,
    /// True when git has this worktree locked (`git worktree lock`), which blocks
    /// pruning. Distinct from the OS holding a file handle.
    pub is_locked: bool,
    pub state: WorktreeState,
    /// True when this worktree was created to run a Spec Desk task in.
    pub is_run_worktree: bool,
}

/// Modified and untracked counts for a worktree, kept apart all the way to the
/// UI.
///
/// "3 changes" hides the only case with no way back: an untracked file has
/// never been written to history, so discarding it is unrecoverable, while a
/// modified tracked file can always be got back. One number would make those
/// read the same.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Type)]
pub struct DirtyCount {
    pub modified: u32,
    pub untracked: u32,
}

impl DirtyCount {
    pub fn is_clean(&self) -> bool {
        self.modified == 0 && self.untracked == 0
    }
}

/// What happened when a removal was attempted.
///
/// Every non-success carries what its offer needs, because each one leads
/// somewhere different: dirt leads to keep-or-discard, a lock leads to Try
/// again, a partial removal leads to telling the user what state the folder was
/// left in. A string would force the UI to parse its way back to these.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoveOutcome {
    /// The worktree is gone. `branch` is what it had checked out, so the caller
    /// can offer to delete it as a follow-on; `branch_merged` says whether that
    /// offer is safe to make.
    Removed {
        branch: Option<String>,
        branch_merged: bool,
    },
    /// Refused: there is uncommitted work in it. Ask, then call again with a
    /// decision.
    RefusedDirty { modified: u32, untracked: u32 },
    /// Refused by the OS: something still has a file open in the folder. On
    /// Windows this is an editor, a terminal sitting in the folder, a dev server,
    /// or a scanner -- and it is the single most common removal failure.
    RefusedLocked { path: String },
    /// Some files were deleted and then it failed. Saying so is the honest
    /// report; "failed" would imply nothing happened, and "removed" would be a
    /// lie.
    PartiallyRemoved { path: String },
}

/// Where the user's work went when they chose to keep it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeptWork {
    /// Set aside as a stash on the main repository, recoverable from the Stashes
    /// list.
    Stashed { message: String },
    /// There was nothing to keep after all.
    NothingToKeep,
}

/// What to do with uncommitted work when removing a worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DirtyChoice {
    /// Stop and report the counts. The default: never destroy on the first ask.
    Refuse,
    /// Set the changes aside recoverably, then remove.
    Keep,
    /// Throw the changes away and remove. This is what `--force` compiles to
    /// after a plain-language confirm -- the user never passes a flag.
    Discard,
}

/// An ignored file a new worktree would not get, offered for copying.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct IgnoredFile {
    /// Path relative to the source checkout.
    pub path: String,
    pub size_bytes: f64,
}

/// Directory names that are rebuilt rather than carried. Copying an installed
/// dependency tree into a new worktree is slow, often wrong (native modules are
/// built against a path), and is not what "my env file is missing" needs.
const NEVER_COPY_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".turbo",
    ".cache",
    ".venv",
    "venv",
    "__pycache__",
    "vendor",
    ".gradle",
    "Pods",
    ".terraform",
    "coverage",
    ".pytest_cache",
    ".mypy_cache",
];

/// Above this, a single file is treated as generated output rather than local
/// configuration. Env files and editor settings are tiny; a 2 MB "ignored file"
/// is a database or a build artifact.
const MAX_COPY_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Cap on how many files the copy offer will list. A checkout with hundreds of
/// ignored files is not one where a checkbox list helps.
const MAX_COPY_FILES: usize = 50;

/// True when an ignored path sits inside a directory that is rebuilt, not
/// carried.
fn in_never_copy_dir(rel: &Path) -> bool {
    rel.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        NEVER_COPY_DIRS.iter().any(|d| *d == name)
    })
}

/// The short branch name a worktree has checked out, and its HEAD sha.
///
/// A linked worktree's HEAD lives in its own admin directory, so this reads the
/// worktree's own repository handle rather than the main one.
fn head_of(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(repo) = git2::Repository::open(path) else {
        return (None, None);
    };
    let Ok(head) = repo.head() else {
        return (None, None);
    };
    let sha = head.target().map(|oid| {
        let full = oid.to_string();
        full.chars().take(7).collect::<String>()
    });
    let branch = if head.is_branch() {
        head.shorthand().ok().map(str::to_string)
    } else {
        None
    };
    (branch, sha)
}

/// Classify a linked worktree's folder.
///
/// `Missing` and `Moved` are told apart by whether the folder is still there:
/// git's own validity check says "not valid" for both, and the two need
/// opposite actions. A folder that exists but no longer opens as a repository
/// counts as moved -- git's link into it is what broke, and repair is the
/// action that fixes a broken link.
fn classify(path: &Path) -> WorktreeState {
    if !path.exists() {
        return WorktreeState::Missing;
    }
    // A worktree folder always carries a `.git` file pointing at its admin
    // directory. If the folder is there but that link is gone or dangling, the
    // link is what needs repairing.
    if git2::Repository::open(path).is_ok() {
        WorktreeState::Ok
    } else {
        WorktreeState::Moved
    }
}

/// Marker file GitWyrm writes into a worktree it created for a Spec Desk run.
///
/// Stored in the worktree's admin directory rather than the working folder, so
/// it can never be committed, never shows up as an untracked change, and does
/// not confuse the hand-edit check that decides whether a discard may delete
/// the folder.
const RUN_MARKER: &str = "gitwyrm-run";

/// The admin directory git keeps for a linked worktree
/// (`<main>/.git/worktrees/<name>`).
fn admin_dir(main_repo: &git2::Repository, name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    Some(main_repo.path().join("worktrees").join(name))
}

/// Mark a worktree as belonging to a run, so the list can label it and discard
/// can find it.
pub fn mark_as_run_worktree(main_repo: &git2::Repository, name: &str) -> Result<(), AppError> {
    let Some(dir) = admin_dir(main_repo, name) else {
        return Ok(());
    };
    std::fs::write(dir.join(RUN_MARKER), b"1").map_err(AppError::Io)
}

fn is_run_worktree(main_repo: &git2::Repository, name: &str) -> bool {
    admin_dir(main_repo, name)
        .map(|d| d.join(RUN_MARKER).exists())
        .unwrap_or(false)
}

/// The repository's main working folder.
///
/// When called on a linked worktree's handle, `commondir` points at the main
/// repo's `.git`, whose parent is the main working folder. Falls back to this
/// repo's own workdir when it *is* the main checkout.
pub fn main_workdir(repo: &git2::Repository) -> Option<PathBuf> {
    let common = repo.commondir();
    // `commondir` is `<main>/.git`; its parent is the main working folder.
    if let Some(parent) = common.parent() {
        if parent.join(".git").exists() {
            return Some(parent.to_path_buf());
        }
    }
    repo.workdir().map(Path::to_path_buf)
}

/// Every checkout of this repository, main one first, then linked worktrees by
/// folder name.
///
/// `current_path` marks which row is the checkout the caller is looking at --
/// the list is built from the main repository regardless of which handle asked,
/// so two tabs of the same repository see the same list with a different row
/// marked.
pub fn list(repo: &git2::Repository, current_path: Option<&Path>) -> Vec<Worktree> {
    let mut out = Vec::new();

    let same = |a: &Path, b: &Path| -> bool {
        // Compare canonically where possible: `C:\code\x` and `C:/code/x` are the
        // same folder, and only canonicalize agrees.
        match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        }
    };

    if let Some(main) = main_workdir(repo) {
        let (branch, head_sha) = head_of(&main);
        let folder_name = main
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push(Worktree {
            name: String::new(),
            folder_name,
            path: main.to_string_lossy().into_owned(),
            branch,
            head_sha,
            is_main: true,
            is_current: current_path.map(|c| same(c, &main)).unwrap_or(false),
            is_locked: false,
            state: WorktreeState::Ok,
            is_run_worktree: false,
        });
    }

    // `worktrees()` on a linked worktree's handle still lists the whole set --
    // the admin directory is shared through commondir.
    let Ok(names) = repo.worktrees() else {
        return out;
    };

    let mut linked: Vec<Worktree> = Vec::new();
    for name in names.iter().flatten().flatten() {
        let Ok(wt) = repo.find_worktree(name) else {
            continue;
        };
        let path = wt.path().to_path_buf();
        let state = classify(&path);
        let (branch, head_sha) = match state {
            WorktreeState::Ok => head_of(&path),
            // A folder that is gone or unreadable has no HEAD to read. Leaving these
            // None is what makes a broken row show its explanation instead of a
            // branch that is not really checked out anywhere.
            _ => (None, None),
        };
        let folder_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.to_string());

        linked.push(Worktree {
            name: name.to_string(),
            folder_name,
            path: path.to_string_lossy().into_owned(),
            branch,
            head_sha,
            is_main: false,
            is_current: current_path.map(|c| same(c, &path)).unwrap_or(false),
            is_locked: wt
                .is_locked()
                .map(|l| !matches!(l, git2::WorktreeLockStatus::Unlocked))
                .unwrap_or(false),
            state,
            is_run_worktree: is_run_worktree(repo, name),
        });
    }

    linked.sort_by(|a, b| {
        a.folder_name
            .to_lowercase()
            .cmp(&b.folder_name.to_lowercase())
    });
    out.extend(linked);
    out
}

/// The worktree holding `branch`, if any -- including the main checkout.
///
/// This is what turns "fatal: 'x' is already checked out at ..." into an offer
/// to open that folder. Returned as the whole [`Worktree`] because the caller
/// needs its path to open it, not just a name to print.
///
/// `repo`'s own workdir is passed through as the current path so the returned
/// worktree carries a meaningful `is_current`. Callers gate on that to tell
/// "some other folder has it" (a real refusal) from "you already have it here"
/// (not a refusal at all) -- with `None` every worktree looked like someone
/// else's, so switching to the branch you were already on was rejected.
pub fn holder_of_branch(repo: &git2::Repository, branch: &str) -> Option<Worktree> {
    let branch = branch.trim();
    if branch.is_empty() {
        return None;
    }
    list(repo, repo.workdir())
        .into_iter()
        .find(|wt| wt.state == WorktreeState::Ok && wt.branch.as_deref() == Some(branch))
}

/// Modified and untracked counts for a checkout at `path`.
///
/// Opens the worktree's own handle rather than asking the main repository:
/// status is per-checkout, and the main repo would report its own tree.
pub fn dirty_count(path: &Path) -> Result<DirtyCount, AppError> {
    let repo = git2::Repository::open(path)
        .map_err(|_| AppError::Other("that folder is not a checkout of this project".into()))?;

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo.statuses(Some(&mut opts))?;

    let mut count = DirtyCount::default();
    for entry in statuses.iter() {
        let s = entry.status();
        if s.contains(git2::Status::WT_NEW) {
            count.untracked += 1;
        } else if s.intersects(
            git2::Status::WT_MODIFIED
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE
                | git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED
                | git2::Status::INDEX_DELETED
                | git2::Status::INDEX_RENAMED
                | git2::Status::INDEX_TYPECHANGE,
        ) {
            count.modified += 1;
        }
    }
    Ok(count)
}

/// True when `branch` is fully contained in another local branch or its
/// upstream -- i.e. deleting it loses no commit that exists nowhere else.
///
/// Deliberately conservative: anything it cannot prove is merged is reported
/// unmerged, so the follow-on "delete the branch too" offer never appears over
/// work that only exists there.
pub fn branch_is_merged(repo: &git2::Repository, branch: &str) -> bool {
    let Ok(target) = repo.find_branch(branch, git2::BranchType::Local) else {
        return false;
    };
    let Some(tip) = target.get().target() else {
        return false;
    };

    // Merged into its own upstream is the common case for a finished branch.
    if let Ok(upstream) = target.upstream() {
        if let Some(up) = upstream.get().target() {
            if repo.graph_descendant_of(up, tip).unwrap_or(false) || up == tip {
                return true;
            }
        }
    }

    let Ok(branches) = repo.branches(Some(git2::BranchType::Local)) else {
        return false;
    };
    for entry in branches.flatten() {
        let (other, _) = entry;
        let Ok(Some(name)) = other.name().map(|n| n.map(str::to_string)) else {
            continue;
        };
        if name == branch {
            continue;
        }
        let Some(other_tip) = other.get().target() else {
            continue;
        };
        // Equality counts as merged and `graph_descendant_of` reports it false: a
        // branch sitting exactly at another branch's tip has no commit of its own,
        // which is the most common shape for a finished worktree branch.
        if other_tip == tip || repo.graph_descendant_of(other_tip, tip).unwrap_or(false) {
            return true;
        }
    }
    false
}

/// Ignored files in `source` that a new worktree would not get.
///
/// A fresh worktree contains only tracked files, so local env files, editor
/// settings, and credentials are simply absent -- and git says nothing about
/// it, which is why a new worktree so often looks broken for no visible reason.
/// Generated directories are excluded outright: they are rebuilt, and offering
/// to copy a dependency tree would turn a helpful checkbox into a mistake.
pub fn ignored_files_worth_copying(source: &Path) -> Vec<IgnoredFile> {
    let Ok(repo) = git2::Repository::open(source) else {
        return Vec::new();
    };

    let mut opts = git2::StatusOptions::new();
    opts.include_ignored(true)
        .include_untracked(true)
        .recurse_untracked_dirs(false)
        // Listing every file inside an ignored directory would enumerate
        // node_modules; the directory entry alone is enough to reject it.
        .recurse_ignored_dirs(false)
        .include_unmodified(false);

    let Ok(statuses) = repo.statuses(Some(&mut opts)) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in statuses.iter() {
        if !entry.status().contains(git2::Status::IGNORED) {
            continue;
        }
        let Ok(rel) = entry.path() else { continue };
        let rel_path = Path::new(rel);

        if in_never_copy_dir(rel_path) {
            continue;
        }
        // A trailing slash means git reported a whole ignored directory. Those are
        // the generated ones we do not carry.
        if rel.ends_with('/') {
            continue;
        }

        let abs = source.join(rel_path);
        let Ok(meta) = std::fs::metadata(&abs) else {
            continue;
        };
        if meta.is_dir() || meta.len() > MAX_COPY_FILE_BYTES {
            continue;
        }

        out.push(IgnoredFile {
            path: rel.replace('\\', "/"),
            size_bytes: meta.len() as f64,
        });
        if out.len() >= MAX_COPY_FILES {
            break;
        }
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Copy chosen ignored files from `source` into `dest`, creating parent folders.
/// Best-effort per file: one unreadable file must not fail a created worktree.
pub fn copy_ignored_files(source: &Path, dest: &Path, files: &[String]) -> usize {
    let mut copied = 0;
    for rel in files {
        // Never let a caller-supplied path escape the source checkout.
        let rel_path = Path::new(rel);
        if rel_path.is_absolute() || rel_path.components().any(|c| c.as_os_str() == "..") {
            continue;
        }
        let from = source.join(rel_path);
        let to = dest.join(rel_path);
        if !from.is_file() {
            continue;
        }
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::copy(&from, &to).is_ok() {
            copied += 1;
        }
    }
    copied
}

/// Turn a branch name into a folder-safe slug for the suggested path.
pub fn branch_slug(branch: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in branch.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') || slug.ends_with('.') {
        slug.pop();
    }
    if slug.is_empty() {
        "worktree".to_string()
    } else {
        slug
    }
}

/// Suggest `../<repo-name>-<branch-slug>`, de-duplicated if that folder exists.
///
/// A sibling of the repository, so the new checkout can never be committed into
/// it -- a worktree inside the working tree is a thing to gitignore correctly
/// forever, and getting it wrong commits an entire second checkout.
pub fn suggest_path(main_workdir: &Path, branch: &str) -> String {
    let repo_name = main_workdir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    let parent = main_workdir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| main_workdir.to_path_buf());

    let base = format!("{repo_name}-{}", branch_slug(branch));
    let mut candidate = parent.join(&base);
    let mut n = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{base}-{n}"));
        n += 1;
        if n > 100 {
            break;
        }
    }
    candidate.to_string_lossy().replace('\\', "/")
}

/// Why a worktree cannot be created at `path`, in plain words. None when it is
/// fine.
///
/// Checked before anything is written, so a bad choice is a sentence in the
/// dialog rather than a half-created folder to clean up.
pub fn path_problem(main_workdir: &Path, path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return Some("Choose a folder for the new worktree.".into());
    }

    if path.exists() {
        if !path.is_dir() {
            return Some(
                "That path is a file, not a folder. Pick a folder name that does not exist yet."
                    .into(),
            );
        }
        let empty = std::fs::read_dir(path)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
        if !empty {
            return Some(
                "That folder already has files in it. Pick an empty folder or a new folder name."
                    .into(),
            );
        }
    }

    // Inside the working tree, the new checkout would show up as untracked files
    // in the repository it came from -- and could be committed into it.
    let inside = match (path.canonicalize(), main_workdir.canonicalize()) {
        (Ok(p), Ok(m)) => p.starts_with(&m),
        _ => path.starts_with(main_workdir),
    };
    if inside {
        return Some(
      "That folder is inside the project, so the new copy of your files could get committed into it by accident. Pick a folder outside the project."
        .into(),
    );
    }

    None
}

/// Add a worktree at `path` for `branch`.
///
/// `new_branch` creates the branch as part of the add (git's `-b`), starting
/// from `start_point` or HEAD. Validation happens before the shell-out so a
/// refusal never leaves a half-made folder behind.
pub fn add(
    repo_path: &str,
    path: &str,
    branch: &str,
    new_branch: bool,
    start_point: Option<&str>,
) -> Result<(), AppError> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err(AppError::Other(
            "Choose a branch for the new worktree.".into(),
        ));
    }
    if branch.starts_with('-') || path.trim().starts_with('-') {
        return Err(AppError::Other("Names cannot start with '-'.".into()));
    }

    let mut args: Vec<&str> = vec!["worktree", "add"];
    if new_branch {
        args.push("-b");
        args.push(branch);
    }
    // `--` keeps a path that looks like a flag from being read as one.
    args.push("--");
    args.push(path);
    if new_branch {
        if let Some(start) = start_point {
            if !start.trim().is_empty() {
                args.push(start);
            }
        }
    } else {
        args.push(branch);
    }

    run_git(Some(repo_path), &args).map_err(explain_add_failure)?;
    Ok(())
}

/// Rewrite git's add failures into something a person can act on.
fn explain_add_failure(err: AppError) -> AppError {
    let text = err.to_string();
    let lower = text.to_lowercase();
    // Git words this two ways depending on version and code path: "is already
    // checked out at" and "is already used by worktree at". Both mean the same
    // thing to the user, and neither says what to do about it.
    if lower.contains("already checked out") || lower.contains("already used by worktree") {
        return AppError::Other(
      "That branch is already open in another worktree. A branch can only be checked out in one folder at a time.".into(),
    );
    }
    if lower.contains("already exists") && lower.contains("branch") {
        return AppError::Other("A branch with that name already exists.".into());
    }
    if lower.contains("not a valid object name") || lower.contains("invalid reference") {
        return AppError::Other("That starting point does not exist in this project.".into());
    }
    if lower.contains("is not an empty directory") {
        return AppError::Other(
            "That folder already has files in it. Pick an empty folder or a new folder name."
                .into(),
        );
    }
    err
}

/// True when a failure text describes the OS refusing to release a file.
///
/// Windows will not delete a directory anything holds a handle in, and git
/// reports that as a permission or "being used by another process" error that
/// says nothing about which program or what to do.
pub fn looks_like_file_lock(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("being used by another process")
        || lower.contains("access is denied")
        || lower.contains("permission denied")
        || lower.contains("resource busy")
        || lower.contains("os error 5")
        || lower.contains("os error 32")
        || lower.contains("device or resource busy")
        || lower.contains("text file busy")
}

/// Remove a linked worktree.
///
/// Refuses the main checkout and the currently open one outright -- removing
/// the folder out from under the window the user is looking at is never what
/// they meant. `choice` decides what happens to uncommitted work, and the
/// caller only sets anything but `Refuse` after its own plain-language confirm:
/// `--force` is what "discard them and remove" compiles to, not something the
/// user passes.
pub fn remove(
    repo: &git2::Repository,
    repo_path: &str,
    target_path: &str,
    choice: DirtyChoice,
) -> Result<RemoveOutcome, AppError> {
    let target = PathBuf::from(target_path);

    let all = list(repo, None);
    let entry = all
        .iter()
        .find(|wt| paths_equal(Path::new(&wt.path), &target))
        .ok_or_else(|| AppError::Other("That worktree is not part of this project.".into()))?;

    if entry.is_main {
        return Err(AppError::Other(
            "That is the project's original folder, so it cannot be removed here.".into(),
        ));
    }

    let branch = entry.branch.clone();
    let state = entry.state;

    // A folder that is already gone is a prune, not a remove.
    if state == WorktreeState::Missing {
        let _ = prune(repo);
        return Ok(RemoveOutcome::Removed {
            branch: branch.clone(),
            branch_merged: branch
                .as_deref()
                .map(|b| branch_is_merged(repo, b))
                .unwrap_or(false),
        });
    }

    // Ask about uncommitted work before touching anything.
    let dirt = dirty_count(&target).unwrap_or_default();
    if !dirt.is_clean() {
        match choice {
            DirtyChoice::Refuse => {
                return Ok(RemoveOutcome::RefusedDirty {
                    modified: dirt.modified,
                    untracked: dirt.untracked,
                });
            }
            DirtyChoice::Keep => {
                keep_work_aside(&target)?;
            }
            DirtyChoice::Discard => {}
        }
    }

    let force_needed = !dirt.is_clean() || entry.is_locked;
    let mut args: Vec<&str> = vec!["worktree", "remove"];
    if force_needed {
        args.push("--force");
    }
    args.push("--");
    args.push(target_path);

    match run_git(Some(repo_path), &args) {
        Ok(_) => {}
        Err(e) => {
            let text = e.to_string();
            if looks_like_file_lock(&text) {
                // Some files may already be gone. Saying which case it is beats
                // reporting a bare failure over a folder that is now half there.
                if target.exists() && !git2::Repository::open(&target).is_ok() {
                    return Ok(RemoveOutcome::PartiallyRemoved {
                        path: target_path.to_string(),
                    });
                }
                return Ok(RemoveOutcome::RefusedLocked {
                    path: target_path.to_string(),
                });
            }
            return Err(e);
        }
    }

    // git leaves the admin entry when the folder could not be fully cleared.
    let _ = prune(repo);

    let branch_merged = branch
        .as_deref()
        .map(|b| branch_is_merged(repo, b))
        .unwrap_or(false);
    Ok(RemoveOutcome::Removed {
        branch,
        branch_merged,
    })
}

/// Set a worktree's uncommitted work aside so removing the folder does not
/// destroy it.
///
/// Stashes inside the worktree itself: a stash is stored in the shared object
/// database and listed on `refs/stash`, which every checkout of the repository
/// shares -- so the entry survives the folder it was made in and shows up in
/// the main checkout's Stashes list, which is where the user will look.
fn keep_work_aside(target: &Path) -> Result<KeptWork, AppError> {
    let mut repo = git2::Repository::open(target)
        .map_err(|_| AppError::Other("that folder is not a checkout of this project".into()))?;

    let folder = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "worktree".to_string());
    let message = format!("gitwyrm: your work from the {folder} folder, kept before removing it");

    let signature = repo.signature()?;
    match repo.stash_save(
        &signature,
        &message,
        // Untracked files are the whole reason to offer keeping: they exist
        // nowhere else, so leaving them out would keep the recoverable half and
        // destroy the unrecoverable half.
        Some(git2::StashFlags::INCLUDE_UNTRACKED),
    ) {
        Ok(_) => Ok(KeptWork::Stashed { message }),
        Err(e) if e.class() == git2::ErrorClass::Stash && e.code() == git2::ErrorCode::NotFound => {
            Ok(KeptWork::NothingToKeep)
        }
        Err(e) => Err(e.into()),
    }
}

/// Same folder, tolerant of separator and case differences on Windows.
pub fn paths_equal(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            let norm = |p: &Path| p.to_string_lossy().to_lowercase().replace('\\', "/");
            norm(a) == norm(b)
        }
    }
}

/// Drop admin entries for worktrees whose folders are gone.
pub fn prune(repo: &git2::Repository) -> Result<u32, AppError> {
    let Ok(names) = repo.worktrees() else {
        return Ok(0);
    };
    let mut pruned = 0;
    for name in names.iter().flatten().flatten() {
        let Ok(wt) = repo.find_worktree(name) else {
            continue;
        };
        // Only prune what is actually gone. `is_prunable` also reports locked
        // worktrees as not prunable, which is the behaviour we want.
        if wt.is_prunable(None).unwrap_or(false) {
            let mut opts = git2::WorktreePruneOptions::new();
            opts.working_tree(true);
            if wt.prune(Some(&mut opts)).is_ok() {
                pruned += 1;
            }
        }
    }
    Ok(pruned)
}

/// Point git at a worktree that was moved.
///
/// `new_path` is where the folder went. Repair is run from the main repository
/// AND from the moved folder, because the link has two ends: the admin entry
/// records the folder's path, and the folder's `.git` file records the admin
/// directory. Moving either end breaks one of them, and only running both
/// directions fixes both cases without asking the user which happened.
pub fn repair(repo_path: &str, new_path: Option<&str>) -> Result<(), AppError> {
    match new_path {
        Some(p) if !p.trim().is_empty() => {
            run_git(Some(repo_path), &["worktree", "repair", "--", p])?;
            // From the moved folder, repair fixes its own pointer back to the main
            // repository -- the case where the main repo moved instead.
            let _ = run_git(Some(p), &["worktree", "repair"]);
        }
        _ => {
            // No path given: the main repository moved, so every linked worktree
            // broke at once. Repair from the main repo fixes them together.
            run_git(Some(repo_path), &["worktree", "repair"])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: `holder_of_branch` used to call `list(repo, None)`, so no
    /// worktree ever came back as `is_current`. The repo's own checkout then
    /// looked like somebody else's, and callers refused to act on the branch the
    /// user was already standing on ("already checked out in another worktree").
    #[test]
    fn the_repos_own_checkout_is_reported_as_current() {
        let dir = tempfile::tempdir().expect("temp repo");
        let repo = git2::Repository::init(dir.path()).expect("repo");
        {
            let mut config = repo.config().expect("config");
            config.set_str("user.name", "Worktree Test").expect("name");
            config
                .set_str("user.email", "worktree@example.com")
                .expect("email");
        }
        std::fs::write(dir.path().join("a.txt"), "a").expect("write");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("a.txt")).expect("add");
        index.write().expect("write index");
        let tree = repo
            .find_tree(index.write_tree().expect("tree id"))
            .expect("tree");
        let sig = git2::Signature::now("Worktree Test", "worktree@example.com").expect("sig");
        repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .expect("commit");

        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        let holder = holder_of_branch(&repo, &branch).expect("the checkout holding the branch");

        assert!(
            holder.is_current,
            "the repo's own checkout must not look like another worktree holding the branch"
        );
    }

    #[test]
    fn slugs_are_folder_safe() {
        assert_eq!(branch_slug("feature/login-fix"), "feature-login-fix");
        assert_eq!(branch_slug("Release/2.0"), "release-2.0");
        assert_eq!(branch_slug("bug#42"), "bug-42");
        assert_eq!(branch_slug("///"), "worktree");
        assert_eq!(branch_slug(""), "worktree");
    }

    #[test]
    fn slug_has_no_trailing_separator() {
        assert_eq!(branch_slug("feature/"), "feature");
        assert_eq!(branch_slug("wip--"), "wip");
    }

    #[test]
    fn suggested_path_is_a_sibling() {
        let main = Path::new("C:/code/GitWyrm");
        let suggested = suggest_path(main, "feature/login");
        assert_eq!(suggested, "C:/code/GitWyrm-feature-login");
        // Outside the repository, so the checkout can never be committed into it.
        assert!(!suggested.starts_with("C:/code/GitWyrm/"));
    }

    #[test]
    fn dependency_folders_are_never_offered() {
        assert!(in_never_copy_dir(Path::new("node_modules/react/index.js")));
        assert!(in_never_copy_dir(Path::new("src/../target/debug/x")));
        assert!(in_never_copy_dir(Path::new(".venv/lib/python3/site.py")));
        assert!(!in_never_copy_dir(Path::new(".env.local")));
        assert!(!in_never_copy_dir(Path::new("src/config/.env")));
    }

    #[test]
    fn windows_lock_errors_are_recognised() {
        assert!(looks_like_file_lock(
      "The process cannot access the file because it is being used by another process. (os error 32)"
    ));
        assert!(looks_like_file_lock("Access is denied. (os error 5)"));
        assert!(looks_like_file_lock("permission denied"));
        assert!(!looks_like_file_lock("fatal: 'x' is already checked out"));
        assert!(!looks_like_file_lock("not a working tree"));
    }

    #[test]
    fn dirty_count_clean_only_when_both_zero() {
        assert!(DirtyCount {
            modified: 0,
            untracked: 0
        }
        .is_clean());
        assert!(!DirtyCount {
            modified: 1,
            untracked: 0
        }
        .is_clean());
        assert!(!DirtyCount {
            modified: 0,
            untracked: 1
        }
        .is_clean());
    }

    #[test]
    fn missing_folder_classifies_as_missing() {
        let path = std::env::temp_dir().join("gitwyrm-no-such-worktree-xyz");
        let _ = std::fs::remove_dir_all(&path);
        assert_eq!(classify(&path), WorktreeState::Missing);
    }

    #[test]
    fn folder_without_git_link_classifies_as_moved() {
        let path = std::env::temp_dir().join(format!("gitwyrm-moved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        // Exists but is not a checkout: the link is what broke, so repair, not prune.
        assert_eq!(classify(&path), WorktreeState::Moved);
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn add_rejects_flag_like_names() {
        let err = add("C:/nope", "--evil", "main", false, None).unwrap_err();
        assert!(err.to_string().contains("cannot start with"));
    }

    #[test]
    fn add_requires_a_branch() {
        let err = add("C:/nope", "C:/tmp/x", "   ", false, None).unwrap_err();
        assert!(err.to_string().contains("Choose a branch"));
    }

    #[test]
    fn path_problem_rejects_inside_the_project() {
        let main = std::env::temp_dir().join(format!("gitwyrm-pp-{}", std::process::id()));
        let inside = main.join("nested");
        let _ = std::fs::create_dir_all(&main);
        let problem = path_problem(&main, &inside).expect("inside the project is refused");
        assert!(problem.contains("inside the project"));
        // The message must explain the consequence, not just name the rule.
        assert!(problem.contains("committed"));
        let _ = std::fs::remove_dir_all(&main);
    }

    #[test]
    fn path_problem_rejects_a_non_empty_folder() {
        let root = std::env::temp_dir().join(format!("gitwyrm-pp2-{}", std::process::id()));
        let main = root.join("repo");
        let target = root.join("busy");
        let _ = std::fs::create_dir_all(&main);
        let _ = std::fs::create_dir_all(&target);
        std::fs::write(target.join("a.txt"), b"x").unwrap();
        let problem = path_problem(&main, &target).expect("non-empty folder is refused");
        assert!(problem.contains("already has files"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_problem_accepts_an_empty_sibling() {
        let root = std::env::temp_dir().join(format!("gitwyrm-pp3-{}", std::process::id()));
        let main = root.join("repo");
        let target = root.join("repo-feature");
        let _ = std::fs::create_dir_all(&main);
        assert_eq!(path_problem(&main, &target), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_refuses_to_escape_the_source() {
        let root = std::env::temp_dir().join(format!("gitwyrm-cp-{}", std::process::id()));
        let src = root.join("src");
        let dest = root.join("dest");
        let _ = std::fs::create_dir_all(&src);
        let _ = std::fs::create_dir_all(&dest);
        std::fs::write(root.join("secret.txt"), b"nope").unwrap();

        let copied = copy_ignored_files(&src, &dest, &["../secret.txt".to_string()]);
        assert_eq!(copied, 0);
        assert!(!dest.join("secret.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_carries_a_nested_env_file() {
        let root = std::env::temp_dir().join(format!("gitwyrm-cp2-{}", std::process::id()));
        let src = root.join("src");
        let dest = root.join("dest");
        let _ = std::fs::create_dir_all(src.join("config"));
        let _ = std::fs::create_dir_all(&dest);
        std::fs::write(src.join("config/.env"), b"KEY=1").unwrap();

        let copied = copy_ignored_files(&src, &dest, &["config/.env".to_string()]);
        assert_eq!(copied, 1);
        assert_eq!(
            std::fs::read_to_string(dest.join("config/.env")).unwrap(),
            "KEY=1"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
