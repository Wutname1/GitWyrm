//! The practice repository behind the hands-on tutorial.
//!
//! Everything here lives under the app's own data directory and is thrown away
//! when the tutorial ends, so a beginner can drag branches around, reset things
//! and generally make a mess without any chance of touching real work. That
//! safety is the whole point: the lessons ask people to try gestures they have
//! never seen, and they will only do that if a mistake cannot cost them
//! anything.
//!
//! The repo is seeded with a deliberately awkward shape -- a branch that has
//! moved on while `main` also moved on, plus a local "origin" that is behind
//! both. A tidy repo would make the sync lesson meaningless, because dragging
//! one branch onto another would have nothing to offer.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::error::AppError;

/// Where the practice repo and its stand-in remote live. Both sit inside the
/// app's data directory rather than the user's code folder: this is our file,
/// not theirs, and it should not show up in their projects list or their
/// backups.
fn tutorial_root(app: &AppHandle) -> Result<PathBuf, AppError> {
  let dir = app
    .path()
    .app_data_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  Ok(dir.join("tutorial"))
}

#[derive(Debug, Serialize, Type)]
pub struct TutorialRepo {
  /// Working directory to open as a tab.
  pub path: String,
  /// The branch that has diverged from `main`, so the UI can point lessons at
  /// it by name instead of guessing.
  pub feature_branch: String,
}

/// The starting file, written at the first commit and edited later so the
/// history has something readable in its diffs. Plain prose on purpose -- a
/// beginner reading a diff should not also be parsing code.
const NOTES_FIRST: &str = "\
# Trail notes

Day one. Found the cave mouth just past the river.
It is bigger than the map said.
";

const NOTES_SECOND: &str = "\
# Trail notes

Day one. Found the cave mouth just past the river.
It is bigger than the map said.

Day two. Went in about thirty paces. Cold air coming up.
Turned back to fetch a better lamp.
";

const NOTES_THIRD: &str = "\
# Trail notes

Day one. Found the cave mouth just past the river.
It is bigger than the map said.

Day two. Went in about thirty paces. Cold air coming up.
Turned back to fetch a better lamp.

Day three. The passage opens out. Something is down there.
";

/// The edit left sitting in the working tree, unstaged, so the staging and
/// line-staging lessons have real changes to work with the moment the tutorial
/// starts. Two separate edits in one file is what makes "stage only these
/// lines" worth teaching.
const NOTES_WORKING: &str = "\
# Trail notes

Day one. Found the cave mouth just past the river.
It is bigger than the map said. Bring rope next time.

Day two. Went in about thirty paces. Cold air coming up.
Turned back to fetch a better lamp.

Day three. The passage opens out. Something is down there.

Day four. It has scales.
";

const SUPPLIES: &str = "\
# Supplies

- lamp oil
- rope
- chalk for marking turns
";

const SUPPLIES_FEATURE: &str = "\
# Supplies

- lamp oil
- rope
- chalk for marking turns
- a much longer rope
- something to write on
";

/// Who to credit for the practice commits.
///
/// Falls back to a neutral author when git has no identity yet. The tutorial is
/// offered at the end of onboarding, which is exactly where someone may have
/// skipped the identity step -- refusing to build the practice repo over a
/// missing name would punish them for that, and these commits are throwaway
/// anyway.
fn tutorial_signature() -> Result<git2::Signature<'static>, AppError> {
  if let Ok(config) = git2::Config::open_default() {
    let name = config.get_string("user.name").unwrap_or_default();
    let email = config.get_string("user.email").unwrap_or_default();
    if !name.trim().is_empty() && !email.trim().is_empty() {
      return Ok(git2::Signature::now(name.trim(), email.trim())?);
    }
  }
  Ok(git2::Signature::now("GitWyrm Tutorial", "tutorial@gitwyrm.local")?)
}

/// Write a file, stage everything, and commit on the current HEAD.
fn commit_files(
  repo: &git2::Repository,
  signature: &git2::Signature,
  message: &str,
  files: &[(&str, &str)],
) -> Result<git2::Oid, AppError> {
  let workdir = repo
    .workdir()
    .ok_or_else(|| AppError::Other("tutorial repo has no working directory".into()))?
    .to_path_buf();

  for (name, contents) in files {
    std::fs::write(workdir.join(name), contents)?;
  }

  let mut index = repo.index()?;
  index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
  index.write()?;
  let tree_oid = index.write_tree()?;
  let tree = repo.find_tree(tree_oid)?;

  // First commit on a fresh repo has no parent; everything after builds on
  // whatever HEAD points at.
  let parents = match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
    Some(parent) => vec![parent],
    None => Vec::new(),
  };
  let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

  let oid = repo.commit(
    Some("HEAD"),
    signature,
    signature,
    message,
    &tree,
    &parent_refs,
  )?;
  Ok(oid)
}

/// Build the practice repository from scratch.
///
/// Always starts clean: a half-finished repo from an interrupted run would put
/// the lessons in a state they do not expect, and there is nothing here worth
/// preserving between runs.
#[tauri::command]
#[specta::specta]
pub async fn create_tutorial_repo(app: AppHandle) -> Result<TutorialRepo, AppError> {
  let root = tutorial_root(&app)?;

  tauri::async_runtime::spawn_blocking(move || {
    if root.exists() {
      std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(&root)?;

    let work = root.join("practice-repo");
    let remote = root.join("origin.git");
    std::fs::create_dir_all(&work)?;

    let signature = tutorial_signature()?;

    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head("main");
    let repo = git2::Repository::init_opts(&work, &options)?;

    // Three commits on main, then a stand-in remote cloned from that point.
    // Cloning here rather than at the end is what leaves origin behind the
    // local branches, which is what gives the sync lesson something to do.
    commit_files(
      &repo,
      &signature,
      "Start the trail notes",
      &[("notes.md", NOTES_FIRST), ("supplies.md", SUPPLIES)],
    )?;
    commit_files(&repo, &signature, "Add day two", &[("notes.md", NOTES_SECOND)])?;
    let shared_tip = commit_files(
      &repo,
      &signature,
      "Add day three",
      &[("notes.md", NOTES_THIRD)],
    )?;

    // A bare repo on disk is a perfectly real remote as far as git is
    // concerned, so push/pull/ahead/behind all behave exactly as they would
    // against a hosted one -- with no network and no account.
    git2::Repository::init_bare(&remote)?;
    let remote_url = remote.to_string_lossy().replace('\\', "/");
    repo.remote("origin", &remote_url)?;

    {
      let mut origin = repo.find_remote("origin")?;
      origin.push(&["refs/heads/main:refs/heads/main"], None)?;
    }
    // Record the tracking ref so the UI shows origin/main sitting where it is,
    // and set upstream so main knows what it is tracking.
    repo.reference(
      "refs/remotes/origin/main",
      shared_tip,
      true,
      "tutorial origin",
    )?;
    {
      let mut main = repo.find_branch("main", git2::BranchType::Local)?;
      main.set_upstream(Some("origin/main"))?;
    }

    // The feature branch moves on from the shared tip...
    let feature_branch = "feature/lamp-upgrade";
    {
      let tip = repo.find_commit(shared_tip)?;
      repo.branch(feature_branch, &tip, false)?;
    }
    repo.set_head(&format!("refs/heads/{feature_branch}"))?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
    commit_files(
      &repo,
      &signature,
      "Pack a longer rope",
      &[("supplies.md", SUPPLIES_FEATURE)],
    )?;

    // ...and main moves on too, so the two have genuinely diverged. Dragging
    // one onto the other now has a real answer, which is the whole lesson.
    repo.set_head("refs/heads/main")?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
    commit_files(
      &repo,
      &signature,
      "Note the cold air",
      &[("supplies.md", SUPPLIES)],
    )?;

    // Leave an uncommitted edit behind for the staging lessons. Written last so
    // nothing above sweeps it into a commit.
    std::fs::write(work.join("notes.md"), NOTES_WORKING)?;

    Ok(TutorialRepo {
      path: work.to_string_lossy().into_owned(),
      feature_branch: feature_branch.to_string(),
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete the practice repository.
///
/// Best-effort on purpose: the caller has already closed the tab, and a file
/// still held open by a watcher should not surface an error for a folder the
/// user never knew existed. A later run recreates it from scratch anyway.
#[tauri::command]
#[specta::specta]
pub async fn discard_tutorial_repo(app: AppHandle) -> Result<(), AppError> {
  let root = tutorial_root(&app)?;
  tauri::async_runtime::spawn_blocking(move || {
    if root.exists() {
      if let Err(error) = std::fs::remove_dir_all(&root) {
        log::warn!("could not remove tutorial repo: {error}");
      }
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// True when the given path is inside the tutorial folder.
///
/// Lets the UI recognise the practice repo it is looking at without threading
/// the path through every layer, which matters for the "this is practice" badge
/// and for refusing to treat it as a real recent repo.
pub fn is_tutorial_path(app: &AppHandle, path: &Path) -> bool {
  let Ok(root) = tutorial_root(app) else {
    return false;
  };
  path.starts_with(root)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The shape the lessons depend on: diverged branches and a remote that is
  /// behind. If this drifts, the sync lesson silently stops teaching anything,
  /// so it is worth asserting rather than eyeballing in the UI.
  #[test]
  fn seeded_repo_has_diverged_branches_and_dirty_tree() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("practice-repo");
    std::fs::create_dir_all(&work).unwrap();

    let signature =
      git2::Signature::now("Tutorial Test", "tutorial@example.com").unwrap();
    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head("main");
    let repo = git2::Repository::init_opts(&work, &options).unwrap();

    commit_files(
      &repo,
      &signature,
      "Start the trail notes",
      &[("notes.md", NOTES_FIRST), ("supplies.md", SUPPLIES)],
    )
    .unwrap();
    let shared_tip =
      commit_files(&repo, &signature, "Add day three", &[("notes.md", NOTES_THIRD)])
        .unwrap();

    let feature = "feature/lamp-upgrade";
    let tip = repo.find_commit(shared_tip).unwrap();
    repo.branch(feature, &tip, false).unwrap();
    repo.set_head(&format!("refs/heads/{feature}")).unwrap();
    repo
      .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
      .unwrap();
    let feature_tip = commit_files(
      &repo,
      &signature,
      "Pack a longer rope",
      &[("supplies.md", SUPPLIES_FEATURE)],
    )
    .unwrap();

    repo.set_head("refs/heads/main").unwrap();
    repo
      .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
      .unwrap();
    let main_tip = commit_files(
      &repo,
      &signature,
      "Note the cold air",
      &[("supplies.md", SUPPLIES)],
    )
    .unwrap();

    // Neither branch contains the other: that is what "diverged" means, and it
    // is what makes the drag-to-sync lesson show a real choice.
    let (ahead, behind) = repo.graph_ahead_behind(main_tip, feature_tip).unwrap();
    assert_eq!(ahead, 1, "main should have one commit feature lacks");
    assert_eq!(behind, 1, "feature should have one commit main lacks");

    std::fs::write(work.join("notes.md"), NOTES_WORKING).unwrap();
    let statuses = repo.statuses(None).unwrap();
    assert!(
      statuses.iter().any(|s| s.status().is_wt_modified()),
      "the working tree should have an unstaged edit to practice staging on"
    );
  }
}
