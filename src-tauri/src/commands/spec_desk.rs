//! Opens and focuses the Spec Desk window.
//!
//! One Desk per repository, labelled `spec-desk-<repoId>`. Both windows load the
//! same bundle; the Desk's URL carries `?window=spec-desk&repo=<id>` and the
//! frontend routes on it. That keeps one build, one dev server, and one set of
//! bindings -- there is no second entry point to keep in sync.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::AppError;
use crate::state::RepoManager;

/// Where the Desk ended up, so the UI can say something true either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DeskOutcome {
  /// A new window was created.
  Opened,
  /// One was already open for this repository; it was brought to the front.
  Focused,
}

/// Tauri window labels allow only `[a-zA-Z0-9-_/:]`, and a repo id is a hash of
/// the path, so it is already safe -- but sanitize anyway rather than trusting a
/// caller-supplied string to build a label.
fn desk_label(repo_id: &str) -> String {
  let safe: String = repo_id
    .chars()
    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
    .collect();
  format!("spec-desk-{safe}")
}

/// Opens the Spec Desk for a repository, or focuses the one already open.
///
/// Size and position are remembered by Tauri's own window-state handling per
/// label, so reopening lands where the user left it.
#[tauri::command]
#[specta::specta]
pub async fn open_spec_desk(
  app: AppHandle,
  manager: tauri::State<'_, RepoManager>,
  repo_id: String,
) -> Result<DeskOutcome, AppError> {
  // Fail early with a clear message rather than opening a Desk onto a repo the
  // backend cannot resolve.
  let open = manager.get(&repo_id)?;
  let repo_name = open
    .path
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| "repository".into());

  let label = desk_label(&repo_id);

  if let Some(existing) = app.get_webview_window(&label) {
    // Already open, possibly minimized or buried behind the editor.
    let _ = existing.unminimize();
    let _ = existing.show();
    let _ = existing.set_focus();
    log::info!("spec desk: focused existing window for {repo_id}");
    return Ok(DeskOutcome::Focused);
  }

  // Carry the path, not just the id: the Desk's store starts empty, and the
  // backend already knows the path here, so the window can open its repository
  // without a settings lookup that would have to re-derive it. Percent-encode
  // by hand -- a Windows path is full of characters a query string cannot hold
  // literally, and adding a URL crate for one call is not worth it.
  let encoded_path: String = open
    .path
    .to_string_lossy()
    .bytes()
    .map(|b| match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        (b as char).to_string()
      }
      other => format!("%{other:02X}"),
    })
    .collect();
  let url = format!("index.html?window=spec-desk&repo={repo_id}&path={encoded_path}");
  // `inner_size` here is the FIRST-OPEN default only. The window-state plugin
  // restores a saved size and position after the window is created, so a Desk
  // the user has already arranged reopens where they left it rather than
  // snapping back to these numbers. Keeping the default is what makes the very
  // first open sensible.
  WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
    .title(format!("Spec Desk - {repo_name}"))
    .inner_size(940.0, 760.0)
    .min_inner_size(720.0, 560.0)
    .decorations(false)
    .background_color(tauri::window::Color(0x12, 0x12, 0x12, 0xff))
    .build()
    .map_err(|e| AppError::Other(format!("could not open the Spec Desk window: {e}")))?;

  log::info!("spec desk: opened window for {repo_id}");
  Ok(DeskOutcome::Opened)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn labels_are_stable_and_safe() {
    assert_eq!(desk_label("abc123"), "spec-desk-abc123");
    // Anything a label cannot carry becomes a dash, and the same input always
    // maps to the same label -- otherwise "focus the existing Desk" would miss.
    assert_eq!(desk_label("c:/code/GitWyrm"), "spec-desk-c--code-GitWyrm");
    assert_eq!(desk_label("a b"), "spec-desk-a-b");
    assert_eq!(desk_label("x"), desk_label("x"));
  }
}
