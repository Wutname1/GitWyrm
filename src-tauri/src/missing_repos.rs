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

fn tombstone_path(dir: &Path) -> PathBuf {
  dir.join("missing-repos.json")
}

fn read_in(dir: &Path) -> Tombstones {
  let Ok(raw) = fs::read_to_string(tombstone_path(dir)) else {
    return Tombstones::new();
  };
  serde_json::from_str(&raw).unwrap_or_default()
}

fn write_in(dir: &Path, tombstones: &Tombstones) -> Result<(), AppError> {
  let json =
    serde_json::to_string_pretty(tombstones).map_err(|error| AppError::Other(error.to_string()))?;
  fs::write(tombstone_path(dir), json)?;
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
      // Out of grace. The note has done its job, so it goes along with the
      // settings the caller is about to drop - otherwise it would linger with
      // nothing left to protect and be re-expired on every later launch.
      Some(first_seen) if now - first_seen >= GRACE_SECS => {
        tombstones.remove(&key);
        expired.push(path.clone());
      }
      // Keep the first sighting: the clock must not restart on every launch.
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

/// The whole sweep against one app data directory: read both files, reconcile,
/// and write back whatever changed. `now` and `exists` are injected so a test can
/// drive the real file I/O with a fake clock and a fake filesystem view.
///
/// Returns the repositories whose settings were forgotten.
fn sweep_in(dir: &Path, now: f64, exists: impl Fn(&str) -> bool) -> Vec<String> {
  let mut settings = settings::read_settings_in(dir);
  let stored = read_in(dir);
  let outcome = reconcile(&stored, &settings.referenced_repo_paths(), now, exists);

  if outcome.tombstones != stored {
    if let Err(error) = write_in(dir, &outcome.tombstones) {
      log::warn!("could not record missing repositories: {error}");
    }
  }

  if !outcome.expired.is_empty() {
    settings.forget_repos(&outcome.expired);
    if let Err(error) = settings::write_settings_in(dir, &settings) {
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

/// Reconcile the notes with what is on disk, then forget the settings of any
/// repository that has been missing longer than the grace period.
///
/// Runs once at startup. Only paths GitWyrm already knows about are checked, one
/// `is_dir` each, so this never walks the filesystem. Returns what was forgotten.
pub fn sweep(app: &tauri::AppHandle) -> Vec<String> {
  let Ok(dir) = settings::app_data_dir(app) else {
    return Vec::new();
  };
  let expired = sweep_in(&dir, now_secs(), |path| Path::new(path).is_dir());
  // Icon files live outside settings.json, so they are cleaned up here rather
  // than inside the directory-only core.
  if !expired.is_empty() {
    crate::commands::repo_icon::forget_icon_keys(app, &expired);
  }
  expired
}

/// Note that a repository's folder is gone, or that it is back. Called when an
/// open attempt fails because the path is missing, and when one succeeds.
fn mark_repo_missing_in(
  dir: &Path,
  repo_path: &str,
  missing: bool,
  now: f64,
) -> Result<(), AppError> {
  let mut tombstones = read_in(dir);
  let key = normalize_repo_path(repo_path);

  let changed = if missing {
    // Keep any existing sighting so the seven-day clock does not restart every
    // time the app launches and fails to open the same folder.
    let first_seen = tombstones.get(&key).copied().unwrap_or(now);
    tombstones.insert(key, first_seen).is_none()
  } else {
    tombstones.remove(&key).is_some()
  };

  if changed {
    write_in(dir, &tombstones)?;
  }
  Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn mark_repo_missing(
  app: tauri::AppHandle,
  repo_path: String,
  missing: bool,
) -> Result<(), AppError> {
  let dir = settings::app_data_dir(&app)?;
  // Reads and rewrites the tombstone file; launch restore calls this once per
  // missing repo, so several land at the same moment.
  tauri::async_runtime::spawn_blocking(move || {
    mark_repo_missing_in(&dir, &repo_path, missing, now_secs())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
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
    // Still inside the grace period, so the single surviving note is visible.
    let outcome = reconcile(
      &stored(&[("C:/code/gone/", NOW - 100.0)]),
      &[GONE.into()],
      NOW,
      |_| false,
    );
    assert_eq!(outcome.tombstones.len(), 1, "one repo, one note");
    assert_eq!(outcome.tombstones.get(KEY).copied(), Some(NOW - 100.0));
    assert!(outcome.expired.is_empty());
  }

  #[test]
  fn a_differently_spelled_note_still_expires_the_repo() {
    let outcome = reconcile(
      &stored(&[("C:/code/gone/", NOW - GRACE_SECS)]),
      &[GONE.into()],
      NOW,
      |_| false,
    );
    assert_eq!(outcome.expired, vec![GONE.to_string()]);
  }

  #[test]
  fn an_expired_note_is_dropped_with_the_settings() {
    // The note exists to protect settings during the grace period. Once those
    // settings go, keeping it would re-expire the repository on every launch.
    let outcome = reconcile(
      &stored(&[(KEY, NOW - GRACE_SECS)]),
      &[GONE.into()],
      NOW,
      |_| false,
    );
    assert_eq!(outcome.expired, vec![GONE.to_string()]);
    assert!(outcome.tombstones.is_empty(), "note goes with the settings");
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

  /// End-to-end tests over a real app data directory.
  ///
  /// These run the same read/reconcile/write code the app runs at startup, on
  /// real files, so a bug in the wiring between the pieces cannot hide behind a
  /// passing unit test. Only the clock and the "does this folder exist?" check
  /// are injected; everything else is production code.
  mod on_disk {
    use super::*;
    use tempfile::TempDir;

    /// A repository the sweep should treat as deleted.
    const MISSING: &str = "C:\\code\\deleted-repo";
    /// A repository whose folder is still there.
    const PRESENT: &str = "C:\\code\\live-repo";

    /// An app data directory holding `settings` and no notes yet.
    fn app_dir(settings: &Settings) -> TempDir {
      let dir = TempDir::new().expect("temp dir");
      settings::write_settings_in(dir.path(), settings).expect("write settings");
      dir
    }

    /// Settings that remember one repository in recents and give it a tab name,
    /// the shape a repository the user has actually opened ends up in.
    fn remembering(path: &str) -> Settings {
      let mut settings = Settings {
        recents: vec![crate::settings::RecentRepo {
          name: "repo".into(),
          path: path.into(),
        }],
        ..Settings::default()
      };
      settings.tab_aliases.insert(path.into(), "My Repo".into());
      settings
    }

    /// Only `MISSING` is treated as gone.
    fn only_missing_is_gone(path: &str) -> bool {
      normalize_repo_path(path) != normalize_repo_path(MISSING)
    }

    fn notes(dir: &TempDir) -> Tombstones {
      read_in(dir.path())
    }

    #[test]
    fn a_deleted_repo_gets_a_note_written_to_disk() {
      let dir = app_dir(&remembering(MISSING));

      let expired = sweep_in(dir.path(), NOW, only_missing_is_gone);

      assert!(expired.is_empty(), "one week has not passed yet");
      assert!(
        tombstone_path(dir.path()).is_file(),
        "the note file should have been created"
      );
      assert_eq!(
        notes(&dir).get(&normalize_repo_path(MISSING)).copied(),
        Some(NOW),
        "the deleted repository should be noted as of now"
      );
    }

    #[test]
    fn a_present_repo_gets_no_note_file_at_all() {
      let dir = app_dir(&remembering(PRESENT));

      sweep_in(dir.path(), NOW, only_missing_is_gone);

      assert!(
        !tombstone_path(dir.path()).is_file(),
        "nothing is missing, so there is nothing to write"
      );
    }

    #[test]
    fn a_second_sweep_keeps_the_first_sighting() {
      // The seven-day clock must survive relaunches, so the note written by the
      // first sweep has to be the one the second sweep keeps.
      let dir = app_dir(&remembering(MISSING));

      sweep_in(dir.path(), NOW, only_missing_is_gone);
      let a_day_later = NOW + 24.0 * 60.0 * 60.0;
      sweep_in(dir.path(), a_day_later, only_missing_is_gone);

      assert_eq!(
        notes(&dir).get(&normalize_repo_path(MISSING)).copied(),
        Some(NOW),
        "the clock must not restart on the second launch"
      );
    }

    #[test]
    fn a_repo_that_comes_back_loses_its_note_and_keeps_its_settings() {
      // The delete-and-re-clone case, the whole reason notes expire slowly.
      let dir = app_dir(&remembering(MISSING));
      sweep_in(dir.path(), NOW, only_missing_is_gone);
      assert!(!notes(&dir).is_empty(), "noted while gone");

      // Same settings, but now the folder is back.
      let expired = sweep_in(dir.path(), NOW + 60.0, |_| true);

      assert!(expired.is_empty());
      assert!(notes(&dir).is_empty(), "the note should be cleared");
      let settings = settings::read_settings_in(dir.path());
      assert_eq!(settings.recents.len(), 1, "recents kept");
      assert!(settings.tab_aliases.contains_key(MISSING), "tab name kept");
    }

    #[test]
    fn settings_are_forgotten_only_after_the_grace_period() {
      let dir = app_dir(&remembering(MISSING));
      sweep_in(dir.path(), NOW, only_missing_is_gone);

      // One minute short of a week: still protected.
      let almost = NOW + GRACE_SECS - 60.0;
      assert!(sweep_in(dir.path(), almost, only_missing_is_gone).is_empty());
      assert_eq!(
        settings::read_settings_in(dir.path()).recents.len(),
        1,
        "settings survive right up to the deadline"
      );

      // A week later: forgotten.
      let expired = sweep_in(dir.path(), NOW + GRACE_SECS, only_missing_is_gone);
      assert_eq!(expired, vec![MISSING.to_string()]);

      let settings = settings::read_settings_in(dir.path());
      assert!(settings.recents.is_empty(), "recents cleared");
      assert!(settings.tab_aliases.is_empty(), "tab name cleared");
      assert!(notes(&dir).is_empty(), "the note goes with the settings");
    }

    #[test]
    fn one_repos_expiry_leaves_another_repo_alone() {
      let mut settings = remembering(MISSING);
      settings.recents.push(crate::settings::RecentRepo {
        name: "live".into(),
        path: PRESENT.into(),
      });
      settings.tab_aliases.insert(PRESENT.into(), "Live".into());
      let dir = app_dir(&settings);

      sweep_in(dir.path(), NOW, only_missing_is_gone);
      let expired = sweep_in(dir.path(), NOW + GRACE_SECS, only_missing_is_gone);

      assert_eq!(expired, vec![MISSING.to_string()]);
      let settings = settings::read_settings_in(dir.path());
      assert_eq!(settings.recents.len(), 1);
      assert_eq!(settings.recents[0].path, PRESENT);
      assert!(settings.tab_aliases.contains_key(PRESENT));
    }

    #[test]
    fn marking_and_unmarking_round_trips_through_the_file() {
      let dir = app_dir(&remembering(MISSING));

      mark_repo_missing_in(dir.path(), MISSING, true, NOW).expect("mark");
      assert_eq!(
        notes(&dir).get(&normalize_repo_path(MISSING)).copied(),
        Some(NOW)
      );

      // Marking again later must not move the timestamp.
      mark_repo_missing_in(dir.path(), MISSING, true, NOW + 5_000.0).expect("mark again");
      assert_eq!(
        notes(&dir).get(&normalize_repo_path(MISSING)).copied(),
        Some(NOW),
        "the first sighting stands"
      );

      mark_repo_missing_in(dir.path(), MISSING, false, NOW + 6_000.0).expect("unmark");
      assert!(notes(&dir).is_empty(), "coming back clears the note");
    }

    #[test]
    fn a_note_written_by_one_path_spelling_is_found_by_another() {
      // An open repository reports "C:/code/deleted-repo/" while the picker sends
      // "C:\code\deleted-repo". Both must land on the same note.
      let dir = app_dir(&remembering(MISSING));

      mark_repo_missing_in(dir.path(), "C:/code/deleted-repo/", true, NOW).expect("mark");
      let expired = sweep_in(dir.path(), NOW + GRACE_SECS, only_missing_is_gone);

      assert_eq!(
        expired,
        vec![MISSING.to_string()],
        "the differently spelled note should still expire this repository"
      );
    }

    #[test]
    fn a_frontend_settings_write_cannot_erase_a_note() {
      // The bug that motivated giving notes their own file: the frontend rewrites
      // settings.json wholesale from its own snapshot. That must not touch notes.
      let dir = app_dir(&remembering(MISSING));
      sweep_in(dir.path(), NOW, only_missing_is_gone);
      let before = notes(&dir);
      assert!(!before.is_empty());

      // A save that knows nothing about the missing repository.
      settings::write_settings_in(dir.path(), &Settings::default()).expect("frontend save");

      assert_eq!(notes(&dir), before, "the note survives untouched");
    }

    #[test]
    fn a_corrupt_note_file_is_ignored_rather_than_fatal() {
      let dir = app_dir(&remembering(MISSING));
      fs::write(tombstone_path(dir.path()), "{ not json").expect("write junk");

      // Starts over from empty rather than failing or losing settings.
      sweep_in(dir.path(), NOW, only_missing_is_gone);

      assert_eq!(
        notes(&dir).get(&normalize_repo_path(MISSING)).copied(),
        Some(NOW)
      );
      assert_eq!(settings::read_settings_in(dir.path()).recents.len(), 1);
    }
  }
}
