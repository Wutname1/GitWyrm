//! Commands that hand a repository off to another program: the OS file
//! manager, a code editor, and a terminal. All operate on the open repo's
//! working directory.
//!
//! Which editors exist and how they are launched lives in
//! [`crate::commands::editors`]; this module only wires them to a repo.

use std::process::Command;

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::commands::editors::{self, EditorAvailability, EditorKind};
use crate::commands::opencode;
use crate::error::AppError;
use crate::state::RepoManager;
use tauri::State;

#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

/// Working directory of an open repo, as an owned String path.
fn repo_path(manager: &RepoManager, repo_id: &str) -> Result<String, AppError> {
    let open = manager.get(repo_id)?;
    Ok(open.path.to_string_lossy().into_owned())
}

/// Open the repo folder in the OS file manager (Explorer / Finder / xdg).
#[tauri::command]
#[specta::specta]
pub async fn reveal_in_file_manager(
    app: AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: String,
) -> Result<(), AppError> {
    let path = repo_path(&manager, &repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.opener()
            .open_path(path, None::<&str>)
            .map_err(|e| AppError::Other(e.to_string()))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Open the repo folder in the given editor. Each editor is launched through
/// its command-line launcher, which has to be on PATH -- for VS Code that is
/// the "Shell Command: Install 'code' command in PATH" step.
#[tauri::command]
#[specta::specta]
pub async fn open_in_editor(
    manager: State<'_, RepoManager>,
    repo_id: String,
    editor: EditorKind,
) -> Result<(), AppError> {
    let path = repo_path(&manager, &repo_id)?;
    tauri::async_runtime::spawn_blocking(move || editors::open_path_in(editor, &path))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
}

/// Which editors are installed, whether Visual Studio is available, and the
/// solutions in this repo. Drives the toolbar's open button and the editor
/// picker in Settings.
/// Detection spawns a process per candidate launcher and walks the repo for
/// solutions, so it must not run on the IPC thread: everything else the
/// frontend asks for queues behind whatever is running there. The repo path is
/// resolved up front because `State` cannot cross into the blocking pool.
#[tauri::command]
#[specta::specta]
pub async fn get_editor_availability(
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
) -> Result<EditorAvailability, AppError> {
    let repo_path = match &repo_id {
        Some(id) => Some(manager.get(id)?.path.clone()),
        None => None,
    };

    tauri::async_runtime::spawn_blocking(move || {
        let _timing =
            crate::perf::CommandTiming::start("get_editor_availability", "editors.detect");
        let visual_studio = editors::detect_visual_studio();

        // Scanning for solutions is only worth the disk walk when there is a Visual
        // Studio to open them with, and when a repo is actually open.
        let solutions = match (&visual_studio, &repo_path) {
            (Some(_), Some(path)) => editors::find_solutions(path),
            _ => Vec::new(),
        };

        Ok(EditorAvailability {
            editors: editors::detect_editors(),
            visual_studio,
            solutions,
        })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Open one of the repo's `.sln` files in Visual Studio. The path is checked
/// against the repo's own solutions so an arbitrary path cannot be launched.
#[tauri::command]
#[specta::specta]
pub async fn open_solution_in_visual_studio(
    manager: State<'_, RepoManager>,
    repo_id: String,
    solution_path: String,
) -> Result<(), AppError> {
    let root = manager.get(&repo_id)?.path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Re-walks the repo to validate the path, then launches devenv.
        let known = editors::find_solutions(&root);
        if !known.iter().any(|s| s.absolute_path == solution_path) {
            return Err(AppError::Other(format!(
                "No solution named {solution_path} in this repository."
            )));
        }
        editors::open_solution(&solution_path)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Start opencode on one task, in the repo folder, with the handoff as its
/// opening message.
///
/// The handoff arrives from the frontend rather than being rebuilt here so that
/// what opencode receives is byte-for-byte what the Desk shows under "what gets
/// copied" -- one composer, no chance of the two drifting apart.
#[tauri::command]
#[specta::specta]
pub async fn open_in_opencode(
    manager: State<'_, RepoManager>,
    repo_id: String,
    handoff: String,
) -> Result<(), AppError> {
    let path = repo_path(&manager, &repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        // find_opencode probes PATH with a `where`/`which` process before launching.
        let Some(found) = opencode::find_opencode() else {
            return Err(AppError::Other(
                "Could not find opencode. Install it from opencode.ai, then try again.".into(),
            ));
        };
        opencode::launch(&found, &path, &handoff)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Open a terminal in the repo folder. Uses Windows Terminal if present,
/// falling back to the classic console; a native terminal elsewhere.
///
/// Spawning a terminal is a process launch (several, when the first candidate
/// is absent), so it runs off the IPC thread.
#[tauri::command]
#[specta::specta]
pub async fn open_in_terminal(
    manager: State<'_, RepoManager>,
    repo_id: String,
) -> Result<(), AppError> {
    let path = repo_path(&manager, &repo_id)?;
    tauri::async_runtime::spawn_blocking(move || spawn_terminal_in(path))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
}

fn spawn_terminal_in(path: String) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Prefer Windows Terminal (`wt`), which takes -d for the start directory.
        let wt = Command::new("wt")
            .args(["-d", &path])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        if wt.is_ok() {
            return Ok(());
        }
        // Fall back to a plain console started in the repo directory.
        Command::new("cmd")
            .args(["/C", "start", "cmd"])
            .current_dir(&path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(AppError::Io)?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal", &path])
            .spawn()
            .map_err(AppError::Io)?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Try a few common Linux terminals in turn.
        let candidates: [&[&str]; 4] = [
            &["x-terminal-emulator"],
            &["gnome-terminal"],
            &["konsole"],
            &["xterm"],
        ];
        for args in candidates {
            if Command::new(args[0]).current_dir(&path).spawn().is_ok() {
                return Ok(());
            }
        }
        return Err(AppError::Other("No terminal emulator found".into()));
    }

    #[allow(unreachable_code)]
    Ok(())
}
