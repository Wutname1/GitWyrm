//! Putting a detached checkout back on a branch.
//!
//! `git submodule update` leaves every submodule on a bare commit with no branch,
//! and that is where the user finds it when they open the submodule in its own
//! tab. From there nothing works the way they expect: Pull refuses ("not on a
//! branch"), a commit made there belongs to no branch and is easy to lose, and
//! the local `main` never moves, so it reads as further and further behind the
//! remote. Attaching fixes all three: the checkout lands on the branch whose
//! history it is already part of, and that branch catches up to it.
//!
//! Attaching never throws away a commit. A branch is only ever moved forward
//! (its old tip must be in the history of the commit it moves to), and a checkout
//! is only moved when the caller allows it AND the commit it leaves is already in
//! the branch it moves to.

use git2::{BranchType, Oid, Repository};

use crate::git::shell::run_git;

/// Whether attaching may move the working tree to a different commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachMode {
    /// Keep the checked-out commit exactly where it is. Used right after a
    /// submodule update, where that commit is the one the parent project pins and
    /// moving off it would make the submodule read as changed.
    KeepCommit,
    /// The branch may be checked out even when it is further along than the
    /// current commit, as long as that commit is part of its history. Used before
    /// a pull, which is about to move the checkout forward anyway.
    MayMoveForward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachOutcome {
    /// HEAD was already on a branch; nothing was done.
    AlreadyOnBranch,
    /// HEAD is now on `branch`. `moved` is true when the working tree moved to a
    /// newer commit to get there.
    Attached { branch: String, moved: bool },
    /// No branch could take the checkout without losing or moving something.
    /// `reason` is user-facing.
    NotAttached { reason: String },
}

/// Put a detached HEAD on a branch when it can be done without losing work.
///
/// `preferred` names the branch to try first -- a submodule's `.gitmodules`
/// branch. Otherwise the remote's default branch is used, then `main`/`master`.
pub fn attach_head(
    repo: &Repository,
    repo_path: &str,
    preferred: Option<&str>,
    mode: AttachMode,
) -> AttachOutcome {
    if !repo.head_detached().unwrap_or(false) {
        return AttachOutcome::AlreadyOnBranch;
    }
    let Some(head) = repo.head().ok().and_then(|h| h.target()) else {
        return not_attached("This folder has no commit checked out.");
    };

    let default = default_branch(repo, preferred);

    // A branch already sitting exactly here costs nothing to switch to. Prefer
    // the default one when several do.
    let mut at_head: Vec<String> = local_branches(repo)
        .into_iter()
        .filter(|(_, tip)| *tip == head)
        .map(|(name, _)| name)
        .collect();
    at_head.sort();
    if let Some(name) = default
        .as_deref()
        .filter(|d| at_head.iter().any(|b| b == d))
        .map(str::to_string)
        .or_else(|| at_head.first().cloned())
    {
        return switch(repo_path, &name, false);
    }

    let Some(branch) = default else {
        return not_attached(
            "This folder is not on a branch, and it has no main branch to put it on.",
        );
    };

    let local_tip = repo
        .find_branch(&branch, BranchType::Local)
        .ok()
        .and_then(|b| b.get().target());
    match local_tip {
        Some(tip) => {
            if descends(repo, head, tip) {
                // The branch is behind this commit: slide it forward to here. The
                // old tip is in this commit's history, so nothing on it is lost.
                let moved_ref = run_git(
                    Some(repo_path),
                    &[
                        "update-ref",
                        "-m",
                        "gitwyrm: catch up to the checked-out commit",
                        &format!("refs/heads/{branch}"),
                        &head.to_string(),
                        // Only if it still points where we read it, so a concurrent
                        // move is never overwritten.
                        &tip.to_string(),
                    ],
                );
                if let Err(e) = moved_ref {
                    log::warn!("could not move {branch} forward to attach HEAD: {e}");
                    return not_attached(&format!("Could not move {branch} up to this commit."));
                }
                switch(repo_path, &branch, false)
            } else if descends(repo, tip, head) {
                // The branch already contains this commit and more. Switching moves
                // the working tree, which is only wanted when a pull is about to.
                if mode == AttachMode::MayMoveForward {
                    switch(repo_path, &branch, true)
                } else {
                    not_attached(&format!(
                        "{branch} is further along than the version this folder is pinned to."
                    ))
                }
            } else {
                not_attached(&format!(
                    "This folder is not on a branch, and its current commit is not part of {branch}. Make a branch here to keep it."
                ))
            }
        }
        None => {
            // No local branch yet, but the remote has one: make the local branch
            // right here, linked to the remote one, as long as the two histories
            // are related. Unrelated history would make a branch that cannot pull.
            let Some((remote_ref, remote_tip)) = remote_branch(repo, &branch) else {
                return not_attached(&format!(
                    "This folder is not on a branch, and there is no {branch} to put it on."
                ));
            };
            if !(remote_tip == head
                || descends(repo, head, remote_tip)
                || descends(repo, remote_tip, head))
            {
                return not_attached(&format!(
                    "This folder is not on a branch, and its current commit is not part of {remote_ref}. Make a branch here to keep it."
                ));
            }
            if let Err(e) = run_git(
                Some(repo_path),
                &["branch", "--no-track", &branch, &head.to_string()],
            ) {
                log::warn!("could not create {branch} to attach HEAD: {e}");
                return not_attached(&format!("Could not make a {branch} branch here."));
            }
            let _ = run_git(
                Some(repo_path),
                &[
                    "branch",
                    &format!("--set-upstream-to={remote_ref}"),
                    &branch,
                ],
            );
            switch(repo_path, &branch, false)
        }
    }
}

/// True when HEAD's commit is on no branch, remote branch or tag -- the commits a
/// checkout move would leave reachable only from the reflog.
pub fn head_has_unsaved_commits(repo_path: &str) -> bool {
    run_git(
        Some(repo_path),
        &[
            "rev-list",
            "-n",
            "1",
            "HEAD",
            "--not",
            "--branches",
            "--remotes",
            "--tags",
        ],
    )
    .map(|out| !out.stdout.trim().is_empty())
    .unwrap_or(false)
}

fn not_attached(reason: &str) -> AttachOutcome {
    AttachOutcome::NotAttached {
        reason: reason.to_string(),
    }
}

fn switch(repo_path: &str, branch: &str, moved: bool) -> AttachOutcome {
    // `--no-guess` keeps git from inventing a tracking branch from a remote one
    // with the same name; every branch here was checked to exist locally.
    match run_git(Some(repo_path), &["switch", "--no-guess", branch]) {
        Ok(_) => AttachOutcome::Attached {
            branch: branch.to_string(),
            moved,
        },
        Err(e) => {
            log::warn!("could not switch to {branch} to attach HEAD: {e}");
            not_attached(&format!("Could not switch this folder to {branch}."))
        }
    }
}

fn descends(repo: &Repository, commit: Oid, ancestor: Oid) -> bool {
    commit == ancestor || repo.graph_descendant_of(commit, ancestor).unwrap_or(false)
}

fn local_branches(repo: &Repository) -> Vec<(String, Oid)> {
    let Ok(branches) = repo.branches(Some(BranchType::Local)) else {
        return Vec::new();
    };
    branches
        .flatten()
        .filter_map(|(b, _)| {
            let name = b.name().ok().flatten()?.to_string();
            let tip = b.get().target()?;
            Some((name, tip))
        })
        .collect()
}

/// The remote to read a default branch from: `origin`, or the only remote.
fn main_remote(repo: &Repository) -> Option<String> {
    let remotes = repo.remotes().ok()?;
    let names: Vec<String> = remotes
        .iter()
        .flatten()
        .flatten()
        .map(str::to_string)
        .collect();
    if names.iter().any(|n| n == "origin") {
        Some("origin".into())
    } else if names.len() == 1 {
        names.into_iter().next()
    } else {
        None
    }
}

fn remote_branch(repo: &Repository, branch: &str) -> Option<(String, Oid)> {
    let remote = main_remote(repo)?;
    let name = format!("{remote}/{branch}");
    let tip = repo
        .find_branch(&name, BranchType::Remote)
        .ok()?
        .get()
        .target()?;
    Some((name, tip))
}

/// The branch a detached checkout belongs on: the caller's preference, then
/// whatever the remote calls its default (`origin/HEAD`), then main or master.
fn default_branch(repo: &Repository, preferred: Option<&str>) -> Option<String> {
    let exists = |name: &str| {
        repo.find_branch(name, BranchType::Local).is_ok() || remote_branch(repo, name).is_some()
    };

    // `.gitmodules` may say "." to mean "the same name as the parent's branch",
    // which says nothing about the submodule's own branches.
    if let Some(p) = preferred
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != ".")
    {
        if exists(p) {
            return Some(p.to_string());
        }
    }

    if let Some(remote) = main_remote(repo) {
        let head_ref = format!("refs/remotes/{remote}/HEAD");
        if let Some(target) = repo
            .find_reference(&head_ref)
            .ok()
            .and_then(|r| r.symbolic_target().ok().flatten().map(str::to_string))
        {
            if let Some(name) = target.strip_prefix(&format!("refs/remotes/{remote}/")) {
                if exists(name) {
                    return Some(name.to_string());
                }
            }
        }
    }

    ["main", "master"]
        .into_iter()
        .find(|n| exists(n))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn git(repo: &str, args: &[&str]) -> String {
        let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
        full.extend_from_slice(args);
        run_git(Some(repo), &full)
            .unwrap()
            .stdout
            .trim()
            .to_string()
    }

    fn commit(repo: &str, file: &str, content: &str) -> String {
        std::fs::write(Path::new(repo).join(file), content).unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-q", "-m", content]);
        git(repo, &["rev-parse", "HEAD"])
    }

    /// An "origin" with three commits on main, and a clone of it whose local main
    /// is still at the first -- the shape of a submodule clone nobody has pulled.
    fn stale_clone() -> (tempfile::TempDir, String, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin").to_string_lossy().into_owned();
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "-q", "-b", "main"]);
        let c1 = commit(&origin, "f.txt", "one");
        let clone = dir.path().join("clone").to_string_lossy().into_owned();
        run_git(None, &["clone", "-q", "--", &origin, &clone]).unwrap();
        let c2 = commit(&origin, "f.txt", "two");
        let c3 = commit(&origin, "f.txt", "three");
        git(&clone, &["fetch", "-q"]);
        (dir, clone, vec![c1, c2, c3])
    }

    fn current_branch(repo: &str) -> Option<String> {
        run_git(Some(repo), &["symbolic-ref", "-q", "--short", "HEAD"])
            .ok()
            .map(|o| o.stdout.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    #[test]
    fn a_checkout_already_on_a_branch_is_left_alone() {
        let (_d, clone, _) = stale_clone();
        let repo = Repository::open(&clone).unwrap();
        assert_eq!(
            attach_head(&repo, &clone, None, AttachMode::KeepCommit),
            AttachOutcome::AlreadyOnBranch
        );
    }

    /// The reported case: the submodule sits on a newer commit than its stale
    /// local main, with a commit of the user's on top that is on no branch.
    #[test]
    fn a_stale_main_catches_up_and_keeps_a_commit_made_while_detached() {
        let (_d, clone, c) = stale_clone();
        git(&clone, &["checkout", "-q", &c[2]]);
        let mine = commit(&clone, "g.txt", "mine");
        assert!(head_has_unsaved_commits(&clone));

        let repo = Repository::open(&clone).unwrap();
        let out = attach_head(&repo, &clone, None, AttachMode::KeepCommit);

        assert_eq!(
            out,
            AttachOutcome::Attached {
                branch: "main".into(),
                moved: false
            }
        );
        assert_eq!(current_branch(&clone).as_deref(), Some("main"));
        assert_eq!(git(&clone, &["rev-parse", "main"]), mine);
        assert!(
            !head_has_unsaved_commits(&clone),
            "the commit is now on main"
        );
        // Still linked to the remote, so Pull and Push work from here.
        assert_eq!(
            git(&clone, &["rev-parse", "--abbrev-ref", "main@{upstream}"]),
            "origin/main"
        );
    }

    #[test]
    fn keep_commit_never_moves_the_checkout_onto_a_newer_branch() {
        let (_d, clone, c) = stale_clone();
        git(&clone, &["merge", "-q", "--ff-only", "origin/main"]);
        git(&clone, &["checkout", "-q", &c[1]]);

        let repo = Repository::open(&clone).unwrap();
        let out = attach_head(&repo, &clone, None, AttachMode::KeepCommit);

        assert!(matches!(out, AttachOutcome::NotAttached { .. }), "{out:?}");
        assert_eq!(git(&clone, &["rev-parse", "HEAD"]), c[1]);
        assert_eq!(
            git(&clone, &["rev-parse", "main"]),
            c[2],
            "main must not move back"
        );
    }

    #[test]
    fn may_move_forward_switches_to_a_branch_that_contains_the_commit() {
        let (_d, clone, c) = stale_clone();
        git(&clone, &["merge", "-q", "--ff-only", "origin/main"]);
        git(&clone, &["checkout", "-q", &c[1]]);

        let repo = Repository::open(&clone).unwrap();
        let out = attach_head(&repo, &clone, None, AttachMode::MayMoveForward);

        assert_eq!(
            out,
            AttachOutcome::Attached {
                branch: "main".into(),
                moved: true
            }
        );
        assert_eq!(git(&clone, &["rev-parse", "HEAD"]), c[2]);
    }

    #[test]
    fn a_commit_off_the_branch_history_is_not_attached() {
        let (_d, clone, c) = stale_clone();
        git(&clone, &["merge", "-q", "--ff-only", "origin/main"]);
        git(&clone, &["checkout", "-q", &c[1]]);
        let side = commit(&clone, "side.txt", "side");

        let repo = Repository::open(&clone).unwrap();
        let out = attach_head(&repo, &clone, None, AttachMode::MayMoveForward);

        assert!(matches!(out, AttachOutcome::NotAttached { .. }), "{out:?}");
        assert_eq!(git(&clone, &["rev-parse", "HEAD"]), side, "nothing moved");
        assert_eq!(git(&clone, &["rev-parse", "main"]), c[2]);
    }

    #[test]
    fn a_missing_local_branch_is_made_from_the_remote_one() {
        let (_d, clone, c) = stale_clone();
        git(&clone, &["checkout", "-q", &c[2]]);
        git(&clone, &["branch", "-q", "-D", "main"]);

        let repo = Repository::open(&clone).unwrap();
        let out = attach_head(&repo, &clone, None, AttachMode::KeepCommit);

        assert_eq!(
            out,
            AttachOutcome::Attached {
                branch: "main".into(),
                moved: false
            }
        );
        assert_eq!(
            git(&clone, &["rev-parse", "--abbrev-ref", "main@{upstream}"]),
            "origin/main"
        );
    }

    #[test]
    fn a_branch_sitting_on_the_commit_is_preferred() {
        let (_d, clone, c) = stale_clone();
        git(&clone, &["branch", "-q", "feature", &c[2]]);
        git(&clone, &["checkout", "-q", &c[2]]);

        let repo = Repository::open(&clone).unwrap();
        let out = attach_head(&repo, &clone, None, AttachMode::KeepCommit);

        assert_eq!(
            out,
            AttachOutcome::Attached {
                branch: "feature".into(),
                moved: false
            }
        );
        assert_eq!(git(&clone, &["rev-parse", "main"]), c[0], "main left alone");
    }
}
