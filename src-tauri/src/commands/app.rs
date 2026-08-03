//! App-level commands: build info, log file management, and the folder handed
//! to us on the command line by Explorer's right-click entry.

use std::fs;
use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

use crate::error::AppError;

pub const LOG_FILE_NAME: &str = "gitwyrm";

/// Folder passed on the command line, waiting for the UI to collect it.
///
/// A slot rather than an event, because of launch timing: the first process
/// parses argv long before the webview exists, so an event emitted then would
/// land with nobody listening and the folder would be silently dropped. The
/// frontend drains this once it is ready. Second and later launches go through
/// the single-instance hook instead, which fires with the UI already up.
static PENDING_LAUNCH_PATH: Mutex<Option<String>> = Mutex::new(None);

/// Pull the folder to open out of a process's arguments.
///
/// Explorer invokes us as `GitWyrm.exe "C:\some\folder"`, so we take the first
/// argument that is an existing directory. Scanning for a real directory rather
/// than blindly taking `argv[1]` keeps flags (`--foo`) and the WebView2 switches
/// Tauri itself appends from being mistaken for a path.
pub fn repo_path_from_args<I, S>(args: I) -> Option<String>
where
  I: IntoIterator<Item = S>,
  S: AsRef<str>,
{
  args
    .into_iter()
    .skip(1) // argv[0] is our own executable
    .map(|arg| arg.as_ref().to_string())
    .find(|arg| !arg.starts_with('-') && std::path::Path::new(arg).is_dir())
}

/// Record a folder for the UI to open once it has finished starting.
pub fn set_pending_launch_path(path: Option<String>) {
  if let Some(path) = path {
    log::info!("Opening {path} from the command line");
    if let Ok(mut slot) = PENDING_LAUNCH_PATH.lock() {
      *slot = Some(path);
    }
  }
}

/// Take the folder GitWyrm was launched with, if any.
///
/// Draining, not peeking: the frontend calls this during startup, and leaving
/// the value behind would reopen the same folder on every later read.
#[tauri::command]
#[specta::specta]
pub fn launch_repo_path() -> Option<String> {
  PENDING_LAUNCH_PATH.lock().ok().and_then(|mut slot| slot.take())
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct BuildInfo {
  pub version: String,
  pub build_date: String,
  pub git_hash: String,
  pub debug: bool,
}

#[tauri::command]
#[specta::specta]
pub fn build_info() -> BuildInfo {
  BuildInfo {
    version: env!("CARGO_PKG_VERSION").to_string(),
    build_date: env!("GW_BUILD_DATE").to_string(),
    git_hash: env!("GW_GIT_HASH").to_string(),
    debug: cfg!(debug_assertions),
  }
}

/// Checks whether a file or folder already exists without changing it.
#[tauri::command]
#[specta::specta]
pub async fn path_exists(path: String) -> bool {
  tauri::async_runtime::spawn_blocking(move || exists_on_disk(&path))
    .await
    .unwrap_or(false)
}

/// The `path_exists` check itself. Split out so it stays directly testable
/// without standing up an async runtime.
fn exists_on_disk(path: &str) -> bool {
  std::path::Path::new(path).exists()
}

fn log_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, AppError> {
  let dir = app
    .path()
    .app_log_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  Ok(dir.join(format!("{LOG_FILE_NAME}.log")))
}

/// Returns the current log file contents ("" when it does not exist yet).
#[tauri::command]
#[specta::specta]
pub async fn read_log(app: tauri::AppHandle) -> Result<String, AppError> {
  let path = log_path(&app)?;
  tauri::async_runtime::spawn_blocking(move || Ok(fs::read_to_string(path).unwrap_or_default()))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// How much of the log a bug report carries. Big enough to hold app startup
/// plus a whole AI model-detection cycle, small enough to upload quickly.
const REPORT_LOG_BYTES: usize = 200_000;

/// Returns the tail of the log for attaching to a bug report.
///
/// Reports want recent history, not the whole rotating file, so this trims to
/// the last `REPORT_LOG_BYTES` and drops the leading partial line so the result
/// always starts on a real log entry. The frontend scrubs the text before it
/// leaves the machine.
#[tauri::command]
#[specta::specta]
pub async fn read_log_tail(app: tauri::AppHandle) -> Result<String, AppError> {
  let path = log_path(&app)?;
  tauri::async_runtime::spawn_blocking(move || {
    let text = fs::read_to_string(path).unwrap_or_default();
    Ok(tail_from_line_boundary(&text, REPORT_LOG_BYTES))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Keep the last `max_bytes` of `text`, starting at the next line boundary so
/// the first entry is never a fragment. Returns the whole input when it fits.
fn tail_from_line_boundary(text: &str, max_bytes: usize) -> String {
  if text.len() <= max_bytes {
    return text.to_string();
  }
  // Slice on a char boundary first: the cut point can land mid-UTF-8, and
  // indexing a &str there panics.
  let mut start = text.len() - max_bytes;
  while start < text.len() && !text.is_char_boundary(start) {
    start += 1;
  }
  let cut = &text[start..];
  match cut.find('\n') {
    Some(nl) => cut[nl + 1..].to_string(),
    None => cut.to_string(),
  }
}

/// Truncates the log file in place so the logger's open handle stays valid.
#[tauri::command]
#[specta::specta]
pub async fn clear_log(app: tauri::AppHandle) -> Result<(), AppError> {
  let path = log_path(&app)?;
  tauri::async_runtime::spawn_blocking(move || {
    if path.exists() {
      fs::OpenOptions::new().write(true).truncate(true).open(path)?;
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Opens the log directory in the OS file manager.
#[tauri::command]
#[specta::specta]
pub async fn open_logs_folder(app: tauri::AppHandle) -> Result<(), AppError> {
  let dir = app
    .path()
    .app_log_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  tauri::async_runtime::spawn_blocking(move || {
    if !dir.exists() {
      fs::create_dir_all(&dir)?;
    }
    app
      .opener()
      .open_path(dir.to_string_lossy().to_string(), None::<&str>)
      .map_err(|e| AppError::Other(e.to_string()))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
  use super::{exists_on_disk, repo_path_from_args, tail_from_line_boundary};

  #[test]
  fn short_logs_are_returned_whole() {
    let text = "[INFO] one\n[INFO] two\n";
    assert_eq!(tail_from_line_boundary(text, 1000), text);
  }

  #[test]
  fn long_logs_start_on_a_whole_entry() {
    let text = "[INFO] aaaaaaaaaa\n[INFO] bbbbbbbbbb\n[INFO] cccccccccc\n";
    // Cuts inside the second line; the partial entry must be dropped.
    let out = tail_from_line_boundary(text, 25);
    assert!(out.starts_with("[INFO] "), "got {out:?}");
    assert!(out.ends_with("[INFO] cccccccccc\n"));
    assert!(!out.contains("aaaaaaaaaa"));
  }

  #[test]
  fn multibyte_text_does_not_panic_at_the_cut() {
    // A cut landing mid-UTF-8 would panic on a naive slice.
    let text = "[INFO] ✅✅✅✅✅✅✅✅✅✅\n[INFO] done\n";
    for max in 1..text.len() {
      let out = tail_from_line_boundary(text, max);
      assert!(text.ends_with(&out) || out.is_empty(), "max={max}");
    }
  }

  #[test]
  fn a_single_huge_line_is_still_trimmed() {
    // No newline to cut on: return the tail rather than the whole line.
    let text = format!("[INFO] {}", "x".repeat(500));
    let out = tail_from_line_boundary(&text, 100);
    assert_eq!(out.len(), 100);
  }

  #[test]
  fn path_exists_reports_existing_and_missing_paths() {
    let directory = tempfile::tempdir().expect("temporary directory");
    assert!(exists_on_disk(&directory.path().to_string_lossy()));
    assert!(!exists_on_disk(
      &directory.path().join("not-created").to_string_lossy()
    ));
  }

  #[test]
  fn finds_the_folder_explorer_passed() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().to_string_lossy().to_string();
    assert_eq!(
      repo_path_from_args(["GitWyrm.exe", &path]),
      Some(path.clone())
    );
  }

  #[test]
  fn ignores_our_own_executable_path() {
    // argv[0] is a real path but never the folder to open. Passing a directory
    // as argv[0] must still yield nothing.
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().to_string_lossy().to_string();
    assert_eq!(repo_path_from_args([&path]), None);
  }

  #[test]
  fn ignores_flags_and_non_directories() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().to_string_lossy().to_string();
    let file = directory.path().join("a-file.txt");
    std::fs::write(&file, "x").expect("write file");

    // A flag that happens to precede the folder must not win, and a file is not
    // something we can open as a repository.
    assert_eq!(
      repo_path_from_args([
        "GitWyrm.exe",
        "--webview-flag",
        &file.to_string_lossy(),
        &path,
      ]),
      Some(path)
    );
  }

  #[test]
  fn no_arguments_means_a_normal_launch() {
    assert_eq!(repo_path_from_args(["GitWyrm.exe"]), None);
    assert_eq!(
      repo_path_from_args(["GitWyrm.exe", "C:\\definitely\\not\\here\\12345"]),
      None
    );
  }
}
