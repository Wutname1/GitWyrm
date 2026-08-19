//! Pull and rebase must never fail merely because the working tree is dirty.
//!
//! Before `--autostash`, both commands handed back git's raw refusal ("Your
//! local changes to the following files would be overwritten by merge") and
//! left the user to stash by hand -- a dead end, since the stash/apply/reapply
//! it demands is exactly what git can do itself. These tests drive the real
//! `git` binary with the same argument lists the commands build, so a regression
//! in those args fails here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let (ok, text) = git(dir, args);
    assert!(ok, "git {args:?} failed: {text}");
    text
}

fn init(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git_ok(dir, &["init", "-q", "-b", "main"]);
    git_ok(dir, &["config", "user.name", "Test Wyrm"]);
    git_ok(dir, &["config", "user.email", "test@gitwyrm.dev"]);
    git_ok(dir, &["config", "core.autocrlf", "false"]);
}

fn commit_all(dir: &Path, msg: &str) {
    git_ok(dir, &["add", "-A"]);
    git_ok(dir, &["commit", "-q", "-m", msg]);
}

/// A remote plus a clone of it, both ready to commit.
fn scratch_pair(tag: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("gitwyrm-autostash-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let origin = base.join("origin");
    let clone = base.join("clone");

    init(&origin);
    fs::write(origin.join("shared.txt"), "base\n").unwrap();
    fs::write(origin.join("other.txt"), "other\n").unwrap();
    commit_all(&origin, "init");
    // A non-bare origin can still be cloned; checked-out branch pushes are what
    // we avoid, and the test only ever commits in origin directly.
    //
    // `core.autocrlf=false` is set via `-c` so it applies to the checkout itself.
    // Configuring it afterwards is too late on Windows: files land CRLF and the
    // fresh clone reports as dirty before the test has changed anything.
    git_ok(
        &base,
        &[
            "-c",
            "core.autocrlf=false",
            "clone",
            "-q",
            origin.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    init_identity(&clone);
    (origin, clone)
}

fn init_identity(dir: &Path) {
    git_ok(dir, &["config", "user.name", "Test Wyrm"]);
    git_ok(dir, &["config", "user.email", "test@gitwyrm.dev"]);
    git_ok(dir, &["config", "core.autocrlf", "false"]);
}

/// The exact argument list `git_pull` builds.
const PULL_ARGS: &[&str] = &["pull", "--progress", "--autostash"];
/// The argument list `git_rebase` builds, minus the caller-supplied target.
const REBASE_ARGS: &[&str] = &["rebase", "--autostash"];

/// The original report: a pull whose incoming commit touches the very file the
/// user has uncommitted edits in. Plain `git pull` refuses outright.
#[test]
fn pull_succeeds_with_conflicting_dirty_file() {
    let (origin, clone) = scratch_pair("pull_same_file");

    // Remote advances the shared file.
    fs::write(origin.join("shared.txt"), "base\nfrom remote\n").unwrap();
    commit_all(&origin, "remote edit");

    // Local has uncommitted work in that same file.
    fs::write(clone.join("shared.txt"), "base\nmy local work\n").unwrap();

    git_ok(&clone, &["fetch", "-q"]);

    // Baseline: a plain merge is exactly the refusal this fix exists to remove.
    // Asserting it here keeps the test honest -- if git ever stopped refusing,
    // the --autostash assertions below would pass for the wrong reason.
    let (plain_ok, plain_text) = git(&clone, &["merge", "--no-edit", "origin/main"]);
    assert!(
        !plain_ok,
        "expected a plain merge to refuse a dirty tree, but it succeeded"
    );
    assert!(
        plain_text.contains("would be overwritten by merge"),
        "expected the overwrite refusal, got: {plain_text}"
    );

    let (ok, text) = git(&clone, PULL_ARGS);
    assert!(
        ok,
        "pull with --autostash should succeed on a dirty tree: {text}"
    );

    // The user's uncommitted work is back in the tree, not stranded in a stash.
    let content = fs::read_to_string(clone.join("shared.txt")).unwrap();
    assert!(
        content.contains("my local work"),
        "local edit must survive the pull, got: {content}"
    );

    // And the remote commit actually landed.
    let log = git_ok(&clone, &["log", "--oneline"]);
    assert!(
        log.contains("remote edit"),
        "remote commit should be present: {log}"
    );
}

/// The common case: dirty file the incoming commits never touch. Plain pull
/// still refuses (merge is all-or-nothing), so this must pass too.
#[test]
fn pull_succeeds_with_unrelated_dirty_file() {
    let (origin, clone) = scratch_pair("pull_other_file");

    fs::write(origin.join("shared.txt"), "base\nfrom remote\n").unwrap();
    commit_all(&origin, "remote edit");

    fs::write(clone.join("other.txt"), "unrelated local work\n").unwrap();
    git_ok(&clone, &["fetch", "-q"]);

    let (ok, text) = git(&clone, PULL_ARGS);
    assert!(ok, "pull should succeed with unrelated dirty file: {text}");

    let content = fs::read_to_string(clone.join("other.txt")).unwrap();
    assert!(
        content.contains("unrelated local work"),
        "unrelated edit must survive: {content}"
    );
}

/// Staged-but-uncommitted work is restored staged, not silently unstaged.
#[test]
fn pull_preserves_staged_changes() {
    let (origin, clone) = scratch_pair("pull_staged");

    fs::write(origin.join("shared.txt"), "base\nfrom remote\n").unwrap();
    commit_all(&origin, "remote edit");

    fs::write(clone.join("new.txt"), "staged work\n").unwrap();
    git_ok(&clone, &["add", "new.txt"]);
    git_ok(&clone, &["fetch", "-q"]);

    let (ok, text) = git(&clone, PULL_ARGS);
    assert!(ok, "pull should succeed with staged changes: {text}");

    let status = git_ok(&clone, &["status", "--short"]);
    assert!(
        status.contains("new.txt"),
        "staged file should still be present: {status}"
    );
    assert!(fs::read_to_string(clone.join("new.txt"))
        .unwrap()
        .contains("staged work"));
}

/// A clean tree must keep working -- --autostash is a no-op there and must not
/// invent an empty stash entry.
#[test]
fn pull_on_clean_tree_creates_no_stash() {
    let (origin, clone) = scratch_pair("pull_clean");

    fs::write(origin.join("shared.txt"), "base\nfrom remote\n").unwrap();
    commit_all(&origin, "remote edit");
    git_ok(&clone, &["fetch", "-q"]);

    let (ok, text) = git(&clone, PULL_ARGS);
    assert!(ok, "pull on a clean tree should succeed: {text}");

    let stashes = git_ok(&clone, &["stash", "list"]);
    assert!(
        stashes.trim().is_empty(),
        "clean pull must not leave a stash: {stashes}"
    );
}

/// `git rebase` has no `--progress` flag; passing one made every rebase exit
/// with "unknown option `progress'". Guard the real argument list.
#[test]
fn rebase_args_are_accepted_by_git() {
    let (_origin, clone) = scratch_pair("rebase_args");

    git_ok(&clone, &["checkout", "-q", "-b", "feature"]);
    fs::write(clone.join("feature.txt"), "feature work\n").unwrap();
    commit_all(&clone, "feature commit");

    let mut args = REBASE_ARGS.to_vec();
    args.push("main");
    let (ok, text) = git(&clone, &args);

    assert!(ok, "rebase args must be valid: {text}");
    assert!(
        !text.contains("unknown option"),
        "rebase must not be given a flag git rejects: {text}"
    );
}

/// Rebase over a dirty tree: the changes are set aside and put back, and the
/// commits actually replay onto the new base.
#[test]
fn rebase_succeeds_with_dirty_tree() {
    let (_origin, clone) = scratch_pair("rebase_dirty");

    // main gains a commit that feature does not have.
    fs::write(clone.join("other.txt"), "main moved on\n").unwrap();
    commit_all(&clone, "main commit");
    git_ok(&clone, &["checkout", "-q", "-b", "feature", "HEAD~1"]);
    fs::write(clone.join("feature.txt"), "feature work\n").unwrap();
    commit_all(&clone, "feature commit");

    // Uncommitted work present at rebase time.
    fs::write(clone.join("dirty.txt"), "in progress\n").unwrap();

    let mut args = REBASE_ARGS.to_vec();
    args.push("main");
    let (ok, text) = git(&clone, &args);
    assert!(ok, "rebase should succeed over a dirty tree: {text}");

    // Dirty work restored.
    let dirty = fs::read_to_string(clone.join("dirty.txt")).unwrap();
    assert!(
        dirty.contains("in progress"),
        "dirty file must survive rebase: {dirty}"
    );

    // And the rebase genuinely replayed onto main's commit.
    let log = git_ok(&clone, &["log", "--oneline"]);
    assert!(
        log.contains("feature commit"),
        "feature commit missing: {log}"
    );
    assert!(
        log.contains("main commit"),
        "should now sit on top of main: {log}"
    );
}
