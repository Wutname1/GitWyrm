//! Repositories whose folder has gone away.
//!
//! When a repository's folder disappears GitWyrm does not forget it right away:
//! a folder can come back from a delete-and-re-clone or an unplugged drive, and
//! the user's tab name, pins and group membership should survive that. So each
//! missing folder gets a note of when it was first noticed, and only after a
//! week of staying gone are the repository's settings dropped.
//!
//! These notes live in their own file, written only here. They are bookkeeping,
//! not preferences: the user never sets them and the UI never reads them. Keeping
//! them out of `settings.json` means the frontend's saves (which rewrite that
//! whole file from its own snapshot) can never erase a note, and this module can
//! never clobber a preference. No merging, no ordering, no shared writer.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::error::AppError;
use crate::settings::{self, normalize_repo_path};

/// How long a repository's folder may stay missing before GitWyrm forgets the
/// repository's settings. Long enough to cover a delete-and-re-clone or a
/// vacation with an external drive unplugged.
pub const GRACE_SECS: f64 = 7.0 * 24.0 * 60.0 * 60.0;

/// When each missing repository was first noticed, in Unix seconds, keyed by
/// [`normalize_repo_path`].
type Tombstones = HashMap<String, f64>;

fn now_secs() -> f64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|value| value.as_secs() as f64)
    .unwrap_or(0.0)
}

fn tombstone_path(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
  let dir = app
    .path()
    .app_data_dir()
    .map_err(|error| AppError::Other(error.to_string()))?;
  fs::create_dir_all(&dir)?;
  Ok(dir.join("missing-repos.json"))
}

fn read(app: &tauri::AppHandle) -> Tombstones {
  let Ok(path) = tombstone_path(app) else {
    return Tombstones::new();
  };
  let Ok(raw) = fs::read_to_string(path) else {
    return Tombstones::new();
  };
  serde_json::from_str(&raw).unwrap_or_default()
}

fn write(app: &tauri::AppHandle, tombstones: &Tombstones) -> Result<(), AppError> {
  let path = tombstone_path(app)?;
  let json =
    serde_json::to_string_pretty(tombstones).map_err(|error| AppError::Other(error.to_string()))?;
  fs::write(path, json)?;
  Ok(())
}

/// What a reconciliation pass decided.
struct Outcome {
  /// The notes to keep, keyed by normalized path.
  tombstones: Tombstones,
  /// Repositories missing longer than the grace period, in their stored spelling.
  expired: Vec<String>,
}

/// Work out which repositories are missing, which have come back, and which have
/// been gone long enough to forget.
///
/// Pure: `referenced` lists the repositories GitWyrm still remembers and `exists`
/// is the only view of the filesystem, so every case is directly testable.
fn reconcile(
  stored: &Tombstones,
  referenced: &[String],
  now: f64,
  exists: impl Fn(&str) -> bool,
) -> Outcome {
  // Paths reach us spelled several ways (a trailing separator from an open repo,
  // forward slashes from a config file), so everything is keyed on one spelling.
  let mut tombstones: Tombstones = Tombstones::new();
  for (path, first_seen) in stored {
    tombstones
      .entry(normalize_repo_path(path))
      .and_modify(|existing| *existing = existing.min(*first_seen))
      .or_insert(*first_seen);
  }

  // A note about a repository nothing references any more (recents rolled past
  // it, the user unpinned it) has nothing left to protect.
  let known: HashSet<String> = referenced.iter().map(|p| normalize_repo_path(p)).collect();
  tombstones.retain(|key, _| known.contains(key));

  let mut expired = Vec::new();
  for path in referenced {
    let key = normalize_repo_path(path);
    if exists(path) {
      // Back again: drop the note so the repository keeps its settings.
      tombstones.remove(&key);
      continue;
    }
    match tombstones.get(&key).copied() {
      // Keep the first sighting: the clock must not restart on every launch.
      Some(first_seen) if now - first_seen >= GRACE_SECS => expired.push(path.clone()),
      Some(_) => {}
      None => {
        tombstones.insert(key, now);
      }
    }
  }

  Outcome {
    tombstones,
    expired,
  }
}

/// Reconcile the notes with what is on disk, then forget the settings of any
/// repository that has been missing longer than the grace period.
///
/// Runs once at startup. Only paths GitWyrm already knows about are checked, one
/// `is_dir` each, so this never walks the filesystem. Returns what was forgotten.
pub fn sweep(app: &tauri::AppHandle) -> Vec<String> {
  let Ok(mut settings) = settings::get_settings(app.clone()) else {
    return Vec::new();
  };

  let stored = read(app);
  let outcome = reconcile(
    &stored,
    &settings.referenced_repo_paths(),
    now_secs(),
    |path| Path::new(path).is_dir(),
  );

  if outcome.tombstones != stored {
    if let Err(error) = write(app, &outcome.tombstones) {
      log::warn!("could not record missing repositories: {error}");
    }
  }

  if !outcome.expired.is_empty() {
    settings.forget_repos(&outcome.expired);
    crate::commands::repo_icon::forget_icon_keys(app, &outcome.expired);
    if let Err(error) = settings::write_settings(app, &settings) {
      log::warn!("could not prune missing repositories: {error}");
      return Vec::new();
    }
    log::info!(
      "Forgot settings for {} repositories missing over a week",
      outcome.expired.len()
    );
  }
  outcome.expired
}

/// Note that a repository's folder is gone, or that it is back. Called when an
/// open attempt fails because the path is missing, and when one succeeds.
#[tauri::command]
#[specta::specta]
pub fn mark_repo_missing(
  app: tauri::AppHandle,
  repo_path: String,
  missing: bool,
) -> Result<(), AppError> {
  let mut tombstones = read(&app);
  let key = normalize_repo_path(&repo_path);

  let changed = if missing {
    // Keep any existing sighting so the seven-day clock does not restart every
    // time the app launches and fails to open the same folder.
    let first_seen = tombstones.get(&key).copied().unwrap_or_else(now_secs);
    tombstones.insert(key, first_seen) != Some(first_seen)
  } else {
    tombstones.remove(&key).is_some()
  };

  if changed {
    write(&app, &tombstones)?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::settings::Settings;

  const GONE: &str = "C:\\code\\gone";
  const KEY: &str = "c:\\code\\gone";
  const NOW: f64 = 1_800_000_000.0;

  fn stored(entries: &[(&str, f64)]) -> Tombstones {
    entries.iter().map(|(p, t)| ((*p).to_string(), *t)).collect()
  }

  #[test]
  fn a_present_repo_is_never_noted() {
    let outcome = reconcile(&stored(&[]), &[GONE.into()], NOW, |_| true);
    assert!(outcome.tombstones.is_empty());
    assert!(outcome.expired.is_empty());
  }

  #[test]
  fn a_deleted_repo_is_noted_with_the_current_time() {
    let outcome = reconcile(&stored(&[]), &[GONE.into()], NOW, |_| false);
    assert_eq!(outcome.tombstones.get(KEY).copied(), Some(NOW));
    assert!(outcome.expired.is_empty(), "not expired on first sighting");
  }

  #[test]
  fn a_repo_that_comes_back_loses_its_note() {
    // The delete-and-re-clone case: settings must survive.
    let outcome = reconcile(&stored(&[(KEY, NOW - 100.0)]), &[GONE.into()], NOW, |_| true);
    assert!(outcome.tombstones.is_empty());
    assert!(outcome.expired.is_empty());
  }

  #[test]
  fn the_first_sighting_is_kept_across_passes() {
    let first = NOW - 3.0 * 24.0 * 60.0 * 60.0;
    let outcome = reconcile(&stored(&[(KEY, first)]), &[GONE.into()], NOW, |_| false);
    assert_eq!(outcome.tombstones.get(KEY).copied(), Some(first));
    assert!(outcome.expired.is_empty());
  }

  #[test]
  fn just_under_a_week_is_not_expired() {
    let outcome = reconcile(
      &stored(&[(KEY, NOW - GRACE_SECS + 60.0)]),
      &[GONE.into()],
      NOW,
      |_| false,
    );
    assert!(outcome.expired.is_empty());
  }

  #[test]
  fn a_full_week_expires() {
    let outcome = reconcile(
      &stored(&[(KEY, NOW - GRACE_SECS)]),
      &[GONE.into()],
      NOW,
      |_| false,
    );
    assert_eq!(outcome.expired, vec![GONE.to_string()]);
  }

  #[test]
  fn spellings_of_one_path_collapse_to_one_note() {
    // An open repo reports "C:/code/gone/"; the picker sends "C:\code\gone".
    let outcome = reconcile(
      &stored(&[("C:/code/gone/", NOW - GRACE_SECS)]),
      &[GONE.into()],
      NOW,
      |_| false,
    );
    assert_eq!(outcome.tombstones.len(), 1, "one repo, one note");
    assert_eq!(outcome.expired, vec![GONE.to_string()]);
  }

  #[test]
  fn a_note_for_a_forgotten_repo_is_dropped() {
    // Recents rolled past it, so nothing references it and the note is moot.
    let outcome = reconcile(&stored(&[("c:\\code\\orphan", NOW - 100.0)]), &[], NOW, |_| {
      false
    });
    assert!(outcome.tombstones.is_empty());
    assert!(outcome.expired.is_empty(), "nothing to prune, just forget it");
  }

  #[test]
  fn grace_period_is_seven_days() {
    assert_eq!(GRACE_SECS, 604_800.0);
  }

  #[test]
  fn every_place_a_repo_is_remembered_counts_as_a_reference() {
    // referenced_repo_paths decides which notes are worth keeping, so a
    // repository named only by, say, a tab alias must still be covered.
    let mut settings = Settings {
      open_repos: vec!["C:\\a".into()],
      pinned_repo_paths: vec!["C:\\b".into()],
      pinned_tab_paths: vec!["C:\\c".into()],
      ..Settings::default()
    };
    settings.tab_aliases.insert("C:\\d".into(), "D".into());

    let referenced = settings.referenced_repo_paths();
    for path in ["C:\\a", "C:\\b", "C:\\c", "C:\\d"] {
      assert!(
        referenced.iter().any(|p| p == path),
        "{path} should be referenced"
      );
    }
  }
}
