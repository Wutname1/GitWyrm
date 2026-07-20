use std::collections::HashMap;

use git2::{BranchType, Oid, Repository};

use crate::error::AppError;

/// One branch as read from the odb, before it is projected into whichever shape
/// a given command needs. Gathered by [`walk_branches`] so the `/HEAD` skip and
/// the tip/time lookups live in exactly one place.
#[derive(Debug, Clone)]
pub struct RefRecord {
  /// Full name as git reports it: `main` for locals, `origin/main` for remotes.
  pub name: String,
  pub is_remote: bool,
  pub is_head: bool,
  pub tip: Option<Oid>,
  /// Commit time of the tip, seconds since epoch. None when the tip is missing
  /// or unreadable (a packed ref pointing at a pruned object).
  pub time: Option<i64>,
  /// Configured upstream, full remote name (`origin/main`). Locals only.
  pub upstream: Option<String>,
}

impl RefRecord {
  /// Short name with the `<remote>/` prefix removed. Locals are unchanged.
  pub fn short_name(&self) -> &str {
    if self.is_remote {
      self.name.split_once('/').map_or(self.name.as_str(), |(_, rest)| rest)
    } else {
      &self.name
    }
  }

  /// The remote this branch lives under, or None for locals.
  pub fn remote_name(&self) -> Option<&str> {
    if self.is_remote {
      self.name.split_once('/').map(|(remote, _)| remote)
    } else {
      None
    }
  }
}

/// Walk every local and remote-tracking branch once.
///
/// Skips the symbolic `<remote>/HEAD` ref, which is a pointer at another branch
/// rather than a branch of its own and would otherwise show up as a duplicate.
pub fn walk_branches(repo: &Repository) -> Result<Vec<RefRecord>, AppError> {
  let head = repo.head().ok();
  let head_oid = head.as_ref().and_then(|h| h.target());
  let head_name = head.as_ref().and_then(|h| h.shorthand()).map(str::to_string);

  let mut out = Vec::new();

  for (kind, is_remote) in [(BranchType::Local, false), (BranchType::Remote, true)] {
    for (branch, _) in repo.branches(Some(kind))?.flatten() {
      let Some(name) = branch.name().ok().flatten().map(str::to_string) else { continue };
      if is_remote && name.ends_with("/HEAD") {
        continue;
      }

      let tip = branch.get().target();
      let time = tip
        .and_then(|oid| repo.find_commit(oid).ok())
        .map(|c| c.time().seconds());

      // `is_head` is only meaningful for locals; a remote-tracking ref is never
      // what HEAD points at even when it shares a tip with the checked-out branch.
      let is_head = !is_remote
        && (branch.is_head() || (tip.is_some() && tip == head_oid && Some(&name) == head_name.as_ref()));

      let upstream = if is_remote {
        None
      } else {
        branch.upstream().ok().and_then(|u| u.name().ok().flatten().map(str::to_string))
      };

      out.push(RefRecord { name, is_remote, is_head, tip, time, upstream });
    }
  }

  Ok(out)
}

/// Ahead/behind of `ours` relative to `theirs`, as `(ahead, behind)`.
/// None when either tip is missing or the graph walk fails.
pub fn ahead_behind(repo: &Repository, ours: Oid, theirs: Oid) -> Option<(u32, u32)> {
  repo.graph_ahead_behind(ours, theirs).ok().map(|(a, b)| (a as u32, b as u32))
}

/// Index of remote-branch full name -> tip, for resolving upstreams without
/// re-walking. Built from the same records so it cannot drift.
pub fn remote_tips(records: &[RefRecord]) -> HashMap<&str, Oid> {
  records
    .iter()
    .filter(|r| r.is_remote)
    .filter_map(|r| r.tip.map(|oid| (r.name.as_str(), oid)))
    .collect()
}

/// Index of local-branch short name -> tip. Used to answer "does any local
/// branch have this remote branch's work?"
pub fn local_tips(records: &[RefRecord]) -> HashMap<&str, Oid> {
  records
    .iter()
    .filter(|r| !r.is_remote)
    .filter_map(|r| r.tip.map(|oid| (r.name.as_str(), oid)))
    .collect()
}

/// Whether the working tree has changes to tracked files. Untracked files are
/// ignored, so an operation that only rewrites tracked content is not blocked
/// by a stray build artifact.
///
/// Use [`any_changes_present`] instead for anything that checks out a
/// different tree: checkout can overwrite an untracked file that the target
/// tree also contains, so those paths must count untracked files as dirty.
pub fn tracked_changes_present(repo: &Repository) -> Result<bool, AppError> {
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(false);
  Ok(repo.statuses(Some(&mut opts))?.iter().any(|e| !e.status().is_ignored()))
}

/// Whether the working tree has any non-ignored change at all, untracked files
/// included. The stricter check, used by the checkout paths.
pub fn any_changes_present(repo: &Repository) -> Result<bool, AppError> {
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(true).recurse_untracked_dirs(true);
  Ok(repo.statuses(Some(&mut opts))?.iter().any(|e| !e.status().is_ignored()))
}

/// Paths currently conflicted in the index, sorted and deduplicated.
///
/// A conflict records up to three sides; the path is the same for all of them
/// except in a rename conflict, so read whichever side is present, preferring
/// ours.
pub fn conflicted_paths(repo: &Repository) -> Result<Vec<String>, AppError> {
  let index = repo.index()?;
  if !index.has_conflicts() {
    return Ok(Vec::new());
  }
  let mut paths = Vec::new();
  for entry in index.conflicts()? {
    let entry = entry?;
    let raw = entry
      .our
      .as_ref()
      .or(entry.their.as_ref())
      .or(entry.ancestor.as_ref())
      .map(|e| e.path.clone());
    if let Some(bytes) = raw {
      paths.push(String::from_utf8_lossy(&bytes).into_owned());
    }
  }
  paths.sort();
  paths.dedup();
  Ok(paths)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  /// Scratch repo with one commit on `main` and a fake `origin/<branch>` ref
  /// created directly under refs/remotes, mimicking a fetched remote branch.
  struct Scratch {
    dir: PathBuf,
    repo: Repository,
  }

  impl Drop for Scratch {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.dir);
    }
  }

  fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("gitwyrm-refs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let repo = Repository::init(&dir).unwrap();
    {
      let sig = git2::Signature::now("T", "t@e").unwrap();
      let tree = {
        let mut idx = repo.index().unwrap();
        let id = idx.write_tree().unwrap();
        repo.find_tree(id).unwrap()
      };
      repo.commit(Some("HEAD"), &sig, &sig, "root", &tree, &[]).unwrap();
    }
    Scratch { dir, repo }
  }

  /// Add a commit on top of `parent_ref`, returning the new oid.
  fn commit_onto(repo: &Repository, parent_ref: &str, msg: &str) -> Oid {
    let sig = git2::Signature::now("T", "t@e").unwrap();
    let parent = repo.find_reference(parent_ref).unwrap().peel_to_commit().unwrap();
    let tree = parent.tree().unwrap();
    repo.commit(None, &sig, &sig, msg, &tree, &[&parent]).unwrap()
  }

  #[test]
  fn skips_symbolic_remote_head() {
    let s = scratch("head");
    let tip = s.repo.head().unwrap().target().unwrap();
    s.repo.reference("refs/remotes/origin/main", tip, true, "").unwrap();
    s.repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/remotes/origin/main", true, "").unwrap();

    let recs = walk_branches(&s.repo).unwrap();
    assert!(recs.iter().any(|r| r.name == "origin/main"));
    assert!(
      !recs.iter().any(|r| r.name.ends_with("/HEAD")),
      "symbolic HEAD must not appear as a branch"
    );
  }

  #[test]
  fn remote_only_branch_is_enumerated_with_a_tip() {
    let s = scratch("remoteonly");
    let ahead = commit_onto(&s.repo, "HEAD", "teammate work");
    // A branch that exists only on the remote - no local counterpart at all.
    s.repo.reference("refs/remotes/origin/claude/feature", ahead, true, "").unwrap();

    let recs = walk_branches(&s.repo).unwrap();
    let rec = recs
      .iter()
      .find(|r| r.name == "origin/claude/feature")
      .expect("remote-only branch must be enumerated");

    assert!(rec.is_remote);
    assert!(!rec.is_head);
    assert_eq!(rec.tip, Some(ahead));
    assert!(rec.time.is_some(), "tip commit time must resolve");
    assert_eq!(rec.short_name(), "claude/feature", "remote prefix is stripped once");
    assert_eq!(rec.remote_name(), Some("origin"));
  }

  #[test]
  fn remote_ahead_of_head_reports_positive_ahead() {
    let s = scratch("ahead");
    let head = s.repo.head().unwrap().target().unwrap();
    let ahead = commit_onto(&s.repo, "HEAD", "one ahead");
    s.repo.reference("refs/remotes/origin/feature", ahead, true, "").unwrap();

    // This is the case the sidebar was silently hiding: work on the remote that
    // the local repo does not have.
    let (a, b) = ahead_behind(&s.repo, ahead, head).unwrap();
    assert_eq!((a, b), (1, 0));
  }

  #[test]
  fn short_name_keeps_nested_paths_intact() {
    let s = scratch("nested");
    let rec = RefRecord {
      name: "origin/dependabot/npm/foo".into(),
      is_remote: true,
      is_head: false,
      tip: None,
      time: None,
      upstream: None,
    };
    // Only the leading remote segment comes off; the folder path survives.
    assert_eq!(rec.short_name(), "dependabot/npm/foo");
    drop(s);
  }

  #[test]
  fn local_branch_is_flagged_as_head() {
    let s = scratch("localhead");
    let recs = walk_branches(&s.repo).unwrap();
    let head = recs.iter().find(|r| !r.is_remote).expect("local branch exists");
    assert!(head.is_head, "checked-out branch must be marked as HEAD");
    assert_eq!(head.short_name(), head.name, "locals are not prefix-stripped");
  }
}
