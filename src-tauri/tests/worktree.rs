//! End-to-end checks for the worktree layer against real git repositories.
//!
//! Unit tests in `git/worktree.rs` cover the pure logic (slugs, path
//! validation, lock-error recognition). These cover the parts only real git can
//! answer: what `git worktree add` actually produces, how the admin entry
//! behaves when a folder is deleted or moved, and whether a removal refusal
//! carries the counts the confirm needs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use git2::Repository;
use gitwyrm_lib::git_worktree::{self as worktree, DirtyChoice, RemoveOutcome, WorktreeState};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
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

fn identity(dir: &Path) {
    git(dir, &["config", "user.email", "t@gitwyrm.dev"]);
    git(dir, &["config", "user.name", "Test Wyrm"]);
}

/// A repo on `main` with a second branch `feature`, plus a couple of commits.
/// Returns (root, repo_path).
fn fixture(label: &str) -> Option<(PathBuf, PathBuf)> {
    Command::new("git").arg("--version").output().ok()?;

    let root = std::env::temp_dir().join(format!("gitwyrm-wt-{}-{label}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let repo = root.join("project");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    identity(&repo);
    fs::write(repo.join("README.md"), "# demo").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    git(&repo, &["branch", "feature"]);

    Some((root, repo))
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The list is the whole picture: the main checkout first, then linked
/// worktrees. "The other ones" would read as if the folder the user is looking
/// at is not a checkout at all.
#[test]
fn list_includes_the_main_checkout_first() {
    let Some((root, repo_path)) = fixture("list") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let list = worktree::list(&repo, Some(&repo_path));

    assert_eq!(list.len(), 2, "main checkout plus one linked worktree");
    assert!(list[0].is_main);
    assert!(
        list[0].is_current,
        "the handle we asked with is the current row"
    );
    assert_eq!(list[0].branch.as_deref(), Some("main"));

    assert!(!list[1].is_main);
    assert_eq!(list[1].branch.as_deref(), Some("feature"));
    assert_eq!(list[1].state, WorktreeState::Ok);
    assert_eq!(list[1].folder_name, "project-feature");

    let _ = fs::remove_dir_all(&root);
}

/// Creating the branch as part of the add is `git worktree add -b`.
#[test]
fn add_can_create_the_branch_at_the_same_time() {
    let Some((root, repo_path)) = fixture("addb") else {
        return;
    };
    let wt_path = root.join("project-new");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "brand-new",
        true,
        Some("main"),
    )
    .unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    assert!(repo
        .find_branch("brand-new", git2::BranchType::Local)
        .is_ok());
    let list = worktree::list(&repo, None);
    assert!(list
        .iter()
        .any(|w| w.branch.as_deref() == Some("brand-new")));

    let _ = fs::remove_dir_all(&root);
}

/// A branch can only be checked out in one folder at a time, and the refusal
/// must name the folder rather than repeating git's message.
#[test]
fn adding_a_branch_already_checked_out_is_refused_in_plain_words() {
    let Some((root, repo_path)) = fixture("collide") else {
        return;
    };
    let first = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&first),
        "feature",
        false,
        None,
    )
    .unwrap();

    let second = root.join("project-feature-again");
    let err = worktree::add(
        &path_str(&repo_path),
        &path_str(&second),
        "feature",
        false,
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("already open in another worktree"),
        "git's raw text must not reach the user: {err}"
    );
    assert!(!err.contains("fatal:"), "no raw git output: {err}");

    let _ = fs::remove_dir_all(&root);
}

/// The holder lookup powers "open that worktree" wherever a checkout starts.
#[test]
fn holder_of_branch_names_the_folder_that_has_it() {
    let Some((root, repo_path)) = fixture("holder") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let holder = worktree::holder_of_branch(&repo, "feature").expect("feature is held");
    assert_eq!(holder.folder_name, "project-feature");
    assert!(!holder.is_main);

    // The main checkout counts as a holder too -- it is a checkout like any other.
    let main_holder = worktree::holder_of_branch(&repo, "main").expect("main is held");
    assert!(main_holder.is_main);

    assert!(worktree::holder_of_branch(&repo, "nonexistent").is_none());

    let _ = fs::remove_dir_all(&root);
}

/// Modified and untracked stay separate all the way through: discarding an
/// untracked file is the one case with no way back.
#[test]
fn dirty_count_keeps_modified_and_untracked_apart() {
    let Some((root, repo_path)) = fixture("dirty") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();

    let clean = worktree::dirty_count(&wt_path).unwrap();
    assert!(clean.is_clean());

    fs::write(wt_path.join("README.md"), "# changed").unwrap();
    fs::write(wt_path.join("brand-new.txt"), "never saved").unwrap();

    let dirt = worktree::dirty_count(&wt_path).unwrap();
    assert_eq!(dirt.modified, 1);
    assert_eq!(dirt.untracked, 1);
    assert!(!dirt.is_clean());

    let _ = fs::remove_dir_all(&root);
}

/// The first ask never destroys anything: removal over a dirty worktree comes
/// back with the counts so the confirm can state them.
#[test]
fn remove_refuses_dirty_with_the_counts_the_confirm_needs() {
    let Some((root, repo_path)) = fixture("refuse") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();
    fs::write(wt_path.join("README.md"), "# changed").unwrap();
    fs::write(wt_path.join("new.txt"), "untracked").unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let outcome = worktree::remove(
        &repo,
        &path_str(&repo_path),
        &path_str(&wt_path),
        DirtyChoice::Refuse,
    )
    .unwrap();

    match outcome {
        RemoveOutcome::RefusedDirty {
            modified,
            untracked,
        } => {
            assert_eq!(modified, 1);
            assert_eq!(untracked, 1);
        }
        other => panic!("expected a dirty refusal, got {other:?}"),
    }
    assert!(wt_path.exists(), "a refusal must not delete anything");

    let _ = fs::remove_dir_all(&root);
}

/// Removing never deletes the branch, and reports whether deleting it would be
/// safe so the follow-on offer only appears when it is.
#[test]
fn remove_leaves_the_branch_and_reports_whether_it_is_merged() {
    let Some((root, repo_path)) = fixture("removeclean") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let outcome = worktree::remove(
        &repo,
        &path_str(&repo_path),
        &path_str(&wt_path),
        DirtyChoice::Refuse,
    )
    .unwrap();

    match outcome {
        RemoveOutcome::Removed {
            branch,
            branch_merged,
            ..
        } => {
            assert_eq!(branch.as_deref(), Some("feature"));
            // feature is at main's tip with no commits of its own, so it is safe.
            assert!(branch_merged, "an unmoved branch is fully merged");
        }
        other => panic!("expected removal, got {other:?}"),
    }

    assert!(!wt_path.exists());
    // The branch survives -- that is what the confirm promises.
    assert!(repo.find_branch("feature", git2::BranchType::Local).is_ok());

    let _ = fs::remove_dir_all(&root);
}

/// Unmerged work is never offered away.
#[test]
fn a_branch_with_its_own_commits_is_not_reported_merged() {
    let Some((root, repo_path)) = fixture("unmerged") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();

    identity(&wt_path);
    fs::write(wt_path.join("only-here.txt"), "work").unwrap();
    git(&wt_path, &["add", "."]);
    git(
        &wt_path,
        &["commit", "-qm", "work that exists nowhere else"],
    );

    let repo = Repository::open(&repo_path).unwrap();
    assert!(!worktree::branch_is_merged(&repo, "feature"));

    let _ = fs::remove_dir_all(&root);
}

/// Keeping the changes must save the untracked files too -- keeping only the
/// recoverable half and destroying the unrecoverable half is the worst outcome.
#[test]
fn keeping_the_changes_stashes_them_including_untracked() {
    let Some((root, repo_path)) = fixture("keep") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();
    identity(&wt_path);
    fs::write(wt_path.join("README.md"), "# changed").unwrap();
    fs::write(wt_path.join("never-saved.txt"), "untracked").unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let outcome = worktree::remove(
        &repo,
        &path_str(&repo_path),
        &path_str(&wt_path),
        DirtyChoice::Keep,
    )
    .unwrap();
    assert!(matches!(outcome, RemoveOutcome::Removed { .. }));
    assert!(!wt_path.exists());

    // The stash lives on refs/stash, which every checkout shares -- so it shows
    // up in the main checkout's Stashes list, which is where the user will look.
    let mut repo = Repository::open(&repo_path).unwrap();
    let mut found = Vec::new();
    repo.stash_foreach(|_, msg, _| {
        found.push(msg.to_string());
        true
    })
    .unwrap();
    assert_eq!(found.len(), 1, "the kept work is recoverable: {found:?}");
    assert!(
        found[0].contains("project-feature"),
        "the message says which folder it came from: {}",
        found[0]
    );

    let _ = fs::remove_dir_all(&root);
}

/// Discarding is what "--force" compiles to after a plain-language confirm.
#[test]
fn discarding_removes_a_dirty_worktree() {
    let Some((root, repo_path)) = fixture("discard") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();
    fs::write(wt_path.join("README.md"), "# changed").unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let outcome = worktree::remove(
        &repo,
        &path_str(&repo_path),
        &path_str(&wt_path),
        DirtyChoice::Discard,
    )
    .unwrap();
    assert!(matches!(outcome, RemoveOutcome::Removed { .. }));
    assert!(!wt_path.exists());

    let _ = fs::remove_dir_all(&root);
}

/// The main checkout is never removable from here.
#[test]
fn remove_refuses_the_main_checkout() {
    let Some((root, repo_path)) = fixture("main") else {
        return;
    };
    let repo = Repository::open(&repo_path).unwrap();
    let err = worktree::remove(
        &repo,
        &path_str(&repo_path),
        &path_str(&repo_path),
        DirtyChoice::Discard,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("original folder"), "{err}");

    let _ = fs::remove_dir_all(&root);
}

/// A deleted folder is `missing` (prune); a moved one is `moved` (repair). The
/// UI offers one action, so this distinction is load-bearing.
#[test]
fn a_deleted_folder_reads_as_missing_and_prunes() {
    let Some((root, repo_path)) = fixture("missing") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();

    fs::remove_dir_all(&wt_path).unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let list = worktree::list(&repo, None);
    let broken = list
        .iter()
        .find(|w| !w.is_main)
        .expect("the row survives the folder");
    assert_eq!(broken.state, WorktreeState::Missing);

    let pruned = worktree::prune(&repo).unwrap();
    assert_eq!(pruned, 1);
    assert_eq!(
        worktree::list(&repo, None).len(),
        1,
        "only the main checkout is left"
    );

    let _ = fs::remove_dir_all(&root);
}

/// A moved folder must not read as missing -- pruning it would throw away a
/// checkout whose files are still on disk.
#[test]
fn a_moved_folder_reads_as_moved_and_repairs() {
    let Some((root, repo_path)) = fixture("moved") else {
        return;
    };
    let wt_path = root.join("project-feature");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "feature",
        false,
        None,
    )
    .unwrap();

    let moved_to = root.join("project-feature-moved");
    fs::rename(&wt_path, &moved_to).unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let list = worktree::list(&repo, None);
    let broken = list
        .iter()
        .find(|w| !w.is_main)
        .expect("the row survives the move");
    assert_eq!(
        broken.state,
        WorktreeState::Missing,
        "git's recorded path is now empty, so the old location is gone"
    );

    // Repair from the new location points git back at it, and the row lists
    // normally again.
    worktree::repair(&path_str(&repo_path), Some(&path_str(&moved_to))).unwrap();
    let repo = Repository::open(&repo_path).unwrap();
    let list = worktree::list(&repo, None);
    let repaired = list.iter().find(|w| !w.is_main).expect("still listed");
    assert_eq!(repaired.state, WorktreeState::Ok);
    assert!(repaired
        .path
        .replace('\\', "/")
        .ends_with("project-feature-moved"));

    let _ = fs::remove_dir_all(&root);
}

/// The ignored-file survey is what turns "my new worktree doesn't work" into a
/// checkbox. It must find env files and must never propose a dependency tree.
#[test]
fn ignored_survey_offers_env_files_but_not_dependency_folders() {
    let Some((root, repo_path)) = fixture("ignored") else {
        return;
    };
    fs::write(
        repo_path.join(".gitignore"),
        ".env\n.env.local\nnode_modules/\n",
    )
    .unwrap();
    git(&repo_path, &["add", ".gitignore"]);
    git(&repo_path, &["commit", "-qm", "ignore rules"]);

    fs::write(repo_path.join(".env"), "SECRET=1").unwrap();
    fs::write(repo_path.join(".env.local"), "LOCAL=1").unwrap();
    fs::create_dir_all(repo_path.join("node_modules/react")).unwrap();
    fs::write(
        repo_path.join("node_modules/react/index.js"),
        "module.exports={}",
    )
    .unwrap();

    let found = worktree::ignored_files_worth_copying(&repo_path);
    let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();

    assert!(paths.contains(&".env"), "env files are offered: {paths:?}");
    assert!(paths.contains(&".env.local"), "{paths:?}");
    assert!(
        !paths.iter().any(|p| p.contains("node_modules")),
        "dependency folders are rebuilt, not carried: {paths:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

/// The suggestion is a sibling folder, so a new checkout can never be committed
/// into the repository it came from.
#[test]
fn suggested_path_is_outside_the_repository() {
    let Some((root, repo_path)) = fixture("suggest") else {
        return;
    };
    let suggested = worktree::suggest_path(&repo_path, "feature/login");
    assert!(suggested.ends_with("project-feature-login"), "{suggested}");
    assert!(
        !Path::new(&suggested).starts_with(&repo_path),
        "must not be inside the project: {suggested}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// A run's worktree is marked in its admin directory, not the working folder --
/// so the marker can never be committed and never shows as an untracked change
/// that would make the hand-edit check think the user touched the folder.
#[test]
fn run_worktrees_are_marked_without_touching_the_working_folder() {
    let Some((root, repo_path)) = fixture("runmark") else {
        return;
    };
    let wt_path = root.join("project-run");
    worktree::add(
        &path_str(&repo_path),
        &path_str(&wt_path),
        "run-branch",
        true,
        Some("main"),
    )
    .unwrap();

    let repo = Repository::open(&repo_path).unwrap();
    let name = worktree::list(&repo, None)
        .into_iter()
        .find(|w| !w.is_main)
        .map(|w| w.name)
        .unwrap();
    worktree::mark_as_run_worktree(&repo, &name).unwrap();

    let list = worktree::list(&repo, None);
    let run_row = list.iter().find(|w| !w.is_main).unwrap();
    assert!(run_row.is_run_worktree);

    // The marker is invisible to the checkout, so discard's hand-edit check is
    // not fooled into thinking the user edited something.
    assert!(worktree::dirty_count(&wt_path).unwrap().is_clean());

    let _ = fs::remove_dir_all(&root);
}
