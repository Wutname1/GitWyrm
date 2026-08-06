//! One place to turn on rename detection, with a cap.
//!
//! Rename detection pairs every added blob against every deleted blob, so its
//! cost grows with the product of the two. On normal commits that is trivial
//! and worth it: a renamed file reads as one change instead of a delete plus an
//! add. On a vendored-source drop that rewrites thousands of paths at once it
//! turns a single commit's diffstat into seconds of work.
//!
//! Detection is gated on the diff's own size rather than tuned with libgit2's
//! `rename_limit`. That option is NOT git's `diff.renameLimit`: instead of
//! skipping detection wholesale past a threshold, it does partial detection,
//! which changes the reported insertions and deletions. Measured on a 2,891
//! delta commit, `rename_limit` 50 / 200 / 1000 gave +326498 / +221496 /
//! +52727 -- three different answers for the same commit. A size gate keeps
//! every diff either fully detected or not detected at all, so the numbers are
//! never partially resolved.
//!
//! Cost on that same commit: 862ms with no detection, 5,947ms with it.

use git2::{Diff, DiffFindOptions};

/// Diffs with more deltas than this skip rename detection entirely.
///
/// Ordinary commits sit far below it and are unaffected. Past it, a rename
/// reads as a delete plus an add -- the same thing git shows when it gives up
/// on rename detection -- which is a far better outcome than making the user
/// wait seconds per row for cosmetic attribution.
const MAX_DELTAS_FOR_RENAMES: usize = 1000;

/// Run rename detection over `diff` unless it is large enough that detection
/// would dominate the request.
///
/// Every diff shown in the UI goes through here, so the log list and the
/// details view can never disagree about a commit's numbers.
pub fn find_renames(diff: &mut Diff<'_>) -> Result<(), git2::Error> {
  if diff.deltas().len() > MAX_DELTAS_FOR_RENAMES {
    return Ok(());
  }
  let mut find = DiffFindOptions::new();
  find.renames(true);
  diff.find_similar(Some(&mut find))
}

#[cfg(test)]
mod tests {
  use super::*;
  use git2::{Repository, Signature};
  use std::fs;

  /// A plain rename under the limit must still collapse to a single delta,
  /// otherwise the cap has broken ordinary detection.
  #[test]
  fn small_rename_is_still_detected() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let sig = Signature::now("Rename Test", "rename@example.com").expect("signature");

    // Content long enough that similarity scoring is unambiguous.
    let body = (0..40).map(|i| format!("line {i}\n")).collect::<String>();
    fs::write(dir.path().join("before.txt"), &body).expect("write");
    let mut index = repo.index().expect("index");
    index.add_path(std::path::Path::new("before.txt")).expect("add");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("tree");
    let first = repo
      .commit(Some("HEAD"), &sig, &sig, "add", &repo.find_tree(tree_id).expect("tree"), &[])
      .expect("commit");

    fs::rename(dir.path().join("before.txt"), dir.path().join("after.txt")).expect("rename");
    let mut index = repo.index().expect("index");
    index.remove_path(std::path::Path::new("before.txt")).expect("remove");
    index.add_path(std::path::Path::new("after.txt")).expect("add");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("tree");
    let new_tree = repo.find_tree(tree_id).expect("tree");
    let parent = repo.find_commit(first).expect("parent");
    let old_tree = parent.tree().expect("old tree");

    let mut diff = repo
      .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)
      .expect("diff");
    assert_eq!(diff.deltas().len(), 2, "before detection: a delete plus an add");

    find_renames(&mut diff).expect("find renames");
    assert_eq!(diff.deltas().len(), 1, "a rename under the cap must collapse to one delta");
  }

  /// Commit exactly `count` files named `{prefix}{i}.txt` and nothing else.
  ///
  /// The index is rebuilt from empty each time, so a second call with a
  /// different prefix genuinely deletes the first call's paths instead of
  /// carrying them forward -- which is what makes the diff a full
  /// delete-all + add-all pair.
  fn commit_many(repo: &Repository, prefix: &str, count: usize, parent: Option<git2::Oid>) -> git2::Oid {
    let dir = repo.workdir().expect("workdir").to_path_buf();
    let sig = Signature::now("Rename Test", "rename@example.com").expect("signature");
    let mut index = repo.index().expect("index");
    index.clear().expect("clear index");
    for i in 0..count {
      let name = format!("{prefix}{i}.txt");
      fs::write(dir.join(&name), format!("contents of file {i}\n")).expect("write");
      index.add_path(std::path::Path::new(&name)).expect("add");
    }
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("tree");
    let tree = repo.find_tree(tree_id).expect("tree");
    let parents: Vec<_> = parent
      .map(|p| repo.find_commit(p).expect("parent"))
      .into_iter()
      .collect();
    let refs: Vec<_> = parents.iter().collect();
    repo
      .commit(Some("HEAD"), &sig, &sig, "bulk", &tree, &refs)
      .expect("commit")
  }

  /// The whole point of the gate: a diff past the threshold must skip detection
  /// rather than spend seconds on it. Renames there stay as add+delete pairs.
  #[test]
  fn a_huge_diff_skips_rename_detection() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");

    // Every path is replaced by a differently-named copy, so full detection
    // would pair them all up. Sized past the gate on the delete+add total.
    let n = MAX_DELTAS_FOR_RENAMES;
    let first = commit_many(&repo, "old", n, None);
    for i in 0..n {
      fs::remove_file(dir.path().join(format!("old{i}.txt"))).expect("remove");
    }
    let second = commit_many(&repo, "new", n, Some(first));

    let old_tree = repo.find_commit(first).expect("c").tree().expect("t");
    let new_tree = repo.find_commit(second).expect("c").tree().expect("t");
    let mut diff = repo
      .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)
      .expect("diff");

    let before = diff.deltas().len();
    assert!(before > MAX_DELTAS_FOR_RENAMES, "fixture must exceed the gate");

    find_renames(&mut diff).expect("find renames");
    assert_eq!(
      diff.deltas().len(),
      before,
      "past the gate the diff must be left exactly as it was, not partially resolved"
    );
  }

  /// Skipping detection must not change the line counts the UI shows -- only
  /// how a rename is attributed. This is what a `rename_limit` tuned to a
  /// smaller value would silently break.
  #[test]
  fn skipping_detection_leaves_line_counts_exact() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let n = MAX_DELTAS_FOR_RENAMES;
    let first = commit_many(&repo, "old", n, None);
    for i in 0..n {
      fs::remove_file(dir.path().join(format!("old{i}.txt"))).expect("remove");
    }
    let second = commit_many(&repo, "new", n, Some(first));

    let old_tree = repo.find_commit(first).expect("c").tree().expect("t");
    let new_tree = repo.find_commit(second).expect("c").tree().expect("t");
    let raw = repo
      .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)
      .expect("diff");
    let expected = raw.stats().expect("stats");
    let (want_ins, want_del) = (expected.insertions(), expected.deletions());

    let mut diff = repo
      .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)
      .expect("diff");
    find_renames(&mut diff).expect("find renames");
    let got = diff.stats().expect("stats");

    assert_eq!(got.insertions(), want_ins, "insertions must be unchanged");
    assert_eq!(got.deletions(), want_del, "deletions must be unchanged");
  }
}
