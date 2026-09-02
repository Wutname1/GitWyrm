//! End-to-end merge + conflict resolution against throwaway repos. Conflict
//! reading and resolution go through the real gitwyrm_lib::git_merge_ops code
//! the commands use, so these tests catch drift in the actual logic.

use std::fs;
use std::path::{Path, PathBuf};

use git2::{build::CheckoutBuilder, MergeOptions, Repository, Signature};
use gitwyrm_lib::git_merge_ops::{apply_resolution, conflict_content, merge_state, Resolution};
use gitwyrm_lib::git_types::OperationKind;

fn scratch_repo(tag: &str) -> (PathBuf, Repository) {
    let dir = std::env::temp_dir().join(format!("gitwyrm-merge-{}-{}", tag, std::process::id()));
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

/// The default branch name after the first commit (init.defaultBranch varies).
fn default_branch(repo: &Repository) -> String {
    repo.head().unwrap().shorthand().unwrap().to_string()
}

/// Stage every change (including deletions) and commit; returns the new oid.
fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.update_all(["*"], None).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig(), &sig(), message, &tree, &parent_refs)
        .unwrap()
}

fn checkout_branch(repo: &Repository, name: &str) {
    repo.set_head(&format!("refs/heads/{name}")).unwrap();
    repo.checkout_head(Some(CheckoutBuilder::new().force()))
        .unwrap();
}

/// Branch off `base`, run `mutate` there and commit, then return to `main` and
/// run `mutate_main` and commit. Returns the feature tip for merging.
fn diverge(
    repo: &Repository,
    base: git2::Oid,
    main: &str,
    mutate_feature: impl FnOnce(),
    mutate_main: impl FnOnce(),
) -> git2::Oid {
    repo.branch("feature", &repo.find_commit(base).unwrap(), false)
        .unwrap();
    checkout_branch(repo, "feature");
    mutate_feature();
    let feat = commit_all(repo, "feature change");
    checkout_branch(repo, main);
    mutate_main();
    commit_all(repo, "main change");
    feat
}

/// Merge `feat` into HEAD expecting conflicts, as merge_branch's normal path does.
fn merge_expect_conflict(repo: &Repository, feat: git2::Oid) {
    let annotated = repo.find_annotated_commit(feat).unwrap();
    let mut checkout = CheckoutBuilder::new();
    // Matches do_merge: diff3 keeps independent edits as separate conflicts.
    checkout.allow_conflicts(true).conflict_style_diff3(true);
    repo.merge(
        &[&annotated],
        Some(&mut MergeOptions::new()),
        Some(&mut checkout),
    )
    .unwrap();
    assert_eq!(
        repo.state(),
        git2::RepositoryState::Merge,
        "merge should be in progress"
    );
    assert!(
        repo.index().unwrap().has_conflicts(),
        "merge should conflict"
    );
}

#[test]
fn fast_forward_merge_advances_head() {
    let (dir, repo) = scratch_repo("ff");

    fs::write(dir.join("f.txt"), "base\n").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    // Branch off, add a commit on feature only -> main can fast-forward to it.
    repo.branch("feature", &repo.find_commit(base).unwrap(), false)
        .unwrap();
    checkout_branch(&repo, "feature");
    fs::write(dir.join("f.txt"), "base\nfeature line\n").unwrap();
    let feat = commit_all(&repo, "feature work");
    checkout_branch(&repo, &main);

    let annotated = repo.find_annotated_commit(feat).unwrap();
    let (analysis, _) = repo.merge_analysis(&[&annotated]).unwrap();
    assert!(analysis.is_fast_forward(), "should be fast-forwardable");

    // Perform the fast-forward as merge_branch does.
    let target = repo.find_object(feat, None).unwrap();
    repo.checkout_tree(&target, Some(CheckoutBuilder::new().safe()))
        .unwrap();
    repo.reference(&format!("refs/heads/{main}"), feat, true, "ff")
        .unwrap();

    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), feat);
    assert!(!repo.index().unwrap().has_conflicts());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn conflicting_merge_resolves_and_commits() {
    let (dir, repo) = scratch_repo("conflict");

    fs::write(dir.join("f.txt"), "line1\nSHARED\nline3\n").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    let feat = diverge(
        &repo,
        base,
        &main,
        || fs::write(dir.join("f.txt"), "line1\nTHEIRS\nline3\n").unwrap(),
        || fs::write(dir.join("f.txt"), "line1\nOURS\nline3\n").unwrap(),
    );
    merge_expect_conflict(&repo, feat);

    // Read the three sides through the real conflict_content.
    let content = conflict_content(&repo, &dir, "f.txt").unwrap();
    assert!(content.ours.contains("OURS"), "stage 2 = ours");
    assert!(content.theirs.contains("THEIRS"), "stage 3 = theirs");
    assert!(content.base.contains("SHARED"), "stage 1 = common ancestor");
    assert!(
        content.merged.contains("<<<<<<<"),
        "working tree has markers"
    );
    assert!(!content.binary);
    assert!(!content.ours_deleted && !content.theirs_deleted);

    // Resolve manually through the real apply_resolution.
    let resolved = "line1\nMERGED BY HAND\nline3\n";
    apply_resolution(
        &repo,
        &dir,
        "f.txt",
        &Resolution::Manual {
            text: resolved.into(),
        },
    )
    .unwrap();
    assert!(
        !repo.index().unwrap().has_conflicts(),
        "conflict cleared after resolve"
    );

    // Commit the merge (mirror commit_merge: two parents, cleanup state).
    let merge_head = {
        let content = fs::read_to_string(repo.path().join("MERGE_HEAD")).unwrap();
        git2::Oid::from_str(content.trim()).unwrap()
    };
    assert_eq!(merge_head, feat, "MERGE_HEAD points at the merged commit");

    let merge_oid = {
        let mut index = repo.index().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let merge_commit = repo.find_commit(merge_head).unwrap();
        repo.commit(
            Some("HEAD"),
            &sig(),
            &sig(),
            "Merge feature",
            &tree,
            &[&head_commit, &merge_commit],
        )
        .unwrap()
    };
    repo.cleanup_state().unwrap();

    let merge_commit = repo.find_commit(merge_oid).unwrap();
    assert_eq!(
        merge_commit.parent_count(),
        2,
        "merge commit has two parents"
    );
    assert_eq!(
        repo.state(),
        git2::RepositoryState::Clean,
        "state cleaned up"
    );
    assert_eq!(
        fs::read_to_string(dir.join("f.txt"))
            .unwrap()
            .replace("\r\n", "\n"),
        resolved
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn binary_conflict_resolution_keeps_chosen_bytes() {
    let (dir, repo) = scratch_repo("binary");

    let base_bytes: &[u8] = b"BIN\x00base\x01\x02";
    let ours_bytes: &[u8] = b"BIN\x00ours\x03\x04\x05";
    let theirs_bytes: &[u8] = b"BIN\x00theirs\x06";

    fs::write(dir.join("img.bin"), base_bytes).unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    let feat = diverge(
        &repo,
        base,
        &main,
        || fs::write(dir.join("img.bin"), theirs_bytes).unwrap(),
        || fs::write(dir.join("img.bin"), ours_bytes).unwrap(),
    );
    merge_expect_conflict(&repo, feat);

    let content = conflict_content(&repo, &dir, "img.bin").unwrap();
    assert!(content.binary, "null bytes should read as binary");

    // Choosing a side must reproduce that side's exact bytes, not text.
    apply_resolution(&repo, &dir, "img.bin", &Resolution::Ours).unwrap();
    assert_eq!(
        fs::read(dir.join("img.bin")).unwrap(),
        ours_bytes,
        "ours bytes intact"
    );
    assert!(!repo.index().unwrap().has_conflicts());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn modify_delete_conflict_can_keep_or_delete() {
    let (dir, repo) = scratch_repo("moddel");

    // Two files sharing the same fate: ours modifies both, theirs deletes both.
    fs::write(dir.join("keep.txt"), "original\n").unwrap();
    fs::write(dir.join("drop.txt"), "original\n").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    let feat = diverge(
        &repo,
        base,
        &main,
        || {
            fs::remove_file(dir.join("keep.txt")).unwrap();
            fs::remove_file(dir.join("drop.txt")).unwrap();
        },
        || {
            fs::write(dir.join("keep.txt"), "ours edit\n").unwrap();
            fs::write(dir.join("drop.txt"), "ours edit\n").unwrap();
        },
    );
    merge_expect_conflict(&repo, feat);

    let content = conflict_content(&repo, &dir, "keep.txt").unwrap();
    assert!(content.theirs_deleted, "their side deleted the file");
    assert!(!content.ours_deleted);
    assert!(content.ours.contains("ours edit"));

    // Keep our modified copy for one file.
    apply_resolution(&repo, &dir, "keep.txt", &Resolution::Ours).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("keep.txt")).unwrap(),
        "ours edit\n"
    );

    // Accept their deletion for the other: file gone, deletion staged.
    apply_resolution(&repo, &dir, "drop.txt", &Resolution::Theirs).unwrap();
    assert!(
        !dir.join("drop.txt").exists(),
        "choosing the deleting side removes the file"
    );

    let index = repo.index().unwrap();
    assert!(!index.has_conflicts(), "both conflicts cleared");
    assert!(
        index.get_path(Path::new("keep.txt"), 0).is_some(),
        "kept file staged"
    );
    assert!(
        index.get_path(Path::new("drop.txt"), 0).is_none(),
        "deletion staged"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn merge_state_reports_an_in_progress_merge() {
    let (dir, repo) = scratch_repo("state-merge");

    fs::write(dir.join("f.txt"), "line1\nSHARED\nline3\n").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    let feat = diverge(
        &repo,
        base,
        &main,
        || fs::write(dir.join("f.txt"), "line1\nTHEIRS\nline3\n").unwrap(),
        || fs::write(dir.join("f.txt"), "line1\nOURS\nline3\n").unwrap(),
    );
    merge_expect_conflict(&repo, feat);

    let state = merge_state(&repo).unwrap();
    assert!(state.merging, "a merge is in progress");
    assert_eq!(state.operation, Some(OperationKind::Merge));
    assert_eq!(state.conflicts, vec!["f.txt".to_string()]);

    apply_resolution(&repo, &dir, "f.txt", &Resolution::Ours).unwrap();
    let state = merge_state(&repo).unwrap();
    assert!(state.merging, "still merging until it is committed");
    assert!(state.conflicts.is_empty(), "resolved path drops out");

    let _ = fs::remove_dir_all(&dir);
}

/// Applying a stash leaves conflicts with no MERGE_HEAD and no rebase directory,
/// so the repo state reads Clean. Those paths must still be reported, or the
/// changes list flags a file `conflict` while the conflict view claims there is
/// nothing to resolve. This is the state a branch switch's auto-stash lands in.
#[test]
fn merge_state_reports_conflicts_without_an_operation() {
    let (dir, mut repo) = scratch_repo("state-stash");

    fs::write(dir.join("f.txt"), "line1\nSHARED\nline3\n").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    // A feature branch that touches the same line the stash will.
    repo.branch("feature", &repo.find_commit(base).unwrap(), false)
        .unwrap();
    checkout_branch(&repo, "feature");
    fs::write(dir.join("f.txt"), "line1\nFEATURE\nline3\n").unwrap();
    commit_all(&repo, "feature change");
    checkout_branch(&repo, &main);

    // Uncommitted work, stashed so the switch can happen, then reapplied on the
    // other branch -- exactly what checkout_branch's auto-stash path does.
    fs::write(dir.join("f.txt"), "line1\nMY WORK\nline3\n").unwrap();
    repo.stash_save(
        &sig(),
        "auto-stash",
        Some(git2::StashFlags::INCLUDE_UNTRACKED),
    )
    .unwrap();
    checkout_branch(&repo, "feature");
    let _ = repo.stash_apply(0, None);

    assert!(
        repo.index().unwrap().has_conflicts(),
        "stash apply should conflict"
    );
    assert_eq!(
        repo.state(),
        git2::RepositoryState::Clean,
        "a stash apply leaves no operation state behind"
    );

    let state = merge_state(&repo).unwrap();
    assert_eq!(
        state.conflicts,
        vec!["f.txt".to_string()],
        "conflicted path is reported"
    );
    assert!(!state.merging, "there is no merge to commit or abort");
    assert_eq!(state.operation, None);

    // The conflict view's data path works the same here as during a merge.
    let content = conflict_content(&repo, &dir, "f.txt").unwrap();
    assert!(
        content.ours.contains("FEATURE"),
        "stage 2 = the branch we are on"
    );
    assert!(
        content.theirs.contains("MY WORK"),
        "stage 3 = the stashed work"
    );
    assert!(
        content.merged.contains("<<<<<<<"),
        "working tree has markers"
    );

    apply_resolution(&repo, &dir, "f.txt", &Resolution::Theirs).unwrap();
    assert!(
        merge_state(&repo).unwrap().conflicts.is_empty(),
        "resolved path drops out"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn abort_merge_restores_pre_merge_state() {
    let (dir, repo) = scratch_repo("abort");

    fs::write(dir.join("f.txt"), "line1\nSHARED\nline3\n").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    let feat = diverge(
        &repo,
        base,
        &main,
        || fs::write(dir.join("f.txt"), "line1\nTHEIRS\nline3\n").unwrap(),
        || fs::write(dir.join("f.txt"), "line1\nOURS\nline3\n").unwrap(),
    );
    let ours_head = repo.head().unwrap().peel_to_commit().unwrap().id();
    merge_expect_conflict(&repo, feat);

    // Abort: hard reset to HEAD + cleanup, as abort_merge does.
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    repo.reset(
        head_commit.as_object(),
        git2::ResetType::Hard,
        Some(CheckoutBuilder::new().force()),
    )
    .unwrap();
    repo.cleanup_state().unwrap();

    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert!(!repo.index().unwrap().has_conflicts());
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        ours_head
    );
    assert_eq!(
        fs::read_to_string(dir.join("f.txt"))
            .unwrap()
            .replace("\r\n", "\n"),
        "line1\nOURS\nline3\n",
        "our version restored"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Two edits with untouched lines between them must stay two separately
/// resolvable conflicts.
///
/// This is the whole reason `conflict_content` regenerates its marker text
/// instead of reading the working copy. Git's default marker style reports this
/// file as ONE conflict whose two sides each span the untouched middle lines,
/// so a per-hunk resolver would offer a single choice over the entire region --
/// exactly the thing the hunk view exists to avoid. diff3 consults the common
/// ancestor, sees the middle was never contested, and splits them.
#[test]
fn independent_edits_stay_separate_conflicts() {
    let (dir, repo) = scratch_repo("diff3-hunks");
    let file = dir.join("f.txt");

    fs::write(&file, "line1
line2
line3
line4
line5
").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    // Each side edits the first and last lines, leaving the middle alone.
    let feat = diverge(
        &repo,
        base,
        &main,
        || fs::write(&file, "line1
THEIRS-A
line3
line4
THEIRS-B
").unwrap(),
        || fs::write(&file, "line1
OURS-A
line3
line4
OURS-B
").unwrap(),
    );
    merge_expect_conflict(&repo, feat);

    let content = conflict_content(&repo, &dir, "f.txt").unwrap();

    let opens = content.conflict_text.matches("<<<<<<<").count();
    assert_eq!(
        opens, 2,
        "expected two separate conflicts, got {opens}:
{}",
        content.conflict_text
    );

    // The ancestor section is what makes the split possible, and the view shows
    // it, so it must actually be present.
    assert!(
        content.conflict_text.contains("|||||||"),
        "diff3 markers should carry the common ancestor:
{}",
        content.conflict_text
    );

    // The untouched middle belongs to neither side: it must sit outside the
    // markers rather than being duplicated into both.
    let inside_first_conflict = content
        .conflict_text
        .split("<<<<<<<")
        .nth(1)
        .expect("a first conflict");
    let first_block = inside_first_conflict
        .split(">>>>>>>")
        .next()
        .expect("a closing marker");
    assert!(
        !first_block.contains("line4"),
        "untouched lines should not be swallowed into a conflict:
{}",
        content.conflict_text
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A binary conflict has no marker text to show; the whole-file choices carry it.
#[test]
fn binary_conflict_reports_no_marker_text() {
    let (dir, repo) = scratch_repo("diff3-binary");
    let file = dir.join("blob.bin");

    fs::write(&file, [0u8, 159, 146, 150, 1, 2, 3]).unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    let feat = diverge(
        &repo,
        base,
        &main,
        || fs::write(&file, [0u8, 159, 146, 150, 9, 9, 9]).unwrap(),
        || fs::write(&file, [0u8, 159, 146, 150, 7, 7, 7]).unwrap(),
    );
    merge_expect_conflict(&repo, feat);

    let content = conflict_content(&repo, &dir, "blob.bin").unwrap();
    assert!(content.binary, "should be detected as binary");
    assert!(
        !content.conflict_text.contains("<<<<<<<"),
        "binary files should not produce marker text"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Reading a path that is no longer conflicted must not abort the process.
///
/// Regression: `conflict_content` built its diff3 marker text with
/// `git2::merge_file` and then called `MergeFileResult::content()`
/// unconditionally. That method hands the raw pointer to
/// `slice::from_raw_parts` with no null check, and libgit2 leaves it null when
/// the merge came back clean -- so the call was undefined behaviour that
/// aborted the whole process (STATUS_STACK_BUFFER_OVERRUN), not a recoverable
/// error. It fired in normal use: resolve one file, and the refetch of a
/// sibling path re-read a now-automergeable file and killed the app.
#[test]
fn reading_an_automergeable_path_does_not_abort() {
    let (dir, repo) = scratch_repo("automergeable-read");
    let file = dir.join("f.txt");

    fs::write(&file, "line1
line2
line3
").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    // Both sides edit, so the merge conflicts and the path enters the index...
    let feat = diverge(
        &repo,
        base,
        &main,
        || fs::write(&file, "line1
THEIRS
line3
").unwrap(),
        || fs::write(&file, "line1
OURS
line3
").unwrap(),
    );
    merge_expect_conflict(&repo, feat);

    // ...then resolve it, which collapses the three stages to one.
    apply_resolution(&repo, &dir, "f.txt", &Resolution::Ours).unwrap();

    // The view refetches after resolving. Reading the settled path must return
    // rather than abort; the marker text is empty because nothing conflicts.
    let content = conflict_content(&repo, &dir, "f.txt").unwrap();
    assert!(
        !content.conflict_text.contains("<<<<<<<"),
        "a resolved path has no conflict to describe"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Two sides that changed nothing relative to the base merge cleanly.
#[test]
fn identical_sides_produce_no_marker_text() {
    let (dir, repo) = scratch_repo("identical-sides");
    let file = dir.join("f.txt");

    fs::write(&file, "same
").unwrap();
    let base = commit_all(&repo, "base");
    let main = default_branch(&repo);

    // Conflict on a second file so the merge stops, while f.txt stays clean.
    let other = dir.join("other.txt");
    fs::write(&other, "base
").unwrap();
    commit_all(&repo, "add other");

    let feat = diverge(
        &repo,
        base,
        &main,
        || fs::write(&other, "theirs
").unwrap(),
        || fs::write(&other, "ours
").unwrap(),
    );
    merge_expect_conflict(&repo, feat);

    // f.txt was never conflicted; reading it must be safe.
    let content = conflict_content(&repo, &dir, "f.txt").unwrap();
    assert!(!content.conflict_text.contains("<<<<<<<"));

    let _ = fs::remove_dir_all(&dir);
}
