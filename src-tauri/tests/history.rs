//! Multi-commit history rewrites (squash / multi-drop) against throwaway
//! repos, exercising the real `git_history` implementation the commands call.

use std::fs;
use std::path::PathBuf;

use git2::{Repository, Signature};
use gitwyrm_lib::git_history;

fn scratch_repo(label: &str) -> (PathBuf, Repository) {
    let dir = std::env::temp_dir().join(format!(
        "gitwyrm-test-history-{}-{label}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let repo = Repository::init(&dir).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test Wyrm").unwrap();
        config.set_str("user.email", "test@gitwyrm.dev").unwrap();
    }
    (dir, repo)
}

fn sig() -> Signature<'static> {
    Signature::now("Test Wyrm", "test@gitwyrm.dev").unwrap()
}

/// Write `content` to `file` and commit everything staged-or-changed as `msg`.
fn commit_file(dir: &PathBuf, repo: &Repository, file: &str, content: &str, msg: &str) -> String {
    fs::write(dir.join(file), content).unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parents = match repo.head() {
        Ok(h) => vec![h.peel_to_commit().unwrap()],
        Err(_) => vec![],
    };
    let parent_refs: Vec<_> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig(), &sig(), msg, &tree, &parent_refs)
        .unwrap()
        .to_string()
}

/// Commit summaries from HEAD down, newest first.
fn log_summaries(repo: &Repository) -> Vec<String> {
    let mut walk = repo.revwalk().unwrap();
    walk.push(repo.head().unwrap().peel_to_commit().unwrap().id())
        .unwrap();
    walk.map(|oid| {
        repo.find_commit(oid.unwrap())
            .unwrap()
            .summary()
            .unwrap()
            .unwrap()
            .to_string()
    })
    .collect()
}

fn head_sha(repo: &Repository) -> String {
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string()
}

#[test]
fn squash_contiguous_commits() {
    let (dir, repo) = scratch_repo("squash");
    commit_file(&dir, &repo, "base.txt", "base\n", "initial");
    commit_file(&dir, &repo, "a.txt", "a\n", "add a");
    let b = commit_file(&dir, &repo, "b.txt", "b\n", "add b");
    let c = commit_file(&dir, &repo, "c.txt", "c\n", "add c");
    commit_file(&dir, &repo, "d.txt", "d\n", "add d");
    let before = head_sha(&repo);

    // Selection arrives newest-first, like the graph sends it.
    let mv = git_history::squash_commits(&repo, &[c, b], "add b and c together").unwrap();
    assert_eq!(mv.previous_sha, before);

    assert_eq!(
        log_summaries(&repo),
        vec!["add d", "add b and c together", "add a", "initial"]
    );
    // The combined commit's changes and the replayed commit's are all present.
    for f in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        assert!(dir.join(f).exists(), "{f} should exist after squash");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn squash_non_contiguous_moves_selection_together() {
    let (dir, repo) = scratch_repo("squash-gap");
    commit_file(&dir, &repo, "base.txt", "base\n", "initial");
    let a = commit_file(&dir, &repo, "a.txt", "a\n", "add a");
    commit_file(&dir, &repo, "b.txt", "b\n", "add b");
    let c = commit_file(&dir, &repo, "c.txt", "c\n", "add c");

    let mv = git_history::squash_commits(&repo, &[c, a], "add a and c").unwrap();
    assert!(!mv.previous_sha.is_empty());

    // The in-between commit replays on top of the combined one.
    assert_eq!(
        log_summaries(&repo),
        vec!["add b", "add a and c", "initial"]
    );
    for f in ["a.txt", "b.txt", "c.txt"] {
        assert!(dir.join(f).exists(), "{f} should exist after squash");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn drop_several_commits_at_once() {
    let (dir, repo) = scratch_repo("drop");
    commit_file(&dir, &repo, "base.txt", "base\n", "initial");
    commit_file(&dir, &repo, "a.txt", "a\n", "add a");
    let b = commit_file(&dir, &repo, "b.txt", "b\n", "add b");
    let c = commit_file(&dir, &repo, "c.txt", "c\n", "add c");
    commit_file(&dir, &repo, "d.txt", "d\n", "add d");

    git_history::drop_commits(&repo, &[c, b]).unwrap();

    assert_eq!(log_summaries(&repo), vec!["add d", "add a", "initial"]);
    assert!(!dir.join("b.txt").exists());
    assert!(!dir.join("c.txt").exists());
    assert!(dir.join("a.txt").exists());
    assert!(dir.join("d.txt").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn conflicting_rewrite_aborts_and_restores() {
    let (dir, repo) = scratch_repo("conflict");
    commit_file(&dir, &repo, "f.txt", "one\n", "initial");
    let b = commit_file(&dir, &repo, "f.txt", "two\n", "set two");
    commit_file(&dir, &repo, "f.txt", "three\n", "set three");
    let before = head_sha(&repo);

    // Dropping "set two" forces "set three" to replay onto "one" -> conflict.
    let err = git_history::drop_commits(&repo, &[b]).unwrap_err();
    assert!(
        err.to_string().contains("nothing was changed"),
        "got: {err}"
    );

    // Branch and file are exactly where they started.
    assert_eq!(head_sha(&repo), before);
    assert_eq!(
        fs::read_to_string(dir.join("f.txt"))
            .unwrap()
            .replace("\r\n", "\n"),
        "three\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dirty_tree_refuses_rewrite() {
    let (dir, repo) = scratch_repo("dirty");
    commit_file(&dir, &repo, "f.txt", "one\n", "initial");
    let b = commit_file(&dir, &repo, "g.txt", "g\n", "add g");
    let c = commit_file(&dir, &repo, "h.txt", "h\n", "add h");
    fs::write(dir.join("f.txt"), "edited\n").unwrap();

    let err = git_history::squash_commits(&repo, &[c, b], "combined").unwrap_err();
    assert!(
        err.to_string().contains("working tree has changes"),
        "got: {err}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The span helper backs the single-commit reword and drop commands. It returns
/// the commits ABOVE the target, oldest-first, with the target excluded.
#[test]
fn span_above_excludes_the_target_and_orders_oldest_first() {
    let (dir, repo) = scratch_repo("span-above");
    commit_file(&dir, &repo, "f.txt", "one\n", "initial");
    let b = commit_file(&dir, &repo, "g.txt", "g\n", "add g");
    commit_file(&dir, &repo, "h.txt", "h\n", "add h");
    commit_file(&dir, &repo, "i.txt", "i\n", "add i");

    let target = git2::Oid::from_str(&b).unwrap();
    let span =
        git_history::collect_span_above(&repo, target, "merge above", "not on branch").unwrap();

    let msgs: Vec<String> = span
        .iter()
        .map(|c| c.summary().ok().flatten().unwrap_or("").to_string())
        .collect();
    assert_eq!(msgs, vec!["add h".to_string(), "add i".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

/// A sha that isn't on the current branch gets the caller's message, not a
/// silent empty span that would make a rewrite drop history.
#[test]
fn span_above_rejects_a_commit_off_the_branch() {
    let (dir, repo) = scratch_repo("span-off-branch");
    commit_file(&dir, &repo, "f.txt", "one\n", "initial");
    commit_file(&dir, &repo, "g.txt", "g\n", "add g");

    // A well-formed sha that exists in no repo.
    let absent = git2::Oid::from_str("0123456789012345678901234567890123456789").unwrap();
    let err =
        git_history::collect_span_above(&repo, absent, "merge above", "not on branch").unwrap_err();
    assert!(err.to_string().contains("not on branch"), "got: {err}");
    let _ = fs::remove_dir_all(&dir);
}
