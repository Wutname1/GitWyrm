//! Opens and focuses the Agent Desk window.
//!
//! There is exactly one Agent Desk window for the whole app, at the stable
//! label [`AGENT_DESK_LABEL`]. Every repository, issue, PR, and OpenSpec
//! change funnels through [`open_spec_desk`], which focuses that window
//! (creating it on first use) and tells it which repository/session to show
//! by emitting [`SELECT_DESK_TARGET_EVENT`] rather than only encoding the
//! target in the URL -- the window usually already exists and has a
//! listener, so the event is what actually retargets it after focus.
//!
//! Before this window-ownership migration, GitWyrm opened one Desk *per
//! repository*, labelled `spec-desk-<repoId>`. Those labels can still be
//! sitting on disk in the window-state plugin's store, or even open, on an
//! install that is mid-upgrade. [`legacy_desk_labels`] finds them so the
//! command can fold an old per-repo Desk into the new app-wide one instead
//! of leaving it orphaned or duplicating a window. See
//! `docs/agent-desk/architecture.md` section 6 and
//! `openspec/changes/agent-desk-workspace-layout/design.md` for the product
//! contract this implements.
//!
//! Both windows load the same bundle; the Desk's URL carries
//! `?window=spec-desk&repo=<id>` (translated to `agent-desk` on the frontend,
//! see `src/lib/windowMode.ts`). That keeps one build, one dev server, and
//! one set of bindings -- there is no second entry point to keep in sync.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::error::AppError;
use crate::state::RepoManager;

/// The one stable label every Agent Desk window opens under, app-wide.
///
/// Not per-repository any more: window placement (size/position/maximized,
/// remembered by `tauri_plugin_window_state` per label) is now shared across
/// every repository instead of being remembered separately for each one. A
/// user who had several repositories' Desks arranged on different monitors
/// under the old per-repo labels will see only one of those placements
/// survive the migration (see [`migrate_legacy_desk`]); the rest become
/// orphaned entries in the window-state store that are simply never read
/// again.
pub const AGENT_DESK_LABEL: &str = "agent-desk";

/// Prefix of the legacy per-repository labels this command used to create.
const LEGACY_DESK_PREFIX: &str = "spec-desk-";

/// Emitted at the app-wide Desk window after it has been created or
/// focused, carrying the repository (and optional change) the caller
/// actually wants shown.
///
/// The Desk now persists across repositories instead of being recreated per
/// one, so a kickoff from a second repository cannot rely on the URL alone
/// -- the window is usually already open, already past its initial URL
/// parse, and has to be retargeted live. The URL query params
/// (`repo`/`path`/`change`) remain the source of truth only for the very
/// first paint of a freshly created window; every subsequent kickoff,
/// including the one that created it, also gets this event so there is a
/// single code path in the frontend rather than "URL for open, event for
/// focus".
pub const SELECT_DESK_TARGET_EVENT: &str = "agent-desk://select-target";

/// Payload of [`SELECT_DESK_TARGET_EVENT`].
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectDeskTarget {
    pub repo_id: String,
    pub repo_path: String,
    pub change_id: Option<String>,
}

/// Where the Desk ended up, so the UI can say something true either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DeskOutcome {
    /// A new window was created.
    Opened,
    /// One was already open (new label or a migrated legacy one); it was
    /// brought to the front and retargeted.
    Focused,
}

/// The legacy per-repository label this command used to create, kept only so
/// tests can recognize one -- there is no longer any code path that creates
/// a fresh legacy label, only [`legacy_desk_labels`], which reads whatever
/// labels are already open.
#[cfg(test)]
fn legacy_desk_label(repo_id: &str) -> String {
    // Tauri window labels allow only `[a-zA-Z0-9-_/:]`, and a repo id is a
    // hash of the path, so it is already safe -- but sanitize anyway rather
    // than trusting a caller-supplied string to build a label.
    let safe: String = repo_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("{LEGACY_DESK_PREFIX}{safe}")
}

/// Percent-encode a query-string value. Done by hand -- a Windows path is full
/// of characters a query string cannot hold literally, and adding a URL crate
/// for two calls is not worth it.
fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Every currently open legacy per-repository Desk window, labels sorted so
/// callers get a deterministic order regardless of `webview_windows()`'s
/// iteration order (a `HashMap` internally, so unordered).
fn legacy_desk_labels(app: &AppHandle) -> Vec<String> {
    let mut labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|label| label.starts_with(LEGACY_DESK_PREFIX))
        .cloned()
        .collect();
    labels.sort();
    labels
}

/// Migration policy for multiple legacy per-repo Desks (see module docs and
/// `openspec/changes/agent-desk-workspace-layout/tasks.md` task 1.5):
///
/// 1. If exactly one legacy window is focused, that one is "most recently
///    used" by definition -- at most one window can hold OS focus at a time,
///    so this is an unambiguous, directly observable signal rather than a
///    heuristic.
/// 2. Otherwise (none focused, e.g. all minimized or behind another app)
///    fall back to the lexicographically first label, so the choice is still
///    deterministic and reproducible rather than depending on HashMap
///    iteration order or wall-clock timing.
///
/// The chosen window's placement (position, size, maximized state) is copied
/// onto the newly created app-wide window before that window is shown, so
/// the user's most recent Desk arrangement survives the migration. Every
/// other legacy window found is left exactly as-is -- not closed, not
/// resized -- so the user does not lose an in-progress Desk they were
/// looking at on another monitor; it simply stops being reachable through
/// "Open Spec Desk" (which now always targets the app-wide label), and the
/// user can close it manually.
fn pick_legacy_desk_to_migrate<'a>(
    app: &AppHandle,
    labels: &'a [String],
) -> Option<(&'a str, WebviewWindow)> {
    if labels.is_empty() {
        return None;
    }
    let focused = labels.iter().find(|label| {
        app.get_webview_window(label)
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false)
    });
    let chosen_label = focused.unwrap_or(&labels[0]);
    app.get_webview_window(chosen_label)
        .map(|w| (chosen_label.as_str(), w))
}

/// Copies a legacy Desk's live placement onto the freshly created app-wide
/// window. Best-effort: a placement read/write failure just means the new
/// window keeps its just-created default geometry, which is never worse
/// than the previous per-repo behavior for a first-ever Desk.
fn migrate_legacy_placement(from: &WebviewWindow, to: &WebviewWindow) {
    if let Ok(true) = from.is_maximized() {
        let _ = to.maximize();
        return;
    }
    if let (Ok(position), Ok(size)) = (from.outer_position(), from.inner_size()) {
        let _ = to.set_position(tauri::Position::Physical(position));
        let _ = to.set_size(tauri::Size::Physical(size));
    }
}

/// Opens the app-wide Agent Desk window, or focuses the one already open.
///
/// `change_id` is the change to select on arrival. Lookup order:
/// 1. The app-wide [`AGENT_DESK_LABEL`] -- if it exists, focus and retarget it.
/// 2. Any legacy `spec-desk-<repoId>` window -- migrate the most relevant one
///    (see [`pick_legacy_desk_to_migrate`]) onto the app-wide label instead of
///    creating a window with default placement.
/// 3. Otherwise create the app-wide window fresh.
#[tauri::command]
#[specta::specta]
pub async fn open_spec_desk(
    app: AppHandle,
    manager: tauri::State<'_, RepoManager>,
    repo_id: String,
    change_id: Option<String>,
) -> Result<DeskOutcome, AppError> {
    // Fail early with a clear message rather than opening a Desk onto a repo the
    // backend cannot resolve.
    let open = manager.get(&repo_id)?;
    let repo_name = open
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repository".into());
    let repo_path = open.path.to_string_lossy().into_owned();

    let target = SelectDeskTarget {
        repo_id: repo_id.clone(),
        repo_path: repo_path.clone(),
        change_id: change_id.clone().filter(|c| !c.is_empty()),
    };

    if let Some(existing) = app.get_webview_window(AGENT_DESK_LABEL) {
        // Already open, possibly minimized or buried behind the editor.
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        let _ = app.emit_to(AGENT_DESK_LABEL, SELECT_DESK_TARGET_EVENT, &target);
        log::info!("agent desk: focused existing window for {repo_id}");
        return Ok(DeskOutcome::Focused);
    }

    let encoded_path = percent_encode(&repo_path);
    let mut url = format!("index.html?window=agent-desk&repo={repo_id}&path={encoded_path}");
    if let Some(change_id) = target.change_id.as_deref() {
        url.push_str(&format!("&change={}", percent_encode(change_id)));
    }

    let legacy_labels = legacy_desk_labels(&app);
    let migration_source = pick_legacy_desk_to_migrate(&app, &legacy_labels);
    if let Some((label, _)) = &migration_source {
        log::info!("agent desk: migrating legacy window {label} to {AGENT_DESK_LABEL}");
    }

    // `inner_size` here is the FIRST-OPEN default only, used when there is no
    // legacy window to migrate placement from. The window-state plugin also
    // restores a saved position for `AGENT_DESK_LABEL` itself on later opens,
    // so only the very first Agent Desk a user ever opens (with no legacy
    // Desk to inherit from) sees these numbers.
    let window = WebviewWindowBuilder::new(&app, AGENT_DESK_LABEL, WebviewUrl::App(url.into()))
        .title(format!("Agent Desk - {repo_name}"))
        .inner_size(940.0, 760.0)
        .min_inner_size(720.0, 560.0)
        .decorations(false)
        .resizable(true)
        // Both of these match the main window's tauri.conf.json entry, and both
        // are load-bearing rather than cosmetic.
        //
        // The drag-drop handler defaults to ENABLED in tauri-runtime, and while
        // it is on, the OS file-drop handler swallows pointer input over the webview
        // -- which is exactly what `data-tauri-drag-region` needs to see. The
        // main window turns it off in tauri.conf.json; this window was built in
        // code and inherited the default, so its title bar could not drag the
        // window at all.
        //
        // `resizable` is stated for the same reason: nothing here should depend
        // on a builder default that the main window never relies on.
        .disable_drag_drop_handler()
        .background_color(tauri::window::Color(0x12, 0x12, 0x12, 0xff))
        .build()
        .map_err(|e| AppError::Other(format!("could not open the Agent Desk window: {e}")))?;

    if let Some((_, legacy_window)) = &migration_source {
        migrate_legacy_placement(legacy_window, &window);
    }

    // The URL above covers this window's first paint; the event covers every
    // kickoff after that (including this one, for a frontend that only wants
    // to listen in one place instead of also parsing the URL).
    let _ = app.emit_to(AGENT_DESK_LABEL, SELECT_DESK_TARGET_EVENT, &target);

    log::info!("agent desk: opened window for {repo_id}");
    Ok(DeskOutcome::Opened)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_wide_label_is_stable_and_not_per_repo() {
        // The whole point of this migration: the label no longer varies with
        // repo_id the way the legacy one did.
        assert_eq!(AGENT_DESK_LABEL, "agent-desk");
    }

    #[test]
    fn legacy_labels_are_stable_and_safe() {
        assert_eq!(legacy_desk_label("abc123"), "spec-desk-abc123");
        // Anything a label cannot carry becomes a dash, and the same input always
        // maps to the same label -- otherwise "focus the existing Desk" would miss.
        assert_eq!(
            legacy_desk_label("c:/code/GitWyrm"),
            "spec-desk-c--code-GitWyrm"
        );
        assert_eq!(legacy_desk_label("a b"), "spec-desk-a-b");
        assert_eq!(legacy_desk_label("x"), legacy_desk_label("x"));
    }

    #[test]
    fn legacy_labels_are_recognized_by_prefix() {
        assert!(legacy_desk_label("repo1").starts_with(LEGACY_DESK_PREFIX));
        assert!(!AGENT_DESK_LABEL.starts_with(LEGACY_DESK_PREFIX));
    }

    #[test]
    fn legacy_label_sorting_is_deterministic() {
        // pick_legacy_desk_to_migrate's fallback branch relies on this being
        // sorted output, not HashMap iteration order.
        let mut labels = vec![
            legacy_desk_label("zzz"),
            legacy_desk_label("aaa"),
            legacy_desk_label("mmm"),
        ];
        labels.sort();
        assert_eq!(
            labels,
            vec![
                legacy_desk_label("aaa"),
                legacy_desk_label("mmm"),
                legacy_desk_label("zzz"),
            ]
        );
    }

    #[test]
    fn select_target_serializes_camel_case() {
        let target = SelectDeskTarget {
            repo_id: "abc".into(),
            repo_path: "C:/code/GitWyrm".into(),
            change_id: Some("add-thing".into()),
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("\"repoId\""), "got: {json}");
        assert!(json.contains("\"repoPath\""), "got: {json}");
        assert!(json.contains("\"changeId\""), "got: {json}");
    }

    #[test]
    fn select_target_change_id_omits_empty() {
        let target = SelectDeskTarget {
            repo_id: "abc".into(),
            repo_path: "C:/code/GitWyrm".into(),
            change_id: None,
        };
        let json = serde_json::to_string(&target).unwrap();
        let back: SelectDeskTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(back.change_id, None);
    }

    #[test]
    fn event_name_is_namespaced() {
        // Namespaced so it reads unambiguously in frontend listener code next
        // to airun's RUN_EVENT and agentdesk's AGENT_SESSION_EVENT.
        assert_eq!(SELECT_DESK_TARGET_EVENT, "agent-desk://select-target");
    }
}
