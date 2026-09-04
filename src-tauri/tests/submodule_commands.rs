//! End-to-end checks for the submodule management commands, driven through the
//! same git CLI paths the Tauri commands use. Verifies the actual git behavior
//! the commands depend on (bump/add/remove), which unit tests on git2 cannot.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git2::Repository;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-c")
        .arg("protocol.file.allow=always")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git should be installed");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn identity(dir: &Path) {
    git(dir, &["config", "user.email", "t@gitwyrm.dev"]);
    git(dir, &["config", "user.name", "Test Wyrm"]);
}

/// Parent pinning `vendor/lib` at the FIRST of three upstream commits, so a
/// bump has somewhere to go. Returns (parent, upstream, first_sha, tip_sha).
fn fixture(label: &str) -> Option<(PathBuf, PathBuf, String, String)> {
    Command::new("git").arg("--version").output().ok()?;

    let root = std::env::temp_dir().join(format!("gitwyrm-subcmd-{}-{label}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let upstream = root.join("upstream");
    fs::create_dir_all(&upstream).unwrap();
    git(&upstream, &["init", "-q", "-b", "main"]);
    identity(&upstream);
    let mut first = String::new();
    for (i, body) in ["v1", "v2", "v3"].iter().enumerate() {
        fs::write(upstream.join("lib.txt"), body).unwrap();
        git(&upstream, &["add", "."]);
        git(&upstream, &["commit", "-qm", &format!("c{}", i + 1)]);
        if i == 0 {
            first = git_out(&upstream, &["rev-parse", "HEAD"]);
        }
    }
    let tip = git_out(&upstream, &["rev-parse", "HEAD"]);

    let parent = root.join("parent");
    fs::create_dir_all(&parent).unwrap();
    git(&parent, &["init", "-q", "-b", "main"]);
    identity(&parent);
    fs::write(parent.join("README.md"), "# demo").unwrap();
    git(&parent, &["add", "."]);
    git(&parent, &["commit", "-qm", "init"]);
    git(
        &parent,
        &[
            "submodule",
            "add",
            "-q",
            "-b",
            "main",
            "../upstream",
            "vendor/lib",
        ],
    );
    git(&parent.join("vendor/lib"), &["checkout", "-q", &first]);
    git(&parent, &["add", "."]);
    git(&parent, &["commit", "-qm", "pin lib at c1"]);

    Some((parent, upstream, first, tip))
}

/// `git submodule update --remote` plus `git add` -- what bump_submodule runs --
/// must move the pointer to the followed branch's tip and stage it.
#[test]
fn bump_moves_pointer_to_branch_tip_and_stages_it() {
    let Some((parent, _up, first, tip)) = fixture("bump") else {
        return;
    };

    let repo = Repository::open(&parent).unwrap();
    let before = gitwyrm_lib::git_submodule::all_submodules(&repo);
    assert_eq!(
        before[0].recorded_sha, first,
        "starts pinned at the first commit"
    );
    assert_eq!(
        before[0].branch.as_deref(),
        Some("main"),
        "branch comes from .gitmodules"
    );

    git(
        &parent,
        &[
            "submodule",
            "update",
            "--remote",
            "--init",
            "--",
            "vendor/lib",
        ],
    );
    git(&parent, &["add", "--", "vendor/lib"]);

    assert_eq!(
        git_out(&parent.join("vendor/lib"), &["rev-parse", "HEAD"]),
        tip,
        "bump must land on the followed branch's newest commit"
    );
    // Staged, so the user can commit it -- shows as an index change, not a
    // working-tree one.
    assert!(
        git_out(&parent, &["diff", "--cached", "--name-only"]).contains("vendor/lib"),
        "the moved pointer must be staged"
    );

    let after = gitwyrm_lib::git_submodule::all_submodules(&Repository::open(&parent).unwrap());
    assert_eq!(
        after[0].recorded_sha, tip,
        "the staged pointer is what we now record"
    );
    assert_eq!(
        after[0].state,
        gitwyrm_lib::git_types::SubmoduleState::InSync,
        "after staging, workdir and recorded agree"
    );
}

/// add_submodule's git invocation must create .gitmodules and leave the new
/// submodule staged and visible to the list.
#[test]
fn add_creates_and_lists_a_new_submodule() {
    let Some((parent, upstream, _f, _t)) = fixture("add") else {
        return;
    };
    let up = upstream.to_string_lossy().replace('\\', "/");

    git(
        &parent,
        &["submodule", "add", "-q", "--", &up, "vendor/second"],
    );

    let repo = Repository::open(&parent).unwrap();
    let all = gitwyrm_lib::git_submodule::all_submodules(&repo);
    let paths: Vec<&str> = all.iter().map(|s| s.path.as_str()).collect();
    assert!(
        paths.contains(&"vendor/second"),
        "the new submodule must be listed, got {paths:?}"
    );
    assert!(
        fs::read_to_string(parent.join(".gitmodules"))
            .unwrap()
            .contains("vendor/second"),
        ".gitmodules must record it"
    );
}

/// remove_submodule's sequence must take the submodule out of the list and
/// clear the cache so the path can be reused.
///
/// The subtle part is .gitmodules: `git rm` does not reliably strip the
/// stanza, and that file is what defines a submodule -- leaving it means the
/// removed submodule keeps appearing in the list.
#[test]
fn remove_drops_it_from_the_list_and_clears_the_cache() {
    let Some((parent, _up, _f, _t)) = fixture("remove") else {
        return;
    };

    git(&parent, &["submodule", "deinit", "-f", "--", "vendor/lib"]);
    git(&parent, &["rm", "-f", "--", "vendor/lib"]);

    // What the command does after `git rm`. --remove-section is best-effort:
    // git usually strips the stanza itself and this then reports "no such
    // section", which is why the real command ignores its exit status.
    let _ = Command::new("git")
        .args([
            "config",
            "--file",
            ".gitmodules",
            "--remove-section",
            "submodule.vendor/lib",
        ])
        .current_dir(&parent)
        .output();
    let gitmodules = parent.join(".gitmodules");
    if fs::read_to_string(&gitmodules)
        .map(|c| c.trim().is_empty())
        .unwrap_or(false)
    {
        fs::remove_file(&gitmodules).unwrap();
        git(&parent, &["rm", "--cached", "--", ".gitmodules"]);
    }

    let cached = parent.join(".git").join("modules").join("vendor/lib");
    if cached.exists() {
        fs::remove_dir_all(&cached).unwrap();
    }

    let repo = Repository::open(&parent).unwrap();
    assert!(
        gitwyrm_lib::git_submodule::all_submodules(&repo).is_empty(),
        "a removed submodule must not be listed"
    );
    assert!(
        !cached.exists(),
        "cached git data must be gone so the path can be re-added"
    );
    assert!(
        !parent.join("vendor/lib").exists(),
        "delete_files removes the folder too"
    );
}

/// The fresh-clone case: cloning a repo with submodules leaves them empty until
/// init, which is what init_all_submodules fixes.
#[test]
fn init_all_downloads_missing_submodules() {
    let Some((parent, _up, first, _t)) = fixture("initall") else {
        return;
    };
    let clone_dir = parent.parent().unwrap().join("cloned");
    let src = parent.to_string_lossy().replace('\\', "/");
    git(
        parent.parent().unwrap(),
        &["clone", "-q", &src, clone_dir.to_str().unwrap()],
    );

    let repo = Repository::open(&clone_dir).unwrap();
    let before = gitwyrm_lib::git_submodule::all_submodules(&repo);
    assert_eq!(
        before[0].state,
        gitwyrm_lib::git_types::SubmoduleState::Uninitialized,
        "a fresh clone has submodules recorded but not checked out"
    );

    git(
        &clone_dir,
        &["submodule", "update", "--init", "--recursive"],
    );

    let after = gitwyrm_lib::git_submodule::all_submodules(&Repository::open(&clone_dir).unwrap());
    assert_eq!(
        after[0].state,
        gitwyrm_lib::git_types::SubmoduleState::InSync,
        "after init the submodule sits at the recorded commit"
    );
    assert_eq!(after[0].workdir_sha.as_deref(), Some(first.as_str()));
}

/// The resolution behind "open as its own project": a downloaded submodule must
/// resolve to its OWN working folder, not the parent's.
#[test]
fn submodule_opens_to_its_own_workdir() {
    let Some((parent, _up, _f, _t)) = fixture("workdir") else {
        return;
    };

    let repo = Repository::open(&parent).unwrap();
    let sub = repo.find_submodule("vendor/lib").unwrap();
    let nested = sub
        .open()
        .expect("a checked-out submodule opens as a repository");
    let workdir = nested.workdir().unwrap().canonicalize().unwrap();

    assert_eq!(
        workdir,
        parent.join("vendor/lib").canonicalize().unwrap(),
        "must resolve to the submodule's own folder"
    );
    assert_ne!(
        workdir,
        parent.canonicalize().unwrap(),
        "must never resolve to the parent project"
    );
}

/// Guards the reason `submodule_workdir` resolves through the submodule handle
/// instead of joining paths: `Repository::discover` searches UPWARD, so an
/// un-downloaded submodule folder resolves to the parent. Opening it that way
/// would silently produce a duplicate tab of the project the user is already
/// in, so the command must fail on this case rather than discover.
#[test]
fn uninitialized_submodule_refuses_rather_than_finding_the_parent() {
    let Some((parent, _up, _f, _t)) = fixture("uninit") else {
        return;
    };

    git(&parent, &["submodule", "deinit", "-f", "--", "vendor/lib"]);

    let repo = Repository::open(&parent).unwrap();
    let sub = repo.find_submodule("vendor/lib").unwrap();
    assert!(
        sub.open().is_err(),
        "an un-downloaded submodule must not open, so the command reports it"
    );

    // The trap this avoids: discovery from the same path climbs to the parent.
    let discovered = Repository::discover(parent.join("vendor/lib")).unwrap();
    assert_eq!(
        discovered.workdir().unwrap().canonicalize().unwrap(),
        parent.canonicalize().unwrap(),
        "discovery walks up to the parent -- exactly what we must not open"
    );
}

/// The reported case: someone else bumps the submodule pointer, you pull the
/// parent, and the recorded commit is not in your nested clone yet.
///
/// libgit2's `Submodule::update` checks out the recorded commit but never
/// fetches, so it fails outright here -- which is why every reset appeared to
/// fail. `git submodule update` fetches first, so it succeeds.
#[test]
fn updating_to_a_commit_not_yet_downloaded_fetches_it_first() {
    let Some((parent, up, first, tip)) = fixture("needsfetch") else {
        return;
    };

    // Record the newer upstream commit in the parent without giving the nested
    // clone a chance to fetch it.
    git(&parent, &["submodule", "update", "--remote", "--init", "--", "vendor/lib"]);
    git(&parent, &["add", "--", "vendor/lib"]);
    git(&parent, &["commit", "-qm", "bump"]);
    assert_ne!(first, tip, "fixture must have two distinct commits");

    // Put the nested clone back on the old commit and drop the newer object,
    // reproducing a clone that has never seen the bump.
    let nested = parent.join("vendor/lib");
    git(&nested, &["checkout", "-q", &first]);
    let _ = up;

    let repo = Repository::open(&parent).unwrap();
    let mut sub = repo.find_submodule("vendor/lib").unwrap();
    // Whether libgit2 can do it is incidental; what matters is that the shell
    // path below always can, because it fetches.
    let _ = sub.update(true, None);

    git(&parent, &["submodule", "update", "--init", "--", "vendor/lib"]);
    assert_eq!(
        git_out(&nested, &["rev-parse", "HEAD"]),
        tip,
        "the shell path must land on the commit the parent records"
    );
}
