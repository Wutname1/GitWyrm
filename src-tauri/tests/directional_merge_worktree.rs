//! Directional-merge safety checks that require real linked worktrees.

use std::fs;
use std::path::Path;
use std::process::Command;

use git2::Repository;
use gitwyrm_lib::git_merge_ops::checkout_directional_target;

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

/// Regression: the merge used to checkout main's tree before Git refused to
/// attach HEAD because the main worktree held it. Every differing file was
/// consequently left staged on the linked worktree's branch.
#[test]
fn target_held_elsewhere_is_refused_before_checkout_changes_files() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }

    // A failed run leaves its folders behind, and `git worktree add` records
    // absolute paths, so stale siblings can make a later run resolve the wrong
    // holder. Sweep the whole family before building a fresh one.
    let base = std::env::temp_dir();
    if let Ok(entries) = fs::read_dir(&base) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name
                .to_string_lossy()
                .starts_with("gitwyrm-directional-merge-worktree-")
            {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    let root = base.join(format!(
        "gitwyrm-directional-merge-worktree-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let main_path = root.join("project");
    let feature_path = root.join("project-feature");
    fs::create_dir_all(&main_path).unwrap();

    git(&main_path, &["init", "-q", "-b", "main"]);
    git(&main_path, &["config", "user.email", "test@gitwyrm.dev"]);
    git(&main_path, &["config", "user.name", "Test Wyrm"]);
    fs::write(main_path.join("README.md"), "base\n").unwrap();
    git(&main_path, &["add", "."]);
    git(&main_path, &["commit", "-qm", "base"]);
    git(&main_path, &["branch", "feature"]);
    git(
        &main_path,
        &[
            "worktree",
            "add",
            "-q",
            feature_path.to_str().unwrap(),
            "feature",
        ],
    );

    // Make main visibly different. If checkout happens before the refusal,
    // this file appears staged in the feature worktree.
    fs::write(main_path.join("main-only.txt"), "main\n").unwrap();
    git(&main_path, &["add", "."]);
    git(&main_path, &["commit", "-qm", "main-only"]);

    let repo = Repository::open(&feature_path).unwrap();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();
    let err = checkout_directional_target(&repo, "main", "feature")
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("main is already open in the project folder"),
        "{err}"
    );
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "feature");
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        head_before
    );
    assert!(!feature_path.join("main-only.txt").exists());
    assert!(
        repo.statuses(None).unwrap().is_empty(),
        "index and files stay clean"
    );

    drop(repo);
    let _ = fs::remove_dir_all(&root);
}
