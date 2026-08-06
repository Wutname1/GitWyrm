//! One place to turn on rename detection, with a cap.
//!
//! Rename detection pairs every added blob against every deleted blob, so its
//! cost grows with the product of the two. On normal commits that is trivial
//! and worth it: a renamed file reads as one change instead of a delete plus an
//! add. On a vendored-source drop that rewrites thousands of paths at once it
//! turns a single commit's diffstat into seconds of work.
//!
//! `RENAME_LIMIT` is the ceiling libgit2 applies to that pairing. Past it,
//! detection is skipped for that diff and renames show as separate add/delete
//! rows -- the same fallback git itself uses, and the same tradeoff its
//! `diff.renameLimit` makes. Numbers stay correct either way; only the
//! rename/add+delete attribution changes.

use git2::{Diff, DiffFindOptions};

/// Maximum add x delete pairings before detection is skipped for a diff.
///
/// Matches git's own default so behavior is unsurprising to anyone who knows
/// `diff.renameLimit`. Commits under this are unaffected; only the pathological
/// ones lose rename attribution, and they are exactly the ones that made the
/// graph hang.
const RENAME_LIMIT: u32 = 1000;

/// Run capped rename detection over `diff`.
///
/// Every diff shown in the UI goes through here so the list and the details
/// view can never disagree about a commit's file count.
pub fn find_renames(diff: &mut Diff<'_>) -> Result<(), git2::Error> {
  let mut find = DiffFindOptions::new();
  find.renames(true);
  find.rename_limit(RENAME_LIMIT as usize);
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
}
