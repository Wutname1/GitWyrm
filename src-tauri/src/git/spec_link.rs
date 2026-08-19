//! Which OpenSpec change a branch is working on.
//!
//! The link is stored per-repository in git config as
//! `branch.<name>.gitwyrm-spec`, so it survives restarts, stays with the clone
//! it belongs to, and never touches the user's global config. Git keeps
//! `branch.*` sections when a branch is renamed and drops them when it is
//! deleted, which is exactly the lifetime a link should have.
//!
//! When a branch has no explicit link, one is inferred from the `Spec:`
//! trailers its own commits carry. That means a branch someone else pushed --
//! or one made before this feature existed -- still reads as linked.

use crate::error::AppError;
use crate::git::shell::run_git;
use crate::git::trailers;

/// Config key holding the change id a branch is linked to.
fn config_key(branch: &str) -> String {
    format!("branch.{branch}.gitwyrm-spec")
}

/// How a branch came to be associated with a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkSource {
    /// The user linked it, and the id is stored in git config.
    Explicit,
    /// No stored link; read from `Spec:` trailers on the branch's commits.
    Inferred,
}

/// A branch's association with an OpenSpec change.
#[derive(Debug, Clone)]
pub struct SpecLink {
    pub change_id: String,
    pub source: LinkSource,
}

/// Read the explicitly stored link for `branch`, if the user set one.
pub fn explicit_link(repo_path: &str, branch: &str) -> Option<String> {
    // An unset key exits non-zero, which is the ordinary "not linked" case.
    let out = run_git(
        Some(repo_path),
        &["config", "--local", "--get", &config_key(branch)],
    )
    .ok()?;
    let value = out.stdout.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Store an explicit link from `branch` to `change_id`.
pub fn set_link(repo_path: &str, branch: &str, change_id: &str) -> Result<(), AppError> {
    run_git(
        Some(repo_path),
        &["config", "--local", &config_key(branch), change_id],
    )?;
    Ok(())
}

/// Drop the explicit link on `branch`.
///
/// Commits already carrying a `Spec:` trailer keep it -- history is not
/// rewritten -- so the graph still shows their chips. Only future commits stop
/// getting the trailer. Note this also means a branch whose commits carry
/// trailers will still read as *inferred*-linked after unlinking.
pub fn clear_link(repo_path: &str, branch: &str) -> Result<(), AppError> {
    // Absent is normal and exits non-zero.
    let _ = run_git(
        Some(repo_path),
        &["config", "--local", "--unset", &config_key(branch)],
    );
    Ok(())
}

/// Infer a change id from the `Spec:` trailers on commits unique to `branch`.
///
/// Only commits not reachable from the default branch are considered, so a
/// feature branch does not inherit a link from unrelated history it happens to
/// build on. The most recent trailer wins when a branch carries several.
fn infer_link(
    repo_path: &str,
    branch: &str,
    base: Option<&str>,
) -> Result<Option<String>, AppError> {
    let range = match base {
        Some(base) if base != branch => format!("{base}..{branch}"),
        _ => branch.to_string(),
    };
    // Read whole messages rather than shelling out per commit; %x1e separates
    // records so multi-line bodies survive intact.
    let out = run_git(
        Some(repo_path),
        &["log", "--max-count=200", "--format=%B%x1e", &range],
    )?;

    Ok(out.stdout.split('\u{1e}').find_map(trailers::spec_id))
}

/// The change `branch` is working on, explicit link first, then inference.
///
/// A branch git cannot walk -- unborn, or deleted out from under us -- has no
/// link rather than being an error, since "no commits yet" is an ordinary
/// state. Anything else git reports propagates, so a broken repository does not
/// quietly masquerade as an unlinked one.
pub fn resolve(
    repo_path: &str,
    branch: &str,
    base: Option<&str>,
) -> Result<Option<SpecLink>, AppError> {
    if let Some(change_id) = explicit_link(repo_path, branch) {
        return Ok(Some(SpecLink {
            change_id,
            source: LinkSource::Explicit,
        }));
    }
    let inferred = match infer_link(repo_path, branch, base) {
        Ok(found) => found,
        // `unknown revision` is what git says for a branch with no commits.
        Err(AppError::Other(message)) if message.contains("unknown revision") => None,
        Err(other) => return Err(other),
    };
    Ok(inferred.map(|change_id| SpecLink {
        change_id,
        source: LinkSource::Inferred,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use std::fs;

    fn init_repo() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = Repository::init(dir.path()).expect("init");
        (dir, repo)
    }

    fn commit(repo: &Repository, name: &str, message: &str) {
        let workdir = repo.workdir().expect("workdir");
        fs::write(workdir.join(name), name).expect("write");
        let mut index = repo.index().expect("index");
        index.add_path(std::path::Path::new(name)).expect("add");
        index.write().expect("write index");
        let tree = repo
            .find_tree(index.write_tree().expect("tree"))
            .expect("find tree");
        let sig = Signature::now("Test", "test@example.com").expect("sig");
        let parents = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
            .expect("commit");
    }

    #[test]
    fn explicit_link_round_trips() {
        let (dir, _repo) = init_repo();
        let path = dir.path().to_str().expect("path");

        assert_eq!(explicit_link(path, "main"), None);

        set_link(path, "main", "add-thing").expect("set");
        assert_eq!(explicit_link(path, "main").as_deref(), Some("add-thing"));

        clear_link(path, "main").expect("clear");
        assert_eq!(explicit_link(path, "main"), None);
    }

    #[test]
    fn clearing_an_absent_link_succeeds() {
        let (dir, _repo) = init_repo();
        let path = dir.path().to_str().expect("path");
        clear_link(path, "never-linked").expect("clear is a no-op");
    }

    /// The branch `git init` created, which differs by git version and by the
    /// user's `init.defaultBranch`.
    fn current_branch(repo: &Repository) -> String {
        repo.head()
            .expect("head")
            .shorthand()
            .expect("shorthand")
            .to_string()
    }

    #[test]
    fn explicit_link_wins_over_trailers() {
        let (dir, repo) = init_repo();
        let path = dir.path().to_str().expect("path");
        commit(&repo, "a.txt", "new: a\n\nSpec: from-trailer");
        let branch = current_branch(&repo);
        set_link(path, &branch, "from-config").expect("set");

        let link = resolve(path, &branch, None)
            .expect("resolve")
            .expect("linked");
        assert_eq!(link.change_id, "from-config");
        assert_eq!(link.source, LinkSource::Explicit);
    }

    #[test]
    fn infers_a_link_from_commit_trailers() {
        let (dir, repo) = init_repo();
        let path = dir.path().to_str().expect("path");
        commit(&repo, "a.txt", "new: a\n\nSpec: add-thing");

        let link = resolve(path, &current_branch(&repo), None)
            .expect("resolve")
            .expect("linked");
        assert_eq!(link.change_id, "add-thing");
        assert_eq!(link.source, LinkSource::Inferred);
    }

    #[test]
    fn newest_trailer_wins_when_a_branch_has_several() {
        let (dir, repo) = init_repo();
        let path = dir.path().to_str().expect("path");
        commit(&repo, "a.txt", "new: a\n\nSpec: older");
        commit(&repo, "b.txt", "new: b\n\nSpec: newer");

        let link = resolve(path, &current_branch(&repo), None)
            .expect("resolve")
            .expect("linked");
        assert_eq!(link.change_id, "newer");
    }

    #[test]
    fn untrailered_history_resolves_to_nothing() {
        let (dir, repo) = init_repo();
        let path = dir.path().to_str().expect("path");
        commit(&repo, "a.txt", "new: a plain commit");

        assert!(resolve(path, &current_branch(&repo), None)
            .expect("resolve")
            .is_none());
    }

    #[test]
    fn a_branch_does_not_inherit_a_link_from_its_base() {
        let (dir, repo) = init_repo();
        let path = dir.path().to_str().expect("path");
        commit(&repo, "a.txt", "new: base work\n\nSpec: base-change");
        let base = current_branch(&repo);

        // Branch off and add a commit of its own with no trailer.
        let tip = repo.head().expect("head").peel_to_commit().expect("commit");
        repo.branch("feature", &tip, false).expect("branch");
        repo.set_head("refs/heads/feature").expect("set head");
        commit(&repo, "b.txt", "new: feature work");

        // Walking the whole branch would find the base's trailer; excluding the
        // base correctly reports no link.
        assert!(resolve(path, "feature", Some(&base))
            .expect("resolve")
            .is_none());
    }

    /// The end-to-end shape task 4.1/4.3 describe: a commit written the way the
    /// commit form writes one is read back as linked, and a plain commit beside
    /// it is not. The unit tests above exercise the parser on string literals;
    /// this one goes through a real commit object, which is what the graph
    /// actually reads.
    #[test]
    fn a_committed_trailer_resolves_but_a_plain_commit_does_not() {
        let (dir, repo) = init_repo();
        let path = dir.path().to_str().expect("path");

        // Same shape `compose_message` produces for a linked branch.
        commit(
            &repo,
            "a.txt",
            "new: linked work\n\nWhy it changed.\n\nSpec: add-spec-commit-links",
        );
        let branch = current_branch(&repo);
        let link = resolve(path, &branch, None)
            .expect("resolve")
            .expect("linked");
        assert_eq!(link.change_id, "add-spec-commit-links");
        assert_eq!(link.source, LinkSource::Inferred);

        // An unlinked branch built on that history must not inherit the link.
        let tip = repo.head().expect("head").peel_to_commit().expect("commit");
        repo.branch("unlinked", &tip, false).expect("branch");
        repo.set_head("refs/heads/unlinked").expect("set head");
        commit(&repo, "b.txt", "new: unlinked work\n\nWhy it changed.");
        assert!(
            resolve(path, "unlinked", Some(&branch))
                .expect("resolve")
                .is_none(),
            "a branch with no trailer of its own must not read as linked"
        );
    }

    #[test]
    fn an_unborn_branch_has_no_link_rather_than_erroring() {
        let (dir, _repo) = init_repo();
        let path = dir.path().to_str().expect("path");
        // No commits exist yet, so the branch cannot be walked.
        assert!(resolve(path, "main", None).expect("resolve").is_none());
    }
}
