use std::collections::HashMap;

use git2::{Oid, Sort};
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
    map.entry(oid).or_default().push(RefInfo { name: rec.name, ref_type });
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
    map.entry(target).or_default().push(RefInfo { name: short, ref_type: RefKind::Tag });
    true
  });
  map
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

    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    // Push every branch tip, local and remote, so side branches and remote-only
    // work both appear in the graph. Remote tips were previously omitted, which
    // hid any branch that had not been checked out locally.
    walk.push_head().ok();
    for rec in refs::walk_branches(&repo).unwrap_or_default() {
      if let Some(oid) = rec.tip {
        walk.push(oid).ok();
      }
    }

    let refs = collect_refs(&repo);
    let mut lanes = LaneState::default();
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
      commits.push(CommitEntry {
        sha: oid.to_string(),
        short_sha: oid.to_string()[..7].to_string(),
        summary: commit.summary().unwrap_or("").to_string(),
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
