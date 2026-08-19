//! App-level commands: build info, log file management, and the folder handed
//! to us on the command line by Explorer's right-click entry.

use std::fs;
use std::sync::Mutex;

use base64::Engine;
use serde::Serialize;
use specta::Type;
use tauri_plugin_opener::OpenerExt;

use crate::error::AppError;
use crate::logs;

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
    args.into_iter()
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
    PENDING_LAUNCH_PATH
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct BuildInfo {
    pub version: String,
    pub build_date: String,
    pub git_hash: String,
    pub debug: bool,
    pub arch: String,
}

#[tauri::command]
#[specta::specta]
pub fn build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_date: env!("GW_BUILD_DATE").to_string(),
        git_hash: env!("GW_GIT_HASH").to_string(),
        debug: cfg!(debug_assertions),
        arch: std::env::consts::ARCH.to_string(),
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

/// One file in the log folder, for the day picker in the log viewer.
#[derive(Debug, Clone, Serialize, Type)]
pub struct LogFile {
    pub name: String,
    /// Bytes on disk. An `f64` because specta refuses to export the 64-bit
    /// integer types, and JavaScript would widen them to this anyway.
    pub size: f64,
    /// Last written, in Unix milliseconds.
    pub modified_ms: f64,
    /// Whether this is the file the app is writing to right now.
    pub active: bool,
}

/// Lists the log files on disk, newest first.
#[tauri::command]
#[specta::specta]
pub async fn list_logs(app: tauri::AppHandle) -> Result<Vec<LogFile>, AppError> {
    let dir = logs::log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let active = logs::active_path();
        let mut files: Vec<LogFile> = logs::entries(&dir, None)
            .into_iter()
            .map(|entry| LogFile {
                name: entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                size: entry.size as f64,
                modified_ms: modified_ms(&entry.path),
                active: active.as_deref() == Some(entry.path.as_path()),
            })
            .collect();
        // `entries` is oldest first because that is the order the sweep needs; the
        // picker wants today at the top.
        files.reverse();
        Ok(files)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

fn modified_ms(path: &std::path::Path) -> f64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Returns one log file's contents ("" when it does not exist).
///
/// Takes a bare file name, not a path: the viewer only ever asks for something
/// `list_logs` handed it, and resolving anything else would turn a log viewer
/// into a file reader.
#[tauri::command]
#[specta::specta]
pub async fn read_log_file(app: tauri::AppHandle, name: String) -> Result<String, AppError> {
    if !logs::is_managed_name(&name) {
        return Err(AppError::Other(format!("{name} is not a log file")));
    }
    let dir = logs::log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(fs::read_to_string(dir.join(name)).unwrap_or_default())
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// How much of the log a bug report carries. Big enough to hold app startup
/// plus a whole AI model-detection cycle, small enough to upload quickly.
const REPORT_LOG_BYTES: usize = 200_000;

/// Returns the tail of the log for attaching to a bug report.
///
/// Reports want recent history, so this reaches back through the daily files
/// until it has `REPORT_LOG_BYTES` -- a report filed just after midnight would
/// otherwise carry only the handful of lines written since. The frontend scrubs
/// the text before it leaves the machine.
#[tauri::command]
#[specta::specta]
pub async fn read_log_tail(app: tauri::AppHandle) -> Result<String, AppError> {
    let dir = logs::log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || Ok(logs::recent_text(&dir, REPORT_LOG_BYTES)))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
}

/// Largest screenshot a report will carry. Sentry rejects envelopes past 20 MB,
/// and base64 inflates by a third, so this leaves room for the log beside it.
const MAX_SCREENSHOT_BYTES: u64 = 8 * 1024 * 1024;

/// Reads a user-picked screenshot into a data URL for a bug report.
///
/// The file picker hands back a path, and the webview has no filesystem access
/// to follow it, so the bytes have to come through here. Returns a data URL
/// rather than raw bytes so the same string both previews in an `<img>` and
/// rides along as the attachment.
#[tauri::command]
#[specta::specta]
pub async fn read_screenshot(path: String) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::Path::new(&path);
        let mime = match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => {
                return Err(AppError::Other(
                    "Choose a PNG, JPG, WebP, or GIF image.".into(),
                ))
            }
        };

        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(AppError::Other("That is not a file.".into()));
        }
        if metadata.len() > MAX_SCREENSHOT_BYTES {
            return Err(AppError::Other(
                "That image is too large. Choose one under 8 MB.".into(),
            ));
        }

        let encoded = base64::engine::general_purpose::STANDARD.encode(fs::read(path)?);
        Ok(format!("data:{mime};base64,{encoded}"))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Empties the log folder: the file being written is truncated in place so the
/// logger's open handle stays valid, and every other log file is deleted.
#[tauri::command]
#[specta::specta]
pub async fn clear_log(app: tauri::AppHandle) -> Result<(), AppError> {
    let dir = logs::log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let active = logs::active_path();
        for entry in logs::entries(&dir, active.as_deref()) {
            let _ = fs::remove_file(&entry.path);
        }
        if let Some(active) = active.filter(|path| path.exists()) {
            fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(active)?;
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
    let dir = logs::log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.opener()
            .open_path(dir.to_string_lossy().to_string(), None::<&str>)
            .map_err(|e| AppError::Other(e.to_string()))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::{exists_on_disk, repo_path_from_args};

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
