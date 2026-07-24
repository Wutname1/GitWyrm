//! Watches an open repository for external changes (editor saves, terminal
//! git commands) and emits a debounced `repo-changed` event to the frontend.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use std::path::PathBuf;

use notify_debouncer_full::{new_debouncer, notify::RecursiveMode, DebouncedEvent, Debouncer, RecommendedCache};
use notify_debouncer_full::notify::RecommendedWatcher;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Type)]
pub struct RepoChangedPayload {
  pub repo_id: String,
}

type RepoDebouncer = Debouncer<RecommendedWatcher, RecommendedCache>;

#[derive(Default)]
pub struct WatcherRegistry {
  watchers: Mutex<HashMap<String, RepoDebouncer>>,
}

impl WatcherRegistry {
  /// Registers the recursive watch on a background blocking thread and returns
  /// immediately. Walking a large working tree to arm `ReadDirectoryChangesW`
  /// can take seconds, and none of `open_repo`'s callers need the watch to be
  /// live before they get their `RepoInfo` back -- external-change events just
  /// start flowing a moment later. `app` owns the managed `WatcherRegistry`, so
  /// the spawned task reaches back into it through state rather than borrowing.
  pub fn watch_deferred(app: AppHandle, repo_id: String, workdir: PathBuf) {
    tauri::async_runtime::spawn_blocking(move || {
      let start = std::time::Instant::now();
      let registry = app.state::<WatcherRegistry>();
      match registry.watch(app.clone(), repo_id.clone(), &workdir) {
        Ok(()) => log::info!(
          "watch: armed in {}ms for {}",
          start.elapsed().as_millis(),
          workdir.display()
        ),
        Err(e) => log::warn!("watch: failed to arm for {}: {e}", workdir.display()),
      }
    });
  }

  pub fn watch(&self, app: AppHandle, repo_id: String, workdir: &Path) -> Result<(), String> {
    let id = repo_id.clone();
    let mut debouncer = new_debouncer(
      Duration::from_millis(300),
      None,
      move |result: Result<Vec<DebouncedEvent>, Vec<notify_debouncer_full::notify::Error>>| {
        if let Ok(events) = result {
          // Ignore churn inside .git internals that doesn't change visible
          // state (index.lock chatter is fine to notify on; keep it simple).
          if events.is_empty() {
            return;
          }
          let _ = app.emit("repo-changed", RepoChangedPayload { repo_id: id.clone() });
        }
      },
    )
    .map_err(|e| e.to_string())?;

    debouncer
      .watch(workdir, RecursiveMode::Recursive)
      .map_err(|e| e.to_string())?;

    self.watchers.lock().unwrap().insert(repo_id, debouncer);
    Ok(())
  }

  pub fn unwatch(&self, repo_id: &str) {
    self.watchers.lock().unwrap().remove(repo_id);
  }
}
