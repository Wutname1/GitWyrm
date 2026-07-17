//! Commands that hand a repository off to another program: the OS file
//! manager, an editor (VS Code), and a terminal. All operate on the open
//! repo's working directory.

use std::process::Command;

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::error::AppError;
use crate::state::RepoManager;
use tauri::State;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Working directory of an open repo, as an owned String path.
fn repo_path(manager: &RepoManager, repo_id: &str) -> Result<String, AppError> {
  let open = manager.get(repo_id)?;
  Ok(open.path.to_string_lossy().into_owned())
}

/// Open the repo folder in the OS file manager (Explorer / Finder / xdg).
#[tauri::command]
#[specta::specta]
pub fn reveal_in_file_manager(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let path = repo_path(&manager, &repo_id)?;
  app
    .opener()
    .open_path(path, None::<&str>)
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Open the repo in VS Code via its `code` launcher. Requires `code` on PATH
/// (VS Code's "Shell Command: Install 'code' command in PATH").
#[tauri::command]
#[specta::specta]
pub fn open_in_editor(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let path = repo_path(&manager, &repo_id)?;

  // On Windows `code` is a .cmd shim, so it must be launched through the shell;
  // elsewhere it is a normal executable on PATH.
  #[cfg(windows)]
  let mut cmd = {
    let mut c = Command::new("cmd");
    c.args(["/C", "code", &path]);
    use std::os::windows::process::CommandExt;
    c.creation_flags(CREATE_NO_WINDOW);
    c
  };
  #[cfg(not(windows))]
  let mut cmd = {
    let mut c = Command::new("code");
    c.arg(&path);
    c
  };

  cmd.spawn().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other(
        "Could not find VS Code. Install it, then run \"Shell Command: Install 'code' command in PATH\" from VS Code.".into(),
      )
    } else {
      AppError::Io(e)
    }
  })?;
  Ok(())
}

/// Open a terminal in the repo folder. Uses Windows Terminal if present,
/// falling back to the classic console; a native terminal elsewhere.
#[tauri::command]
#[specta::specta]
pub fn open_in_terminal(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let path = repo_path(&manager, &repo_id)?;

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
