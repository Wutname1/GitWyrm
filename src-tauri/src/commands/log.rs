use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use git2::{Commit, Oid, Sort};
use tauri::State;

use crate::error::AppError;
use crate::git::graph::{initials, LaneState};
use crate::git::rename_detect;
use crate::git::trailers;
use crate::git::types::{CommitEntry, CommitStats, LogPage, RefInfo, RefKind};
use crate::state::{OpenRepo, RepoManager};

/// Which commits carry which branch/tag labels.
///
/// Deliberately does NOT go through [`refs::walk_branches`]. That helper also
/// resolves each ref's tip commit for its timestamp and looks up its upstream
/// from config -- two extra operations per ref that the graph never displays.
/// On a repository with over a thousand refs that was tens of milliseconds
/// repeated on every page fetch, which is felt as a stutter while scrolling.
/// Reading the reference names and targets directly is all a row label needs.
fn collect_refs(repo: &git2::Repository) -> HashMap<Oid, Vec<RefInfo>> {
  let mut map: HashMap<Oid, Vec<RefInfo>> = HashMap::new();

  let head_name = repo
    .head()
    .ok()
    .filter(|head| head.is_branch())
    .and_then(|head| head.shorthand().map(str::to_string));

  if let Ok(references) = repo.references() {
    for reference in references.flatten() {
      let Some(name) = reference.name() else { continue };
      let Some(oid) = reference.target() else { continue };

      let (short, ref_type) = if let Some(short) = name.strip_prefix("refs/heads/") {
        let kind = if Some(short) == head_name.as_deref() {
          RefKind::Head
        } else {
          RefKind::Branch
        };
        (short, kind)
      } else if let Some(short) = name.strip_prefix("refs/remotes/") {
        // `origin/HEAD` is a symbolic pointer at the default branch, not a
        // branch of its own; showing it would double-label that commit.
        if short.ends_with("/HEAD") {
          continue;
        }
        (short, RefKind::Remote)
      } else {
        continue;
      };

      map.entry(oid).or_default().push(RefInfo {
        name: short.to_string(),
        ref_type,
      });
    }
  }
  let _ = repo.tag_foreach(|oid, name| {
    let name = String::from_utf8_lossy(name);
    let short = name.trim_start_matches("refs/tags/").to_string();
    // Resolve annotated tags to their target commit.
    let target = repo
      .find_tag(oid)
      .ok()
      .map(|t| t.target_id())
      .unwrap_or(oid);
    map.entry(target).or_default().push(RefInfo {
      name: short,
      ref_type: RefKind::Tag,
    });
    true
  });
  map
}

/// Every commit that can give useful context to the history graph.
///
/// Branch tips alone are not enough: a stash can be based on a commit made
/// while HEAD was detached, and that base remains discoverable only through the
/// stash reflog. Tags can also be the sole owner of a commit. We deliberately push
/// stash *bases* instead of stash commits because stashes have their own graph
/// rows and their extra index/worktree parents are implementation details.
fn collect_log_roots(repo: &git2::Repository) -> Vec<Oid> {
  let mut roots = Vec::new();
  let mut seen = HashSet::new();
  let mut push_commit = |oid: Oid| {
    if seen.insert(oid) && repo.find_commit(oid).is_ok() {
      roots.push(oid);
    }
  };

  // Keep the current checkout first for stable ordering when timestamps tie.
  if let Ok(head) = repo.head() {
    if let Ok(commit) = head.peel_to_commit() {
      push_commit(commit.id());
    }
  }

  // Local branches, remote branches, and tags are all visible graph refs.
  if let Ok(references) = repo.references() {
    for reference in references.flatten() {
      let Some(name) = reference.name() else {
        continue;
      };
      if name == "refs/stash" {
        continue;
      }
      if name.starts_with("refs/heads/")
        || name.starts_with("refs/remotes/")
        || name.starts_with("refs/tags/")
      {
        if let Ok(commit) = reference.peel_to_commit() {
          push_commit(commit.id());
        }
      }
    }
  }

  // Each refs/stash reflog entry is a synthetic stash commit. Its first parent
  // is the real history commit the stash was taken from.
  if let Ok(reflog) = repo.reflog("refs/stash") {
    for i in 0..reflog.len() {
      let Some(entry) = reflog.get(i) else { continue };
      if let Ok(stash) = repo.find_commit(entry.id_new()) {
        if let Ok(base) = stash.parent_id(0) {
          push_commit(base);
        }
      }
    }
  }

  roots
}

/// A cheap fingerprint of where every branch and remote branch currently points.
///
/// [`primary_lane_oid`]'s answer depends on the ref tips as much as on HEAD, so
/// they belong in its memo key. Keyed on HEAD alone, a fetch that moved
/// `origin/<branch>` forward while HEAD stayed put kept serving the answer
/// computed before the fetch: lane zero stayed reserved for the old tip and the
/// commits the fetch brought in rendered as a lane forking off a history that
/// never branched, until something else moved HEAD.
///
/// Targets are read directly -- no peel, no config lookup -- which is the same
/// cheap pass [`collect_refs`] already makes, not the per-ref ahead/behind work
/// the memo exists to avoid.
fn ref_tips_fingerprint(repo: &git2::Repository) -> u64 {
  let Ok(references) = repo.references() else {
    return 0;
  };
  let mut fingerprint: u64 = 0;
  for reference in references.flatten() {
    let Some(name) = reference.name() else { continue };
    if !(name.starts_with("refs/heads/") || name.starts_with("refs/remotes/")) {
      continue;
    }
    let Some(oid) = reference.target() else { continue };
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    oid.as_bytes().hash(&mut hasher);
    // Summed rather than folded in sequence: `references()` promises no
    // ordering, so a key that depended on one would churn for no reason.
    fingerprint = fingerprint.wrapping_add(hasher.finish());
  }
  fingerprint
}

/// The commit lane zero is reserved for. Normally HEAD itself, but when another
/// branch's tip sits directly above HEAD on the same first-parent line -- the
/// remote branch after a rewind, or a local branch that moved ahead -- reserve
/// that tip instead. The whole line then renders as one straight lane instead of
/// forking sideways at HEAD even though the history never actually branched.
fn primary_lane_oid(repo: &git2::Repository, head: Oid) -> Oid {
  let mut best: Option<(i64, Oid)> = None;
  let Ok(references) = repo.references() else {
    return head;
  };
  for reference in references.flatten() {
    let Some(name) = reference.name() else {
      continue;
    };
    if !(name.starts_with("refs/heads/") || name.starts_with("refs/remotes/")) {
      continue;
    }
    let Ok(tip) = reference.peel_to_commit() else {
      continue;
    };
    if tip.id() == head {
      continue;
    }
    // Cheap prefilter: the tip must be strictly ahead of HEAD with nothing
    // missing, or HEAD cannot be on its line at all.
    let Ok((ahead, behind)) = repo.graph_ahead_behind(tip.id(), head) else {
      continue;
    };
    if behind != 0 || ahead == 0 {
      continue;
    }
    // Confirm HEAD is on the tip's FIRST-parent chain. A branch that merely
    // contains HEAD through a merge still deserves its own lane. Every commit
    // on that chain down to HEAD is counted in `ahead`, so `ahead` steps is
    // always enough to reach it.
    let mut cursor = tip.clone();
    let mut on_line = false;
    for _ in 0..ahead {
      match cursor.parent(0) {
        Ok(parent) if parent.id() == head => {
          on_line = true;
          break;
        }
        Ok(parent) => cursor = parent,
        Err(_) => break,
      }
    }
    if !on_line {
      continue;
    }
    // Several tips can share the line below their fork point; the newest one
    // extends lane zero the furthest up before the others peel off.
    let time = tip.time().seconds();
    if best.map_or(true, |(t, _)| time > t) {
      best = Some((time, tip.id()));
    }
  }
  best.map_or(head, |(_, oid)| oid)
}

/// [`commit_change_stats`], memoized on the open repository.
///
/// A log page needs stats for every commit on it, and each one is a tree-to-tree
/// diff with rename detection -- by far the most expensive part of building the
/// page. Refreshing the graph after a rewind or a commit asks for the same
/// commits again, so without this the whole cost is paid over from scratch every
/// time and the graph visibly trails the rest of the UI.
///
/// A miss still computes and stores, so the first view of any commit costs the
/// same as before; only repeat views get cheaper.
pub(crate) fn cached_change_stats(
  open: &OpenRepo,
  repo: &git2::Repository,
  commit: &Commit<'_>,
) -> Result<(u32, u32, u32), git2::Error> {
  let oid = commit.id();
  if let Some(hit) = open.cached_stats(oid) {
    return Ok(hit);
  }
  let stats = commit_change_stats(repo, commit)?;
  open.store_stats(oid, stats);
  Ok(stats)
}

/// Summarize a commit against its first parent, matching the comparison used
/// by the commit-details view. Root commits compare against an empty tree.
pub(crate) fn commit_change_stats(
  repo: &git2::Repository,
  commit: &Commit<'_>,
) -> Result<(u32, u32, u32), git2::Error> {
  let tree = commit.tree()?;
  let parent_tree = commit.parent(0).ok().and_then(|parent| parent.tree().ok());
  let mut diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;

  // Treat a rename as one changed file instead of a delete plus an add.
  rename_detect::find_renames(&mut diff)?;

  let stats = diff.stats()?;
  Ok((
    stats.files_changed().min(u32::MAX as usize) as u32,
    stats.insertions().min(u32::MAX as usize) as u32,
    stats.deletions().min(u32::MAX as usize) as u32,
  ))
}

#[tauri::command]
#[specta::specta]
pub async fn get_log(
  manager: State<'_, RepoManager>,
  repo_id: String,
  skip: u32,
  limit: u32,
) -> Result<LogPage, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();

    let head_oid = repo
      .head()
      .ok()
      .and_then(|head| head.peel_to_commit().ok())
      .map(|commit| commit.id());

    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    for oid in collect_log_roots(&repo) {
      walk.push(oid)?;
    }

    let refs = collect_refs(&repo);
    // Memoized on HEAD plus the ref tips: this comparison runs over every ref
    // and dominates the cost of a page, but its answer is identical for every
    // page of the same scroll. See `OpenRepo::primary_lane`.
    let ref_tips = ref_tips_fingerprint(&repo);
    let mut lanes = head_oid
      .map(|head| {
        let primary = open.cached_primary_lane(head, ref_tips).unwrap_or_else(|| {
          let computed = primary_lane_oid(&repo, head);
          open.store_primary_lane(head, ref_tips, computed);
          computed
        });
        LaneState::with_primary(primary)
      })
      .unwrap_or_default();
    let mut commits = Vec::with_capacity(limit as usize);
    let mut has_more = false;
    let end = skip as usize + limit as usize;

    for (i, oid) in walk.flatten().enumerate() {
      if i >= end {
        has_more = true;
        break;
      }
      let commit = repo.find_commit(oid)?;
      let parents: Vec<Oid> = commit.parent_ids().collect();
      // Lane state must advance over skipped commits too, so pagination keeps
      // consistent lanes.
      let assignment = lanes.assign(oid, &parents);
      if i < skip as usize {
        continue;
      }

      let author = commit.author();
      let name = author.name().unwrap_or("unknown").to_string();
      // Deliberately cache-only: computing stats here would make the page cost
      // one rename-detected tree diff per commit, which on a repo with huge
      // commits takes seconds before anything renders. Rows that miss come back
      // as `None` and the frontend fills them in for the visible range only.
      let stats = open.cached_stats(oid);
      // The full message is already loaded here, so reading trailers costs
      // nothing extra -- no second walk and no per-commit shell-out.
      let message = commit.message().unwrap_or("");
      commits.push(CommitEntry {
        sha: oid.to_string(),
        short_sha: oid.to_string()[..7].to_string(),
        summary: commit.summary().unwrap_or("").to_string(),
        files_changed: stats.map(|s| s.0),
        additions: stats.map(|s| s.1),
        deletions: stats.map(|s| s.2),
        author_initials: initials(&name),
        author_email: author.email().unwrap_or("").to_string(),
        author_name: name,
        time: commit.time().seconds() as f64,
        lane: assignment.lane,
        parent_lanes: assignment.parent_lanes,
        parent_shas: parents.iter().map(|p| p.to_string()).collect(),
        is_merge: parents.len() > 1,
        refs: refs.get(&oid).cloned().unwrap_or_default(),
        spec_id: trailers::spec_id(message),
        assisted_by: trailers::assisted_by(message),
      });
    }

    Ok(LogPage { commits, has_more })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Diff stats for a set of commits, computed on demand.
///
/// The log page leaves stats out for any commit it has not already computed, so
/// the frontend asks for just the rows the user can actually see. Results are
/// memoized on the open repo, so scrolling back over the same commits is free
/// and a later `get_log` refresh returns them inline.
///
/// Unknown or unreadable shas are skipped rather than failing the batch: a row
/// that cannot be summarized should stay blank, not break the ones around it.
#[tauri::command]
#[specta::specta]
pub async fn get_commit_stats(
  manager: State<'_, RepoManager>,
  repo_id: String,
  shas: Vec<String>,
) -> Result<HashMap<String, CommitStats>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut out = HashMap::with_capacity(shas.len());
    for sha in shas {
      let Ok(oid) = Oid::from_str(&sha) else { continue };
      let Ok(commit) = repo.find_commit(oid) else {
        continue;
      };
      if let Ok((files_changed, additions, deletions)) = cached_change_stats(&open, &repo, &commit) {
        out.insert(
          sha,
          CommitStats {
            files_changed,
            additions,
            deletions,
          },
        );
      }
    }
    Ok(out)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
  use std::fs;

  use git2::{Repository, Signature, StashFlags};

  use super::*;

  fn commit_file(repo: &Repository, name: &str, contents: &str, message: &str) -> Oid {
    let workdir = repo.workdir().expect("workdir");
    fs::write(workdir.join(name), contents).expect("write fixture");
    let mut index = repo.index().expect("index");
    index
      .add_path(std::path::Path::new(name))
      .expect("add fixture");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("tree id");
    let tree = repo.find_tree(tree_id).expect("tree");
    let signature = Signature::now("Graph Test", "graph@example.com").expect("signature");
    let parents = repo
      .head()
      .ok()
      .and_then(|head| head.peel_to_commit().ok())
      .into_iter()
      .collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();
    repo
      .commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
      )
      .expect("commit")
  }

  /// The cache must be a pure speedup: a hit has to return exactly what a fresh
  /// computation would, and it must not bleed one commit's stats into another.
  #[test]
  fn cached_stats_match_a_fresh_computation_per_commit() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    // Two commits with different shapes, so a mixed-up key is visible.
    let first = commit_file(&repo, "a.txt", "one\n", "add a");
    let second = commit_file(&repo, "b.txt", "one\ntwo\nthree\n", "add b");

    let open = OpenRepo::for_test(repo);
    let repo = open.repo.lock().unwrap();

    for oid in [first, second] {
      let commit = repo.find_commit(oid).expect("commit");
      let expected = commit_change_stats(&repo, &commit).expect("uncached stats");

      // Cold: computes and stores.
      assert!(open.cached_stats(oid).is_none(), "cache should start empty");
      let cold = cached_change_stats(&open, &repo, &commit).expect("cold stats");
      assert_eq!(cold, expected, "a miss must match a fresh computation");

      // Warm: served from the cache, same answer.
      assert_eq!(open.cached_stats(oid), Some(expected), "miss must store");
      let warm = cached_change_stats(&open, &repo, &commit).expect("warm stats");
      assert_eq!(warm, expected, "a hit must match a fresh computation");
    }

    let a = open.cached_stats(first).expect("first cached");
    let b = open.cached_stats(second).expect("second cached");
    assert_ne!(a, b, "distinct commits must not share a cache entry");
  }

  #[test]
  fn detached_stash_base_is_a_log_root() {
    let dir = tempfile::tempdir().expect("temp repo");
    let mut repo = Repository::init(dir.path()).expect("repo");
    let main = commit_file(&repo, "base.txt", "base", "main");

    repo.set_head_detached(main).expect("detach HEAD");
    let detached = commit_file(&repo, "detached.txt", "detached", "detached work");
    fs::write(dir.path().join("stash.txt"), "saved work").expect("stash fixture");
    let signature = Signature::now("Graph Test", "graph@example.com").expect("signature");
    repo
      .stash_save(
        &signature,
        "detached stash",
        Some(StashFlags::INCLUDE_UNTRACKED),
      )
      .expect("stash");

    repo.set_head("refs/heads/master").expect("return to main");
    repo
      .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
      .expect("checkout main");

    let roots = collect_log_roots(&repo);
    assert!(
      roots.contains(&detached),
      "detached stash base must remain visible"
    );

    let mut walk = repo.revwalk().expect("revwalk");
    walk
      .set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
      .expect("sorting");
    for oid in roots {
      walk.push(oid).expect("push root");
    }
    let commits = walk.flatten().collect::<Vec<_>>();
    assert!(
      commits.contains(&detached),
      "detached stash base must be walked"
    );
  }

  #[test]
  fn rewound_head_hands_lane_zero_to_the_tip_above_it() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let base = commit_file(&repo, "a.txt", "a", "base");
    let newer = commit_file(&repo, "b.txt", "b", "newer");

    // The remote stays at the newer tip while the local branch rewinds under it.
    repo
      .reference("refs/remotes/origin/master", newer, true, "test remote")
      .expect("remote ref");
    let base_commit = repo.find_commit(base).expect("base commit");
    repo
      .reset(base_commit.as_object(), git2::ResetType::Hard, None)
      .expect("rewind");

    assert_eq!(
      primary_lane_oid(&repo, base),
      newer,
      "the straight line through HEAD must keep lane zero"
    );
  }

  /// A fetch moves a remote ref without moving HEAD. The memoized primary lane
  /// has to notice, or lane zero stays reserved for the pre-fetch tip and every
  /// commit the fetch brought in renders as a side lane forking off a history
  /// that never branched.
  #[test]
  fn fetching_new_commits_invalidates_the_primary_lane_memo() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let base = commit_file(&repo, "a.txt", "a", "base");
    let fetched = commit_file(&repo, "b.txt", "b", "newer");

    // Pre-fetch: HEAD and the remote both sit at `base`; `fetched` exists in the
    // object store but no ref points at it yet.
    repo
      .reference("refs/remotes/origin/master", base, true, "test remote")
      .expect("remote ref");
    {
      let base_commit = repo.find_commit(base).expect("base commit");
      repo
        .reset(base_commit.as_object(), git2::ResetType::Hard, None)
        .expect("rewind to base");
    }

    let open = OpenRepo::for_test(repo);
    let repo = open.repo.lock().unwrap();

    let before = ref_tips_fingerprint(&repo);
    let primary = primary_lane_oid(&repo, base);
    assert_eq!(primary, base, "nothing is ahead of HEAD yet");
    open.store_primary_lane(base, before, primary);
    assert_eq!(
      open.cached_primary_lane(base, before),
      Some(base),
      "an unchanged repo must still hit the memo"
    );

    // The fetch: the remote ref moves ahead, HEAD does not.
    repo
      .reference("refs/remotes/origin/master", fetched, true, "fetched")
      .expect("move remote ref");

    let after = ref_tips_fingerprint(&repo);
    assert_ne!(before, after, "a moved ref must change the fingerprint");
    assert_eq!(
      open.cached_primary_lane(base, after),
      None,
      "the pre-fetch answer must not be served after the fetch"
    );
    assert_eq!(
      primary_lane_oid(&repo, base),
      fetched,
      "lane zero now belongs to the tip the fetch brought in"
    );
  }

  /// Deleting a ref changes nothing about where the survivors point, so a
  /// fingerprint built by combining tips must still notice it.
  #[test]
  fn ref_tips_fingerprint_notices_an_added_or_deleted_ref() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let tip = commit_file(&repo, "a.txt", "a", "base");

    let bare = ref_tips_fingerprint(&repo);
    repo
      .reference("refs/heads/side", tip, true, "side branch")
      .expect("side ref");
    let with_side = ref_tips_fingerprint(&repo);
    assert_ne!(bare, with_side, "a new ref at a known commit still counts");

    repo
      .find_reference("refs/heads/side")
      .expect("side ref")
      .delete()
      .expect("delete side ref");
    assert_eq!(
      ref_tips_fingerprint(&repo),
      bare,
      "removing the ref returns to the earlier fingerprint"
    );
  }

  /// Row labels come from the raw references rather than the branch helper, so
  /// the classification each ref gets is pinned down here: the checked-out
  /// branch is `Head`, other locals are `Branch`, remotes are `Remote`, and the
  /// symbolic `origin/HEAD` pointer is not a label at all.
  #[test]
  fn collect_refs_labels_head_branches_and_remotes() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let tip = commit_file(&repo, "a.txt", "a", "base");

    repo
      .reference("refs/heads/side", tip, true, "side branch")
      .expect("side ref");
    repo
      .reference("refs/remotes/origin/master", tip, true, "remote ref")
      .expect("remote ref");
    // Symbolic default-branch pointer; must not become its own label.
    repo
      .reference_symbolic(
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/master",
        true,
        "remote head",
      )
      .expect("remote head");

    let map = collect_refs(&repo);
    let labels = map.get(&tip).expect("labels on the tip");

    let kind_of = |name: &str| {
      labels
        .iter()
        .find(|r| r.name == name)
        .map(|r| r.ref_type)
        .unwrap_or_else(|| panic!("missing label {name}"))
    };

    assert!(matches!(kind_of("master"), RefKind::Head), "checked-out branch");
    assert!(matches!(kind_of("side"), RefKind::Branch), "other local");
    assert!(matches!(kind_of("origin/master"), RefKind::Remote), "remote");
    assert!(
      !labels.iter().any(|r| r.name.ends_with("/HEAD")),
      "origin/HEAD is a pointer at another ref, not a label of its own"
    );
  }

  /// With HEAD detached no branch is checked out, so nothing may claim `Head`.
  #[test]
  fn collect_refs_marks_no_head_when_detached() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let tip = commit_file(&repo, "a.txt", "a", "base");
    repo.set_head_detached(tip).expect("detach");

    let map = collect_refs(&repo);
    let labels = map.get(&tip).expect("labels on the tip");
    assert!(
      !labels.iter().any(|r| matches!(r.ref_type, RefKind::Head)),
      "a detached HEAD is on no branch, so no branch is the head branch"
    );
  }

  /// The cache must be a pure speedup: same answer as computing it fresh, and
  /// keyed tightly enough that moving HEAD does not serve a stale lane.
  #[test]
  fn primary_lane_cache_matches_a_fresh_computation_and_follows_head() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let base = commit_file(&repo, "a.txt", "a", "base");
    let newer = commit_file(&repo, "b.txt", "b", "newer");
    repo
      .reference("refs/remotes/origin/master", newer, true, "test remote")
      .expect("remote ref");

    let open = OpenRepo::for_test(repo);
    let repo = open.repo.lock().unwrap();

    let tips = ref_tips_fingerprint(&repo);

    // Cold: nothing cached for this HEAD yet.
    assert!(open.cached_primary_lane(newer, tips).is_none(), "starts empty");
    let fresh = primary_lane_oid(&repo, newer);
    open.store_primary_lane(newer, tips, fresh);
    assert_eq!(
      open.cached_primary_lane(newer, tips),
      Some(fresh),
      "a stored entry must be served back for the same HEAD and ref tips"
    );

    // A different HEAD must miss rather than reuse the previous answer.
    assert!(
      open.cached_primary_lane(base, tips).is_none(),
      "moving HEAD must not serve the lane computed for the old HEAD"
    );
  }

  #[test]
  fn diverged_tip_does_not_take_the_head_lane() {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    let base = commit_file(&repo, "a.txt", "a", "base");
    let side = commit_file(&repo, "b.txt", "b", "side work");
    repo
      .reference("refs/heads/side", side, true, "test branch")
      .expect("side ref");

    let base_commit = repo.find_commit(base).expect("base commit");
    repo
      .reset(base_commit.as_object(), git2::ResetType::Hard, None)
      .expect("rewind");
    let head = commit_file(&repo, "c.txt", "c", "diverged");

    assert_eq!(
      primary_lane_oid(&repo, head),
      head,
      "a diverged branch must keep its own lane"
    );
  }
}
