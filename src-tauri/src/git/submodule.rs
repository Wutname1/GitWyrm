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
        let (ahead, behind) = repo
          .graph_ahead_behind(checked_out, recorded)
          .map(|(a, b)| (a as u32, b as u32))
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
