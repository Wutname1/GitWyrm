use std::collections::{HashMap, HashSet};

use git2::{Commit, DiffFindOptions, Oid, Sort};
use tauri::State;

use crate::error::AppError;
use crate::git::graph::{initials, LaneState};
use crate::git::refs;
use crate::git::types::{CommitEntry, LogPage, RefInfo, RefKind};
use crate::state::RepoManager;

fn collect_refs(repo: &git2::Repository) -> HashMap<Oid, Vec<RefInfo>> {
  let mut map: HashMap<Oid, Vec<RefInfo>> = HashMap::new();

  for rec in refs::walk_branches(repo).unwrap_or_default() {
    let Some(oid) = rec.tip else { continue };
    let ref_type = match (rec.is_remote, rec.is_head) {
      (true, _) => RefKind::Remote,
      (false, true) => RefKind::Head,
      (false, false) => RefKind::Branch,
    };
    map.entry(oid).or_default().push(RefInfo {
      name: rec.name,
      ref_type,
    });
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
  let mut find = DiffFindOptions::new();
  find.renames(true);
  diff.find_similar(Some(&mut find))?;

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
    let mut lanes = head_oid.map(LaneState::with_primary).unwrap_or_default();
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
      let (files_changed, additions, deletions) = commit_change_stats(&repo, &commit)?;
      commits.push(CommitEntry {
        sha: oid.to_string(),
        short_sha: oid.to_string()[..7].to_string(),
        summary: commit.summary().unwrap_or("").to_string(),
        files_changed,
        additions,
        deletions,
        author_initials: initials(&name),
        author_email: author.email().unwrap_or("").to_string(),
        author_name: name,
        time: commit.time().seconds() as f64,
        lane: assignment.lane,
        parent_lanes: assignment.parent_lanes,
        parent_shas: parents.iter().map(|p| p.to_string()).collect(),
        is_merge: parents.len() > 1,
        refs: refs.get(&oid).cloned().unwrap_or_default(),
      });
    }

    Ok(LogPage { commits, has_more })
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
}
