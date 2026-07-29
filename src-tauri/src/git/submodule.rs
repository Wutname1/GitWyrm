//! Submodule inspection helpers.
//!
//! A submodule is a pinned pointer: the parent repo records the exact commit a
//! nested repo should sit at. When the nested checkout sits at a different
//! commit, the parent shows the submodule path as "modified" -- but ordinary
//! file operations (stash, discard-via-checkout) can't touch it, which is why a
//! moved submodule leaves the user stuck. These helpers surface what actually
//! moved so the UI can explain it and offer submodule-specific actions.

use std::collections::HashMap;

use crate::git::types::SubmoduleMove;

/// A path -> pointer-move map for every submodule whose workdir HEAD differs
/// from the commit the parent repo records. Paths not present are in sync (or
/// not submodules). Uninitialized submodules are included with `initialized:
/// false` and no workdir sha.
pub fn moved_submodules(repo: &git2::Repository) -> HashMap<String, SubmoduleMove> {
  let mut moves = HashMap::new();

  let Ok(subs) = repo.submodules() else {
    return moves;
  };

  for sub in subs {
    let Some(path) = sub.path().to_str().map(str::to_string) else {
      continue;
    };

    // The commit the parent repo pins (from its index/HEAD).
    let recorded = sub.index_id().or_else(|| sub.head_id());
    // The commit the nested checkout currently sits at.
    let checked_out = sub.workdir_id();

    match (recorded, checked_out) {
      (Some(recorded), Some(checked_out)) if recorded != checked_out => {
        // The submodule's commits live in the submodule's own object database,
        // not the parent's -- so ahead/behind must be computed against the
        // nested repo. If it can't be opened, fall back to unknown (0/0).
        let (ahead, behind) = sub
          .open()
          .ok()
          .and_then(|nested| {
            nested
              .graph_ahead_behind(checked_out, recorded)
              .ok()
              .map(|(a, b)| (a as u32, b as u32))
          })
          .unwrap_or((0, 0));
        moves.insert(
          path.clone(),
          SubmoduleMove {
            path,
            recorded_sha: recorded.to_string(),
            workdir_sha: Some(checked_out.to_string()),
            ahead,
            behind,
            initialized: true,
          },
        );
      }
      // Recorded but not checked out anywhere: the submodule isn't initialized.
      (Some(recorded), None) => {
        moves.insert(
          path.clone(),
          SubmoduleMove {
            path,
            recorded_sha: recorded.to_string(),
            workdir_sha: None,
            ahead: 0,
            behind: 0,
            initialized: false,
          },
        );
      }
      _ => {}
    }
  }

  moves
}

/// True when `path` names a submodule in this repo (moved or not).
pub fn is_submodule(repo: &git2::Repository, path: &str) -> bool {
  repo.find_submodule(path).is_ok()
}

/// Check out every initialized submodule at the commit the parent now records.
///
/// Operations that write a tree -- cherry-pick, revert, merge -- move the
/// gitlink in the index but leave the nested checkout where it was. The commit
/// then lands correctly while the working tree reports the submodule as
/// modified, so the repo is dirty the instant the operation "succeeds" and the
/// next operation refuses to start. Running this afterwards makes the checkout
/// match what was just committed.
///
/// Uninitialized submodules are skipped: there is no checkout to move, and
/// downloading one is a separate, explicit user action. Failures are ignored
/// per-submodule -- a submodule whose commit is missing locally must not fail
/// the operation that already committed successfully.
pub fn sync_submodule_workdirs(repo: &git2::Repository) {
  let Ok(subs) = repo.submodules() else {
    return;
  };

  for mut sub in subs {
    // No workdir id means it was never checked out; leave it alone.
    if sub.workdir_id().is_none() {
      continue;
    }
    // Re-read so index_id reflects the tree just written.
    let _ = sub.reload(true);
    let (Some(recorded), Some(checked_out)) = (sub.index_id(), sub.workdir_id()) else {
      continue;
    };
    if recorded == checked_out {
      continue;
    }
    let _ = sub.update(false, None);
  }
}
