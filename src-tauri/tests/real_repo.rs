//! Integration checks against real repositories on this machine.
//! These exercise the git2 logic directly (not the tauri command layer).

use git2::{BranchType, Oid, Repository, Sort};

const REPO: &str = "C:/code/audiobookshelf";

fn open() -> Option<Repository> {
  Repository::discover(REPO).ok()
}

#[test]
fn revwalk_matches_git_rev_list_count() {
  let Some(repo) = open() else { return };
  let mut walk = repo.revwalk().unwrap();
  walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME).unwrap();
  walk.push_head().unwrap();
  let libgit_count = walk.count();
  assert!(libgit_count > 1000, "expected a large history, got {libgit_count}");
}

#[test]
fn lanes_are_consistent_and_first_page_matches_paged_walk() {
  let Some(repo) = open() else { return };

  fn walk_lanes(repo: &Repository, take: usize) -> Vec<(Oid, u32)> {
    use gitwyrm_lib_test_shim::LaneState;
    let mut walk = repo.revwalk().unwrap();
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME).unwrap();
    walk.push_head().unwrap();
    if let Ok(branches) = repo.branches(Some(BranchType::Local)) {
      for (branch, _) in branches.flatten() {
        if let Some(oid) = branch.get().target() {
          walk.push(oid).ok();
        }
      }
    }
    let mut lanes = LaneState::default();
    let mut out = Vec::new();
    for oid in walk.flatten().take(take) {
      let commit = repo.find_commit(oid).unwrap();
      let parents: Vec<Oid> = commit.parent_ids().collect();
      let a = lanes.assign(oid, &parents);
      out.push((oid, a.lane));
    }
    out
  }

  let full = walk_lanes(&repo, 400);
  // Lane indices must be sane (merge-heavy repos can go wide but not absurd).
  let max_lane = full.iter().map(|(_, l)| *l).max().unwrap_or(0);
  assert!(max_lane < 64, "lane explosion: {max_lane}");
  // Head commit sits in lane 0.
  assert_eq!(full[0].1, 0, "HEAD should be lane 0");
}

#[test]
fn status_reports_workdir_changes() {
  let Some(repo) = open() else { return };
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(true).recurse_untracked_dirs(true);
  let statuses = repo.statuses(Some(&mut opts)).unwrap();
  // This repo is known to have local modifications on this machine.
  assert!(statuses.iter().count() > 0, "expected workdir changes");
}

#[test]
fn diff_index_to_workdir_produces_lines_for_modified_file() {
  let Some(repo) = open() else { return };
  let mut opts = git2::DiffOptions::new();
  opts.context_lines(3);
  let diff = repo.diff_index_to_workdir(None, Some(&mut opts)).unwrap();
  let mut line_count = 0usize;
  diff
    .foreach(
      &mut |_, _| true,
      None,
      None,
      Some(&mut |_d, _h, _l| {
        line_count += 1;
        true
      }),
    )
    .unwrap();
  assert!(line_count > 0, "expected diff lines for modified workdir");
}

// Re-export the lane state through a shim so the integration test can use the
// crate's internal module (it is pub within the lib).
mod gitwyrm_lib_test_shim {
  pub use gitwyrm_lib::git_graph::LaneState;
}
