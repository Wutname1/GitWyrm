//! Repository tab icons: lightweight discovery inside a worktree plus a small,
//! app-owned custom-icon cache. Settings stay text-only; image bytes cross the
//! command boundary as data URLs only while the app is running.
//!
//! Discovery walks the worktree, so it only ever runs for a repository the user
//! actually opened. The result is remembered in an index next to the custom
//! icons, letting screens that list many repositories (the open-a-repository
//! screen) show icons for a single `stat` per entry instead of a scan each.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;

use crate::error::AppError;

const MAX_DISCOVERED_BYTES: u64 = 2 * 1024 * 1024;
const MAX_CUSTOM_BYTES: u64 = 10 * 1024 * 1024;
const MAX_RESULTS: usize = 12;
const MAX_SCAN_DEPTH: usize = 5;
const RASTER_ICON_SIZE: u32 = 64;

#[derive(Debug, Clone, Serialize, Type)]
pub struct RepoIcon {
    pub source_path: String,
    pub label: String,
    pub data_url: String,
    pub custom: bool,
}

/// One remembered icon location. Stored per repository so a later lookup can
/// read the file directly instead of walking the worktree again.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedIcon {
    /// Absolute path of the image that won discovery, or of the custom icon.
    /// Empty when the user turned the icon off.
    #[serde(default)]
    icon_path: String,
    /// Whether `icon_path` is a user-chosen icon rather than a discovered one.
    #[serde(default)]
    custom: bool,
    /// The user asked for no icon at all. Suppresses discovery so a repository
    /// with a favicon in its files stays on the plain tab marker.
    #[serde(default)]
    hidden: bool,
}

/// A repository path paired with the icon GitWyrm already knows about, for
/// screens that list many repositories at once.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CachedRepoIcon {
    pub repo_path: String,
    pub data_url: String,
    pub custom: bool,
}

fn icon_index_path(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    Ok(custom_icon_dir(app)?.join("index.json"))
}

fn read_icon_index(app: &tauri::AppHandle) -> HashMap<String, CachedIcon> {
    let Ok(path) = icon_index_path(app) else {
        return HashMap::new();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_icon_index(app: &tauri::AppHandle, index: &HashMap<String, CachedIcon>) {
    // A lost index only costs one rescan per repository, so a write failure is
    // never worth failing the caller's command over.
    if let Ok(path) = icon_index_path(app) {
        if let Ok(json) = serde_json::to_string(index) {
            let _ = fs::write(path, json);
        }
    }
}

fn remember_icon(app: &tauri::AppHandle, repo_path: &str, icon: Option<&RepoIcon>) {
    let mut index = read_icon_index(app);
    let key = repo_key(repo_path);
    let changed = match icon {
        Some(icon) => {
            let entry = CachedIcon {
                icon_path: icon.source_path.clone(),
                custom: icon.custom,
                hidden: false,
            };
            match index.get(&key) {
                Some(current)
                    if current.icon_path == entry.icon_path
                        && current.custom == entry.custom
                        && !current.hidden =>
                {
                    false
                }
                _ => {
                    index.insert(key, entry);
                    true
                }
            }
        }
        None => index.remove(&key).is_some(),
    };
    if changed {
        write_icon_index(app, &index);
    }
}

/// Record that this repository should show no icon at all, so discovery stops
/// re-finding one on the next open.
fn remember_hidden_icon(app: &tauri::AppHandle, repo_path: &str) {
    let mut index = read_icon_index(app);
    let key = repo_key(repo_path);
    if index.get(&key).is_some_and(|entry| entry.hidden) {
        return;
    }
    index.insert(
        key,
        CachedIcon {
            icon_path: String::new(),
            custom: false,
            hidden: true,
        },
    );
    write_icon_index(app, &index);
}

fn icon_is_hidden(app: &tauri::AppHandle, repo_path: &str) -> bool {
    read_icon_index(app)
        .get(&repo_key(repo_path))
        .is_some_and(|entry| entry.hidden)
}

/// Drop every remembered icon whose repository path hashes into `keys`.
pub fn forget_icon_keys(app: &tauri::AppHandle, repo_paths: &[String]) {
    if repo_paths.is_empty() {
        return;
    }
    let mut index = read_icon_index(app);
    let mut changed = false;
    for repo_path in repo_paths {
        if index.remove(&repo_key(repo_path)).is_some() {
            changed = true;
        }
        if let Ok(paths) = custom_icon_paths(app, repo_path) {
            for path in paths {
                if path.exists() && fs::remove_file(&path).is_ok() {
                    changed = true;
                }
            }
        }
    }
    if changed {
        write_icon_index(app, &index);
    }
}

fn supported_extension(path: &Path) -> Option<&'static str> {
    match path
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "ico" => Some("image/x-icon"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn file_data_url(path: &Path, custom: bool, max_bytes: u64) -> Result<RepoIcon, AppError> {
    let mime = supported_extension(path).ok_or_else(|| {
        AppError::Other("Choose a PNG, JPG, WebP, GIF, ICO, or SVG image.".into())
    })?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::Other("The selected icon is not a file.".into()));
    }
    if metadata.len() > max_bytes {
        return Err(AppError::Other(
            "That image is too large. Choose one under 10 MB.".into(),
        ));
    }
    let bytes = fs::read(path)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(RepoIcon {
        source_path: path.to_string_lossy().to_string(),
        label: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Repository icon".into()),
        data_url: format!("data:{mime};base64,{encoded}"),
        custom,
    })
}

fn custom_icon_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Other(error.to_string()))?
        .join("repo-icons");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn repo_key(repo_path: &str) -> String {
    // FNV-1a keeps the filename stable across Rust releases. The path itself is
    // never exposed in the app-data filename.
    //
    // Separators, case, and a trailing separator are all normalized away first:
    // an open repository reports its path with a trailing separator while the
    // frontend sends it without one, and the two must land on the same key or a
    // repository loses track of its icon.
    let normalized = repo_path.replace('/', "\\");
    let trimmed = normalized.trim_end_matches('\\');
    // Keep a bare drive root ("C:\") addressable rather than collapsing to "C:".
    let trimmed = if trimmed.is_empty() { &normalized } else { trimmed };

    let mut hash = 0xcbf29ce484222325u64;
    for byte in trimmed.to_ascii_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn custom_icon_paths(app: &tauri::AppHandle, repo_path: &str) -> Result<[PathBuf; 2], AppError> {
    let base = custom_icon_dir(app)?.join(repo_key(repo_path));
    Ok([base.with_extension("png"), base.with_extension("svg")])
}

fn existing_custom_icon(
    app: &tauri::AppHandle,
    repo_path: &str,
) -> Result<Option<PathBuf>, AppError> {
    if let Some(path) = custom_icon_paths(app, repo_path)?
        .into_iter()
        .find(|path| path.is_file())
    {
        return Ok(Some(path));
    }
    adopt_legacy_custom_icon(app, repo_path)
}

/// Icons saved before `repo_key` trimmed the trailing separator live under a
/// different filename. Rename the first one found to the current key so a user
/// who picked an icon in an older build keeps it.
fn adopt_legacy_custom_icon(
    app: &tauri::AppHandle,
    repo_path: &str,
) -> Result<Option<PathBuf>, AppError> {
    let normalized = repo_path.replace('/', "\\");
    if normalized.ends_with('\\') {
        return Ok(None);
    }
    let legacy_base = custom_icon_dir(app)?.join(legacy_repo_key(&format!("{normalized}\\")));
    let current = custom_icon_paths(app, repo_path)?;
    for (extension, target) in ["png", "svg"].into_iter().zip(current) {
        let legacy = legacy_base.with_extension(extension);
        if legacy.is_file() && fs::rename(&legacy, &target).is_ok() {
            return Ok(Some(target));
        }
    }
    Ok(None)
}

/// `repo_key` as it hashed before trailing separators were trimmed.
fn legacy_repo_key(repo_path: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in repo_path.replace('/', "\\").to_ascii_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn is_skipped_directory(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        ".git"
            | ".next"
            | ".nuxt"
            | ".svelte-kit"
            | "build"
            | "coverage"
            | "dist"
            | "node_modules"
            | "out"
            | "target"
            | "vendor"
    )
}

fn icon_score(root: &Path, path: &Path) -> Option<u32> {
    supported_extension(path)?;
    let stem = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
    let name_score = if stem == "favicon" {
        0
    } else if stem.contains("favicon") {
        10
    } else if matches!(stem.as_str(), "app-icon" | "app_icon" | "appicon") {
        20
    } else if stem == "logo" {
        30
    } else if stem.ends_with("-logo") || stem.ends_with("_logo") {
        38
    } else if stem == "icon" {
        45
    } else if stem.ends_with("-icon") || stem.ends_with("_icon") {
        52
    } else {
        return None;
    };

    let relative = path.strip_prefix(root).unwrap_or(path);
    let depth = relative.components().count().saturating_sub(1) as u32;
    let parent = relative
        .parent()
        .map(|value| {
            value
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    let directory_score = if parent.is_empty() {
        0
    } else if matches!(
        parent.as_str(),
        "public" | "static" | "assets" | "icons" | "images"
    ) {
        2
    } else if parent.ends_with("/public")
        || parent.ends_with("/static")
        || parent.ends_with("/assets")
    {
        5
    } else {
        15
    };
    Some(name_score + directory_score + depth * 3)
}

fn collect_icon_paths(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    if !root.is_dir() {
        return Err(AppError::Other(
            "That repository folder is no longer available.".into(),
        ));
    }

    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut ranked = Vec::<(u32, PathBuf)>::new();
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < MAX_SCAN_DEPTH
                    && !is_skipped_directory(&entry.file_name().to_string_lossy())
                {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() > MAX_DISCOVERED_BYTES {
                continue;
            }
            if let Some(score) = icon_score(root, &path) {
                ranked.push((score, path));
            }
        }
    }
    ranked.sort_by(|(left_score, left_path), (right_score, right_path)| {
        left_score.cmp(right_score).then_with(|| {
            left_path
                .to_string_lossy()
                .cmp(&right_path.to_string_lossy())
        })
    });
    ranked.truncate(MAX_RESULTS);
    Ok(ranked.into_iter().map(|(_, path)| path).collect())
}

fn find_repo_icons_sync(repo_path: String) -> Result<Vec<RepoIcon>, AppError> {
    let root = PathBuf::from(repo_path);
    Ok(collect_icon_paths(&root)?
        .into_iter()
        .filter_map(|path| {
            let mut icon = file_data_url(&path, false, MAX_DISCOVERED_BYTES).ok()?;
            icon.label = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            Some(icon)
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn find_repo_icons(repo_path: String) -> Result<Vec<RepoIcon>, AppError> {
    tauri::async_runtime::spawn_blocking(move || find_repo_icons_sync(repo_path))
        .await
        .map_err(|error| AppError::Other(error.to_string()))?
}

fn get_repo_icon_sync(
    app: &tauri::AppHandle,
    repo_path: String,
) -> Result<Option<RepoIcon>, AppError> {
    let _timing = crate::perf::CommandTiming::start("get_repo_icon", "icon.load");
    if icon_is_hidden(app, &repo_path) {
        return Ok(None);
    }
    if let Some(path) = existing_custom_icon(&app, &repo_path)? {
        let icon = file_data_url(&path, true, MAX_CUSTOM_BYTES)?;
        remember_icon(app, &repo_path, Some(&icon));
        return Ok(Some(icon));
    }
    let root = PathBuf::from(&repo_path);
    let Some(path) = collect_icon_paths(&root)?.into_iter().next() else {
        // A repository with no icon is remembered as such by dropping any stale
        // entry, so the open-a-repository screen shows its folder glyph.
        remember_icon(app, &repo_path, None);
        return Ok(None);
    };
    let icon = file_data_url(&path, false, MAX_DISCOVERED_BYTES)?;
    remember_icon(app, &repo_path, Some(&icon));
    Ok(Some(icon))
}

#[tauri::command]
#[specta::specta]
pub async fn get_repo_icon(
    app: tauri::AppHandle,
    repo_path: String,
) -> Result<Option<RepoIcon>, AppError> {
    tauri::async_runtime::spawn_blocking(move || get_repo_icon_sync(&app, repo_path))
        .await
        .map_err(|error| AppError::Other(error.to_string()))?
}

/// Icons for repositories GitWyrm has opened before, read straight from the
/// remembered locations. Never scans a worktree: an unknown repository, a
/// deleted icon file, or a deleted repository each fall out of the result
/// immediately, so listing hundreds of entries stays cheap. An icon that has
/// gone missing is forgotten here too, letting the next real open rediscover it.
fn get_cached_repo_icons_sync(
    app: &tauri::AppHandle,
    repo_paths: Vec<String>,
) -> Vec<CachedRepoIcon> {
    let mut index = read_icon_index(app);
    let mut icons = Vec::new();
    let mut forgotten = false;

    for repo_path in repo_paths {
        let key = repo_key(&repo_path);
        let Some(entry) = index.get(&key) else {
            continue;
        };
        if entry.hidden {
            continue;
        }
        let max_bytes = if entry.custom {
            MAX_CUSTOM_BYTES
        } else {
            MAX_DISCOVERED_BYTES
        };
        match file_data_url(Path::new(&entry.icon_path), entry.custom, max_bytes) {
            Ok(icon) => icons.push(CachedRepoIcon {
                repo_path,
                data_url: icon.data_url,
                custom: icon.custom,
            }),
            Err(_) => {
                if entry.custom {
                    // The app-owned copy is gone, so the manual choice is gone
                    // with it. Clearing it returns the repository to automatic.
                    let _ = clear_custom_icon_files(app, &repo_path);
                }
                index.remove(&key);
                forgotten = true;
            }
        }
    }

    if forgotten {
        write_icon_index(app, &index);
    }
    icons
}

#[tauri::command]
#[specta::specta]
pub async fn get_cached_repo_icons(
    app: tauri::AppHandle,
    repo_paths: Vec<String>,
) -> Result<Vec<CachedRepoIcon>, AppError> {
    tauri::async_runtime::spawn_blocking(move || get_cached_repo_icons_sync(&app, repo_paths))
        .await
        .map_err(|error| AppError::Other(error.to_string()))
}

fn set_repo_icon_sync(
    app: &tauri::AppHandle,
    repo_path: String,
    source_path: String,
) -> Result<RepoIcon, AppError> {
    let source = PathBuf::from(source_path);
    let mime = supported_extension(&source).ok_or_else(|| {
        AppError::Other("Choose a PNG, JPG, WebP, GIF, ICO, or SVG image.".into())
    })?;
    let metadata = fs::metadata(&source)?;
    if !metadata.is_file() {
        return Err(AppError::Other("The selected icon is not a file.".into()));
    }
    if metadata.len() > MAX_CUSTOM_BYTES {
        return Err(AppError::Other(
            "That image is too large. Choose one under 10 MB.".into(),
        ));
    }

    let targets = custom_icon_paths(&app, &repo_path)?;
    clear_custom_icon_files(app, &repo_path)?;

    let target = if mime == "image/svg+xml" {
        fs::copy(&source, &targets[1])?;
        targets[1].clone()
    } else {
        let bytes = fs::read(&source)?;
        let image = image::load_from_memory(&bytes)
            .map_err(|_| AppError::Other("GitWyrm could not read that image.".into()))?;
        let cropped =
            image.resize_to_fill(RASTER_ICON_SIZE, RASTER_ICON_SIZE, FilterType::Lanczos3);
        cropped
            .save_with_format(&targets[0], image::ImageFormat::Png)
            .map_err(|error| AppError::Other(error.to_string()))?;
        targets[0].clone()
    };

    let icon = file_data_url(&target, true, MAX_CUSTOM_BYTES)?;
    remember_icon(app, &repo_path, Some(&icon));
    Ok(icon)
}

#[tauri::command]
#[specta::specta]
pub async fn set_repo_icon(
    app: tauri::AppHandle,
    repo_path: String,
    source_path: String,
) -> Result<RepoIcon, AppError> {
    tauri::async_runtime::spawn_blocking(move || set_repo_icon_sync(&app, repo_path, source_path))
        .await
        .map_err(|error| AppError::Other(error.to_string()))?
}

fn clear_custom_icon_files(app: &tauri::AppHandle, repo_path: &str) -> Result<(), AppError> {
    for path in custom_icon_paths(app, repo_path)? {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn clear_repo_icon_sync(
    app: &tauri::AppHandle,
    repo_path: String,
) -> Result<Option<RepoIcon>, AppError> {
    clear_custom_icon_files(app, &repo_path)?;
    // Drop any "no icon" choice too, so discovery is free to run again.
    remember_icon(app, &repo_path, None);
    get_repo_icon_sync(app, repo_path)
}

/// Turn the icon off entirely: forget the custom image and stop discovery from
/// picking one up again. The repository falls back to the plain tab marker.
fn hide_repo_icon_sync(app: &tauri::AppHandle, repo_path: String) -> Result<(), AppError> {
    clear_custom_icon_files(app, &repo_path)?;
    remember_hidden_icon(app, &repo_path);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn hide_repo_icon(app: tauri::AppHandle, repo_path: String) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || hide_repo_icon_sync(&app, repo_path))
        .await
        .map_err(|error| AppError::Other(error.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn clear_repo_icon(
    app: tauri::AppHandle,
    repo_path: String,
) -> Result<Option<RepoIcon>, AppError> {
    tauri::async_runtime::spawn_blocking(move || clear_repo_icon_sync(&app, repo_path))
        .await
        .map_err(|error| AppError::Other(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favicons_rank_before_general_logos() {
        let root = Path::new("C:\\repo");
        let favicon = root.join("public").join("favicon.png");
        let logo = root.join("logo.png");
        assert!(icon_score(root, &favicon).unwrap() < icon_score(root, &logo).unwrap());
    }

    #[test]
    fn generated_and_dependency_folders_are_skipped() {
        assert!(is_skipped_directory("node_modules"));
        assert!(is_skipped_directory("TARGET"));
        assert!(!is_skipped_directory("assets"));
    }

    #[test]
    fn unrelated_images_are_not_candidates() {
        assert!(icon_score(Path::new("C:\\repo"), Path::new("C:\\repo\\hero.png")).is_none());
    }

    #[test]
    fn a_hidden_entry_survives_a_reread_of_the_index() {
        // The "no icon" choice is the only entry with no icon path, so it has to
        // round-trip through JSON without being mistaken for a corrupt record.
        let mut index = HashMap::new();
        index.insert(
            repo_key("C:\\code\\app"),
            CachedIcon {
                icon_path: String::new(),
                custom: false,
                hidden: true,
            },
        );
        let json = serde_json::to_string(&index).unwrap();
        let restored: HashMap<String, CachedIcon> = serde_json::from_str(&json).unwrap();
        assert!(restored[&repo_key("C:\\code\\app")].hidden);
    }

    #[test]
    fn an_older_index_entry_is_not_hidden() {
        // Entries written before the "no icon" option existed have no `hidden`
        // field and must keep showing their icon.
        let entry: CachedIcon =
            serde_json::from_str(r#"{"icon_path":"C:\\code\\app\\favicon.png"}"#).unwrap();
        assert!(!entry.hidden);
        assert!(!entry.custom);
    }

    #[test]
    fn repo_key_ignores_separator_case_and_trailing_slash() {
        // An open repository reports "C:/code/app/" while the frontend sends
        // "C:\code\app"; both must find the same cached icon.
        let expected = repo_key("C:\\code\\app");
        assert_eq!(repo_key("C:/code/app"), expected);
        assert_eq!(repo_key("C:/code/app/"), expected);
        assert_eq!(repo_key("C:\\code\\app\\"), expected);
        assert_eq!(repo_key("c:\\CODE\\App"), expected);
        assert_ne!(repo_key("C:\\code\\app2"), expected);
    }

    #[test]
    fn repo_key_keeps_a_drive_root_addressable() {
        // Trimming must not reduce "C:\" to "C:", which would collide with the
        // drive-relative path of the same name.
        assert!(!repo_key("C:\\").is_empty());
        assert_eq!(repo_key("C:\\"), repo_key("C:/"));
    }
}
