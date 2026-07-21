//! Repository tab icons: lightweight discovery inside a worktree plus a small,
//! app-owned custom-icon cache. Settings stay text-only; image bytes cross the
//! command boundary as data URLs only while the app is running.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use image::imageops::FilterType;
use serde::Serialize;
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
    let mut hash = 0xcbf29ce484222325u64;
    for byte in repo_path.replace('/', "\\").to_ascii_lowercase().bytes() {
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
    Ok(custom_icon_paths(app, repo_path)?
        .into_iter()
        .find(|path| path.is_file()))
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
    if let Some(path) = existing_custom_icon(&app, &repo_path)? {
        return file_data_url(&path, true, MAX_CUSTOM_BYTES).map(Some);
    }
    let root = PathBuf::from(repo_path);
    let Some(path) = collect_icon_paths(&root)?.into_iter().next() else {
        return Ok(None);
    };
    file_data_url(&path, false, MAX_DISCOVERED_BYTES).map(Some)
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
    for target in &targets {
        if target.exists() {
            fs::remove_file(target)?;
        }
    }

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

    file_data_url(&target, true, MAX_CUSTOM_BYTES)
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

fn clear_repo_icon_sync(
    app: &tauri::AppHandle,
    repo_path: String,
) -> Result<Option<RepoIcon>, AppError> {
    for path in custom_icon_paths(&app, &repo_path)? {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    let root = PathBuf::from(repo_path);
    let Some(path) = collect_icon_paths(&root)?.into_iter().next() else {
        return Ok(None);
    };
    file_data_url(&path, false, MAX_DISCOVERED_BYTES).map(Some)
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
}
