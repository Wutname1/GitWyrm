//! Submodule inspection helpers.
//!
//! A submodule is a pinned pointer: the parent repo records the exact commit a
//! nested repo should sit at. When the nested checkout sits at a different
//! commit, the parent shows the submodule path as "modified" -- but ordinary
//! file operations (stash, discard-via-checkout) can't touch it, which is why a
//! moved submodule leaves the user stuck. These helpers surface what actually
//! moved so the UI can explain it and offer submodule-specific actions.

use std::collections::HashMap;

use crate::error::AppError;
use crate::git::head_attach::{attach_head, head_has_unsaved_commits, AttachMode, AttachOutcome};
use crate::git::shell::run_git;
use crate::git::types::{SubmoduleFollowed, SubmoduleMove, SubmoduleState, SubmoduleStatus};

/// Re-read `.git/index` when it changed underneath us.
///
/// The app keeps one long-lived `git2::Repository` per repo, but pull, rebase
/// and clone shell out to git.exe, which rewrites the index behind libgit2's
/// back. A handle opened before that runs still answers from the index it
/// cached, so the gitlink it reports is the OLD commit -- and a submodule
/// pointer that just moved reads as not moved at all. The move is then never
/// followed and the user is left holding a pending change nobody made.
///
/// `read(false)` is a stat check that reloads only when the file actually
/// changed, so calling it before every read costs nothing when nothing moved.
fn refresh_index(repo: &git2::Repository) {
    if let Ok(mut index) = repo.index() {
        let _ = index.read(false);
    }
}

/// A path -> pointer-move map for every submodule whose workdir HEAD differs
/// from the commit the parent repo records. Paths not present are in sync (or
/// not submodules). Uninitialized submodules are included with `initialized:
/// false` and no workdir sha.
pub fn moved_submodules(repo: &git2::Repository) -> HashMap<String, SubmoduleMove> {
    let mut moves = HashMap::new();

    refresh_index(repo);
    let Ok(subs) = repo.submodules() else {
        return moves;
    };

    for sub in subs {
        let Some(path) = sub.path().to_str().map(str::to_string) else {
            continue;
        };

        // The commit the parent repo pins (from its index/HEAD).
        let recorded = sub.index_id().or_else(|| sub.head_id());
        // The commit the nested checkout currently sits at.
        let checked_out = sub.workdir_id();

        match (recorded, checked_out) {
            (Some(recorded), Some(checked_out)) if recorded != checked_out => {
                // The submodule's commits live in the submodule's own object database,
                // not the parent's -- so ahead/behind must be computed against the
                // nested repo. If it can't be opened, fall back to unknown (0/0).
                let (ahead, behind) = sub
                    .open()
                    .ok()
                    .and_then(|nested| {
                        nested
                            .graph_ahead_behind(checked_out, recorded)
                            .ok()
                            .map(|(a, b)| (a as u32, b as u32))
                    })
                    .unwrap_or((0, 0));
                moves.insert(
                    path.clone(),
                    SubmoduleMove {
                        path,
                        recorded_sha: recorded.to_string(),
                        workdir_sha: Some(checked_out.to_string()),
                        ahead,
                        behind,
                        initialized: true,
                    },
                );
            }
            // Recorded but not checked out anywhere: the submodule isn't initialized.
            (Some(recorded), None) => {
                moves.insert(
                    path.clone(),
                    SubmoduleMove {
                        path,
                        recorded_sha: recorded.to_string(),
                        workdir_sha: None,
                        ahead: 0,
                        behind: 0,
                        initialized: false,
                    },
                );
            }
            _ => {}
        }
    }

    moves
}

/// Every submodule in the repo, in sync or not, sorted by path.
///
/// This is the list view's source: unlike [`moved_submodules`], a healthy
/// submodule still appears, because "all my submodules are fine" is something
/// the user needs to be able to see rather than infer from an empty list.
///
/// ahead/behind are computed against the submodule's OWN repo -- the nested
/// commits are not in the parent's object database, so asking the parent
/// returns 0/0.
pub fn all_submodules(repo: &git2::Repository) -> Vec<SubmoduleStatus> {
    refresh_index(repo);
    let Ok(subs) = repo.submodules() else {
        return Vec::new();
    };

    // The index is the source of truth for what the repo currently tracks. A
    // staged-but-uncommitted removal is gone from the index while HEAD still has
    // the gitlink, so falling back to head_id would keep listing a submodule the
    // user just removed.
    let index_has_entry = |path: &str| {
        repo.index()
            .ok()
            .map(|idx| idx.get_path(std::path::Path::new(path), 0).is_some())
            .unwrap_or(false)
    };

    let mut out = Vec::new();
    for sub in subs {
        let Some(path) = sub.path().to_str().map(str::to_string) else {
            continue;
        };
        let recorded = match sub.index_id() {
            Some(oid) => oid,
            // Not in the index: only real if HEAD still has it AND it was not just
            // removed. `git rm` drops the index entry, which is what distinguishes
            // "removal pending commit" from "index simply not loaded".
            None => match sub.head_id() {
                Some(oid) if index_has_entry(&path) => oid,
                _ => continue,
            },
        };
        let checked_out = sub.workdir_id();

        let (state, ahead, behind) = match checked_out {
            None => (SubmoduleState::Uninitialized, 0, 0),
            Some(at) if at == recorded => (SubmoduleState::InSync, 0, 0),
            Some(at) => {
                let (a, b) = sub
                    .open()
                    .ok()
                    .and_then(|nested| {
                        nested
                            .graph_ahead_behind(at, recorded)
                            .ok()
                            .map(|(a, b)| (a as u32, b as u32))
                    })
                    .unwrap_or((0, 0));
                (SubmoduleState::Moved, a, b)
            }
        };

        // Which branch the nested checkout is on, if any. A detached HEAD is
        // what `git submodule update` leaves behind, so this is common rather
        // than broken -- it just has to be visible.
        let head_branch = sub.open().ok().and_then(|nested| {
            let head = nested.head().ok()?;
            if head.is_branch() {
                head.shorthand().ok().map(str::to_string)
            } else {
                None
            }
        });

        out.push(SubmoduleStatus {
            name: sub.name().unwrap_or(&path).to_string(),
            url: sub.url().ok().flatten().map(str::to_string),
            branch: sub.branch().ok().flatten().map(str::to_string),
            recorded_sha: recorded.to_string(),
            workdir_sha: checked_out.map(|o| o.to_string()),
            ahead,
            behind,
            head_branch,
            state,
            path,
        });
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The commit each submodule is pinned to right now, by path.
///
/// Taken before an operation that can move pins (pull, rebase, branch switch),
/// so the follow-up afterwards can tell a pin the operation moved from a
/// submodule the user moved themselves. Only the first should be followed.
pub fn recorded_pins(repo: &git2::Repository) -> HashMap<String, git2::Oid> {
    refresh_index(repo);
    let Ok(subs) = repo.submodules() else {
        return HashMap::new();
    };
    subs.iter()
        .filter_map(|sub| {
            let path = sub.path().to_str()?.to_string();
            let pin = sub.index_id().or_else(|| sub.head_id())?;
            Some((path, pin))
        })
        .collect()
}

/// Put a submodule's checkout on its branch after something left it detached,
/// without moving it off the commit it sits at. Best effort: a submodule that
/// cannot be attached simply stays where it is, which is still correct.
pub fn attach_submodule(repo_path: &str, sub_path: &str) {
    let branch = git2::Repository::open(repo_path)
        .ok()
        .and_then(|parent| followed_branch(&parent, sub_path));
    let branch = branch.as_deref();
    let nested = std::path::Path::new(repo_path).join(sub_path);
    let Ok(nested_repo) = git2::Repository::open(&nested) else {
        return;
    };
    let nested = nested.to_string_lossy().into_owned();
    if let AttachOutcome::NotAttached { reason } =
        attach_head(&nested_repo, &nested, branch, AttachMode::KeepCommit)
    {
        log::info!("left submodule {sub_path} off a branch: {reason}");
    }
}

/// The branch `.gitmodules` says a submodule follows, if any.
fn followed_branch(repo: &git2::Repository, sub_path: &str) -> Option<String> {
    repo.find_submodule(sub_path)
        .ok()?
        .branch()
        .ok()
        .flatten()
        .map(str::to_string)
}

/// True when `path` names a submodule in this repo (moved or not).
pub fn is_submodule(repo: &git2::Repository, path: &str) -> bool {
    repo.find_submodule(path).is_ok()
}

/// Check out every initialized submodule at the commit the parent now records.
///
/// Operations that write a tree -- cherry-pick, revert, merge -- move the
/// gitlink in the index but leave the nested checkout where it was. The commit
/// then lands correctly while the working tree reports the submodule as
/// modified, so the repo is dirty the instant the operation "succeeds" and the
/// next operation refuses to start. Running this afterwards makes the checkout
/// match what was just committed.
///
/// Uninitialized submodules are skipped: there is no checkout to move, and
/// downloading one is a separate, explicit user action. Failures are ignored
/// per-submodule -- a submodule whose commit is missing locally must not fail
/// the operation that already committed successfully.
pub fn sync_submodule_workdirs(repo: &git2::Repository) {
    let Ok(subs) = repo.submodules() else {
        return;
    };

    for mut sub in subs {
        // No workdir id means it was never checked out; leave it alone.
        if sub.workdir_id().is_none() {
            continue;
        }
        // Re-read so index_id reflects the tree just written.
        let _ = sub.reload(true);
        let (Some(recorded), Some(checked_out)) = (sub.index_id(), sub.workdir_id()) else {
            continue;
        };
        if recorded == checked_out {
            continue;
        }
        let _ = sub.update(false, None);
    }
}

/// What happened to one submodule while following a pull.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowOutcome {
    /// Its checkout now sits at the commit the parent records.
    Updated,
    /// Edits inside it were set aside first, then it was updated. The user has a
    /// stash entry in the nested repo to recover.
    StashedAndUpdated,
    /// It could not be updated -- the reason is for the log, not the user.
    Failed(String),
}

/// One submodule's result from [`follow_recorded_pins`].
#[derive(Debug, Clone)]
pub struct FollowResult {
    pub path: String,
    pub outcome: FollowOutcome,
}

/// True when the nested checkout at `path` has edits of its own -- modified
/// tracked files or untracked ones. A moved pointer alone is not "dirty": that
/// lives in the PARENT's index, and is exactly what we are about to fix.
///
/// `--ignore-submodules=all` keeps a grandchild submodule's own pointer move
/// from reading as dirt here; the recursive update handles those separately.
fn nested_is_dirty(repo_path: &str, path: &str) -> bool {
    run_git(
        Some(repo_path),
        &[
            "-C",
            path,
            "status",
            "--porcelain",
            "--untracked-files=normal",
            "--ignore-submodules=all",
        ],
    )
    .map(|out| !out.stdout.trim().is_empty())
    .unwrap_or(false)
}

/// Move every initialized submodule to the commit the parent now records,
/// stashing local edits inside a submodule first so nothing is destroyed.
///
/// This is the step plain `git pull` leaves undone. Pulling a commit that moves
/// a submodule pointer updates the parent's index, but never touches the nested
/// checkout -- so the submodule stays at the OLD commit and the parent reports
/// it as a pending change. The user did not change anything, yet is handed a
/// modification they have to understand submodules to interpret.
///
/// Why shell out rather than reuse [`sync_submodule_workdirs`]: after a pull the
/// newly recorded commit is usually not in the nested repo's object database yet
/// -- nobody fetched it. libgit2's `Submodule::update` has no remote to ask and
/// fails. `git submodule update` fetches on demand, so it can actually land the
/// commit that just arrived.
///
/// Edits inside a submodule are stashed rather than carried across: a checkout
/// that would overwrite them fails outright, which would leave the pointer
/// stale and put us back at the symptom. The stash entry lives in the nested
/// repo, so the work is recoverable with a normal `stash pop` there.
///
/// Failures are per-submodule and never propagate: the pull already succeeded,
/// and a submodule whose commit is unreachable must not turn that into an error.
///
/// `pins_before` is [`recorded_pins`] taken before the operation. A submodule is
/// followed when the operation moved its pin, or when its checkout is simply
/// behind the pin (nothing of the user's to lose). One the USER moved ahead --
/// commits made inside it -- is left alone: following it would snap it back to
/// the old pin, off its branch, every time the parent was pulled.
pub fn follow_recorded_pins(
    repo: &git2::Repository,
    repo_path: &str,
    pins_before: &HashMap<String, git2::Oid>,
) -> Result<Vec<FollowResult>, AppError> {
    let mut results = Vec::new();

    let mut moved: Vec<SubmoduleMove> = moved_submodules(repo)
        .into_values()
        // An uninitialized submodule has no checkout to move. Downloading one is a
        // separate, explicit choice the user makes, not a side effect of pulling.
        .filter(|m| m.initialized)
        .filter(|m| {
            let pin_moved =
                pins_before.get(&m.path).map(|oid| oid.to_string()) != Some(m.recorded_sha.clone());
            let only_behind = m.ahead == 0 && m.behind > 0;
            pin_moved || only_behind
        })
        .collect();
    if moved.is_empty() {
        return Ok(results);
    }
    moved.sort_by(|a, b| a.path.cmp(&b.path));

    for m in moved {
        let nested = std::path::Path::new(repo_path)
            .join(&m.path)
            .to_string_lossy()
            .into_owned();
        let branch = followed_branch(repo, &m.path);

        // A detached checkout with commits of its own would lose them from every
        // branch when it moves. Put them on their branch first; if that is not
        // possible, leave the submodule where it is and say why.
        if let Ok(nested_repo) = git2::Repository::open(&nested) {
            let _ = attach_head(
                &nested_repo,
                &nested,
                branch.as_deref(),
                AttachMode::KeepCommit,
            );
        }
        if head_has_unsaved_commits(&nested) {
            results.push(FollowResult {
                path: m.path.clone(),
                outcome: FollowOutcome::Failed(
                    "it has commits of its own that are on no branch, so it was left where it is"
                        .into(),
                ),
            });
            continue;
        }

        let stashed = if nested_is_dirty(repo_path, &m.path) {
            // Untracked files are included: a checkout that has to write one fails the
            // same way a modified file does.
            let saved = run_git(
                Some(repo_path),
                &[
                    "-C",
                    &m.path,
                    "stash",
                    "push",
                    "--include-untracked",
                    "--message",
                    "gitwyrm: changes set aside to follow the pulled submodule version",
                ],
            );
            match saved {
                Ok(_) => true,
                Err(e) => {
                    // Nothing was set aside, so updating would overwrite real work. Leave
                    // the submodule alone and report why.
                    results.push(FollowResult {
                        path: m.path.clone(),
                        outcome: FollowOutcome::Failed(format!(
                            "could not set aside local changes: {e}"
                        )),
                    });
                    continue;
                }
            }
        } else {
            false
        };

        // `--init` covers a submodule added by the very commit just pulled;
        // `--recursive` follows pointer moves in nested submodules too.
        let updated = run_git(
            Some(repo_path),
            &[
                "submodule",
                "update",
                "--init",
                "--recursive",
                "--",
                &m.path,
            ],
        );

        // `git submodule update` always leaves the checkout detached. Put it back
        // on its branch when that branch can catch up to it.
        if updated.is_ok() {
            attach_submodule(repo_path, &m.path);
        }

        let outcome = match updated {
            Ok(_) if stashed => FollowOutcome::StashedAndUpdated,
            Ok(_) => FollowOutcome::Updated,
            Err(e) => FollowOutcome::Failed(e.to_string()),
        };
        results.push(FollowResult {
            path: m.path,
            outcome,
        });
    }

    Ok(results)
}

/// Follow the recorded pins, then say what happened in the shape the UI reads.
///
/// Pull, rebase and branch switch all need the same three things after moving a
/// pin: run the follow, record any failure in the log against the operation that
/// caused it, and hand back a list the frontend can report from. A failure here
/// is the case that most needs saying out loud -- the operation itself
/// succeeded, so nothing else on screen explains why a linked folder is now
/// sitting in the user's changes.
///
/// `operation` names the caller in the log line ("pull", "rebase", ...).
pub fn follow_and_report(
    repo: &git2::Repository,
    repo_path: &str,
    operation: &str,
    pins_before: &HashMap<String, git2::Oid>,
) -> Vec<SubmoduleFollowed> {
    follow_recorded_pins(repo, repo_path, pins_before)
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let (stashed, failed) = match r.outcome {
                FollowOutcome::Updated => (false, None),
                FollowOutcome::StashedAndUpdated => (true, None),
                FollowOutcome::Failed(why) => (false, Some(why)),
            };
            if let Some(why) = failed.as_deref() {
                log::warn!("{operation} could not update submodule {}: {}", r.path, why);
            }
            SubmoduleFollowed {
                path: r.path,
                stashed,
                failed,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A parent repo with one submodule, both on a real commit. Returns the temp
    /// dir (kept alive by the caller) and the parent's path as a string.
    ///
    /// Built by shelling out rather than through libgit2: `submodule add` wires up
    /// .gitmodules, .git/modules and the gitlink together, and reproducing that by
    /// hand is exactly the setup a test should not be asserting about.
    fn parent_with_submodule() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let sub_src = root.join("sub-src");
        std::fs::create_dir_all(&sub_src).unwrap();
        let sub_src_s = sub_src.to_string_lossy().into_owned();
        run_git(Some(&sub_src_s), &["init", "-q"]).unwrap();
        std::fs::write(sub_src.join("f.txt"), "v1").unwrap();
        run_git(Some(&sub_src_s), &["add", "."]).unwrap();
        commit(&sub_src_s, "v1");

        let parent = root.join("parent");
        std::fs::create_dir_all(&parent).unwrap();
        let parent_s = parent.to_string_lossy().into_owned();
        run_git(Some(&parent_s), &["init", "-q"]).unwrap();
        std::fs::write(parent.join("a.txt"), "one").unwrap();
        run_git(Some(&parent_s), &["add", "."]).unwrap();
        commit(&parent_s, "base");
        // file:// submodules are refused by default since the 2022 CVE fixes.
        run_git(
            Some(&parent_s),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                "--",
                &sub_src_s,
                "sub",
            ],
        )
        .unwrap();
        commit(&parent_s, "add sub");

        (dir, parent_s)
    }

    /// Commit with an identity supplied inline, so the test does not depend on
    /// whatever user.name the machine running it happens to have configured.
    fn commit(repo_path: &str, message: &str) {
        run_git(
            Some(repo_path),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "-m",
                message,
            ],
        )
        .unwrap();
    }

    fn head_of(repo_path: &str) -> String {
        run_git(Some(repo_path), &["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string()
    }

    /// Move the submodule's own checkout forward and record the new pin in the
    /// parent, mimicking what arrives in a pulled commit.
    fn bump_pin(parent: &str) {
        let sub = format!("{parent}/sub");
        std::fs::write(Path::new(&sub).join("f.txt"), "v2").unwrap();
        run_git(Some(&sub), &["add", "."]).unwrap();
        commit(&sub, "v2");
        run_git(Some(parent), &["add", "sub"]).unwrap();
        commit(parent, "bump sub");
    }

    /// The exact shape of the reported bug: the parent records a new commit for
    /// the submodule while the nested checkout still sits at the old one.
    fn strand_the_checkout(parent: &str) {
        let sub = format!("{parent}/sub");
        let old = run_git(Some(&sub), &["rev-parse", "HEAD~1"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        run_git(Some(&sub), &["checkout", "-q", &old]).unwrap();
    }

    #[test]
    fn a_submodule_left_behind_by_a_pull_is_moved_to_the_recorded_commit() {
        let (_dir, parent) = parent_with_submodule();
        bump_pin(&parent);
        let recorded = run_git(Some(&format!("{parent}/sub")), &["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        strand_the_checkout(&parent);

        let repo = git2::Repository::open(&parent).unwrap();
        // Precondition: this is the bug -- the parent sees a change nobody made.
        assert!(
            !moved_submodules(&repo).is_empty(),
            "expected a stranded submodule to fix"
        );

        let results = follow_recorded_pins(&repo, &parent, &HashMap::new()).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "sub");
        assert_eq!(results[0].outcome, FollowOutcome::Updated);
        assert_eq!(head_of(&format!("{parent}/sub")), recorded);
        // And the phantom pending change is gone.
        let repo = git2::Repository::open(&parent).unwrap();
        assert!(
            moved_submodules(&repo).is_empty(),
            "submodule should no longer read as modified"
        );
    }

    #[test]
    fn edits_inside_a_submodule_are_stashed_rather_than_overwritten() {
        let (_dir, parent) = parent_with_submodule();
        bump_pin(&parent);
        strand_the_checkout(&parent);

        let sub = format!("{parent}/sub");
        std::fs::write(Path::new(&sub).join("f.txt"), "MY WORK").unwrap();

        let repo = git2::Repository::open(&parent).unwrap();
        let results = follow_recorded_pins(&repo, &parent, &HashMap::new()).unwrap();

        assert_eq!(results[0].outcome, FollowOutcome::StashedAndUpdated);
        // The pulled version won the working tree...
        assert_eq!(
            std::fs::read_to_string(Path::new(&sub).join("f.txt")).unwrap(),
            "v2"
        );
        // ...but the user's work is recoverable, which is the whole point of
        // stashing instead of discarding.
        let stashes = run_git(Some(&sub), &["stash", "list"]).unwrap().stdout;
        assert!(
            stashes.contains("gitwyrm"),
            "expected a gitwyrm stash entry, got: {stashes:?}"
        );
    }

    /// Clone `origin` to a sibling `clone` directory, submodules and all.
    fn clone_of(origin: &str) -> String {
        let dest = format!(
            "{}/clone",
            Path::new(origin).parent().unwrap().to_string_lossy()
        );
        run_git(
            None,
            &[
                "-c",
                "protocol.file.allow=always",
                "clone",
                "-q",
                "--recurse-submodules",
                "--",
                origin,
                &dest,
            ],
        )
        .unwrap();
        dest
    }

    /// The real pull flow, which the other tests skip: the app keeps ONE
    /// long-lived `git2::Repository` per repo, opened long before the pull runs.
    /// The pull shells out to git.exe, which rewrites `.git/index` on disk behind
    /// libgit2's back -- so a handle opened earlier is reading a stale index and
    /// sees the old pin on both sides. The pointer move it is supposed to follow
    /// is invisible to it, and the user is left with the phantom pending change.
    #[test]
    fn a_pull_is_followed_through_a_handle_opened_before_it() {
        let (_dir, origin) = parent_with_submodule();
        let clone = clone_of(&origin);
        bump_pin(&origin);
        // Publish the submodule's new commit so the clone can actually fetch it,
        // the way a real remote would already have it. A side ref keeps the push
        // off sub-src's checked-out branch.
        run_git(
            Some(&format!("{origin}/sub")),
            &["push", "-q", "origin", "HEAD:refs/heads/pinned"],
        )
        .unwrap();

        // Opened before the pull, exactly as RepoManager holds it.
        let repo = git2::Repository::open(&clone).unwrap();
        // Warm the submodule/index cache the way the running app does -- status
        // runs constantly, so the handle has always read the pre-pull index.
        let _ = moved_submodules(&repo);
        let pins_before = recorded_pins(&repo);

        run_git(
            Some(&clone),
            &[
                "-c",
                "protocol.file.allow=always",
                "pull",
                "-q",
                "--autostash",
            ],
        )
        .unwrap();

        let results = follow_recorded_pins(&repo, &clone, &pins_before).unwrap();

        assert_eq!(
            results.len(),
            1,
            "the pull moved the pin, so it must be followed"
        );
        assert_eq!(results[0].outcome, FollowOutcome::Updated);
        // The point of it all: no phantom pending change is left behind.
        let fresh = git2::Repository::open(&clone).unwrap();
        assert!(
            moved_submodules(&fresh).is_empty(),
            "submodule should sit at the pulled commit, not read as modified"
        );
    }

    /// A pin naming a commit that exists nowhere is the shape of a failed follow:
    /// `git submodule update` has to fetch it and cannot. The operation that
    /// recorded it has already succeeded, so this comes back as a reported
    /// failure rather than an error -- and reporting it is the only thing that
    /// explains the pending change the user is left holding.
    #[test]
    fn a_pin_that_cannot_be_fetched_is_reported_as_failed() {
        let (_dir, parent) = parent_with_submodule();
        // A commit no repository has, so no fetch can produce it.
        run_git(
            Some(&parent),
            &[
                "update-index",
                "--cacheinfo",
                "160000,0000000000000000000000000000000000000001,sub",
            ],
        )
        .unwrap();

        let repo = git2::Repository::open(&parent).unwrap();
        let reported = follow_and_report(&repo, &parent, "pull", &HashMap::new());

        assert_eq!(reported.len(), 1, "the stranded submodule must be reported");
        assert_eq!(reported[0].path, "sub");
        assert!(
            reported[0].failed.is_some(),
            "an unfetchable pin must come back as failed"
        );
        // Nothing was set aside: the nested checkout was clean.
        assert!(!reported[0].stashed);
    }

    /// The quiet case: a follow that worked reports itself as neither failed nor
    /// stashed, so the UI has nothing to interrupt the user about.
    #[test]
    fn a_clean_follow_is_reported_without_a_failure() {
        let (_dir, parent) = parent_with_submodule();
        bump_pin(&parent);
        strand_the_checkout(&parent);

        let repo = git2::Repository::open(&parent).unwrap();
        let reported = follow_and_report(&repo, &parent, "pull", &HashMap::new());

        assert_eq!(reported.len(), 1);
        assert_eq!(reported[0].failed, None);
        assert!(!reported[0].stashed);
    }

    #[test]
    fn a_submodule_already_in_sync_is_left_alone() {
        let (_dir, parent) = parent_with_submodule();
        let repo = git2::Repository::open(&parent).unwrap();

        // Nothing moved, so there is nothing to report and no work to do.
        assert!(follow_recorded_pins(&repo, &parent, &HashMap::new())
            .unwrap()
            .is_empty());
    }

    fn branch_of(repo_path: &str) -> String {
        run_git(Some(repo_path), &["symbolic-ref", "-q", "--short", "HEAD"])
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default()
    }

    /// Commits made inside a submodule are the user's work, not a stale pin. A
    /// pull that never touched the pin used to snap the folder back to it anyway
    /// -- off its branch, with the new commit reachable only from the reflog.
    #[test]
    fn a_submodule_the_user_moved_ahead_is_not_snapped_back() {
        let (_dir, parent) = parent_with_submodule();
        let sub = format!("{parent}/sub");
        let branch = branch_of(&sub);
        std::fs::write(Path::new(&sub).join("f.txt"), "mine").unwrap();
        run_git(Some(&sub), &["add", "."]).unwrap();
        commit(&sub, "mine");
        let mine = head_of(&sub);

        let repo = git2::Repository::open(&parent).unwrap();
        let pins_before = recorded_pins(&repo);
        let results = follow_recorded_pins(&repo, &parent, &pins_before).unwrap();

        assert!(results.is_empty(), "nothing moved the pin: {results:?}");
        assert_eq!(head_of(&sub), mine);
        assert_eq!(branch_of(&sub), branch, "still on its branch");
    }

    /// When the pin really does move, a commit the user made on a detached
    /// checkout is put on its branch before the folder moves, so it survives.
    #[test]
    fn a_detached_commit_is_kept_on_its_branch_when_the_pin_moves() {
        let (_dir, parent) = parent_with_submodule();
        let sub = format!("{parent}/sub");
        let branch = branch_of(&sub);
        let repo = git2::Repository::open(&parent).unwrap();
        let pins_before = recorded_pins(&repo);
        let v1 = head_of(&sub);

        // Someone else's commit, which the parent is about to pin.
        run_git(Some(&sub), &["checkout", "-q", "-b", "theirs"]).unwrap();
        std::fs::write(Path::new(&sub).join("t.txt"), "theirs").unwrap();
        run_git(Some(&sub), &["add", "."]).unwrap();
        commit(&sub, "theirs");
        let theirs = head_of(&sub);

        // The user's commit, made on a bare checkout the way `submodule update` leaves it.
        run_git(Some(&sub), &["checkout", "-q", "--detach", &v1]).unwrap();
        std::fs::write(Path::new(&sub).join("m.txt"), "mine").unwrap();
        run_git(Some(&sub), &["add", "."]).unwrap();
        commit(&sub, "mine");
        let mine = head_of(&sub);

        run_git(
            Some(&parent),
            &[
                "update-index",
                "--cacheinfo",
                &format!("160000,{theirs},sub"),
            ],
        )
        .unwrap();

        let results = follow_recorded_pins(&repo, &parent, &pins_before).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, FollowOutcome::Updated);
        assert_eq!(head_of(&sub), theirs);
        let kept = run_git(Some(&sub), &["rev-parse", &branch])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        assert_eq!(
            kept, mine,
            "the user's commit must be on {branch}, not only in the reflog"
        );
    }

    /// After following, the folder lands on its branch rather than on a bare
    /// commit, so opening it shows a branch that Pull and Push work from.
    #[test]
    fn a_followed_submodule_ends_up_on_its_branch() {
        let (_dir, parent) = parent_with_submodule();
        bump_pin(&parent);
        let sub = format!("{parent}/sub");
        let branch = branch_of(&sub);
        strand_the_checkout(&parent);
        assert_eq!(branch_of(&sub), "", "precondition: detached");

        let repo = git2::Repository::open(&parent).unwrap();
        follow_recorded_pins(&repo, &parent, &recorded_pins(&repo)).unwrap();

        assert_eq!(branch_of(&sub), branch);
    }

    #[test]
    fn an_untracked_file_alone_counts_as_dirty() {
        let (_dir, parent) = parent_with_submodule();
        let sub = format!("{parent}/sub");

        assert!(
            !nested_is_dirty(&parent, "sub"),
            "a clean submodule must not read as dirty"
        );

        // Untracked files matter: a checkout that has to write one fails the same
        // way a modified tracked file does, so they have to be stashed too.
        std::fs::write(Path::new(&sub).join("scratch.txt"), "notes").unwrap();
        assert!(nested_is_dirty(&parent, "sub"));
    }
}
