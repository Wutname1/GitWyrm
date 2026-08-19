//! Tauri commands for branch-to-change links.
//!
//! Thin wrappers over `crate::git::spec_link`. The link itself is git config,
//! so these are cheap; the only real work is deciding which branch a feature
//! branch was cut from, so inference does not walk into shared history.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::error::AppError;
use crate::git::spec_link::{self, LinkSource};
use crate::state::RepoManager;

/// A branch's link to an OpenSpec change, as the UI sees it.
///
/// Fields stay snake_case, matching every other type crossing this boundary.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct BranchSpecLink {
    pub change_id: String,
    /// True when the user linked this branch, false when it was read from
    /// `Spec:` trailers. The UI words the two differently: an inferred link is
    /// a guess the user can make explicit.
    pub explicit: bool,
}

/// Best guess at the branch `branch` was cut from, used to keep inference off
/// shared history.
///
/// Prefers the remote's default branch (`origin/HEAD`), then the branch's own
/// upstream. Returns None when neither is known, in which case inference walks
/// the whole branch -- acceptable, since a repo with no remote and no upstream
/// has no shared history to confuse it with.
fn base_branch(repo: &git2::Repository, branch: &str) -> Option<String> {
    if let Ok(head) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let Ok(Some(target)) = head.symbolic_target() {
            if let Some(name) = target.strip_prefix("refs/remotes/") {
                if name != branch {
                    return Some(name.to_string());
                }
            }
        }
    }

    let local = repo.find_branch(branch, git2::BranchType::Local).ok()?;
    let upstream = local.upstream().ok()?;
    let name = upstream.name().ok().flatten()?.to_string();
    if name == branch {
        None
    } else {
        Some(name)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn spec_link_get(
    manager: State<'_, RepoManager>,
    repo_id: String,
    branch: String,
) -> Result<Option<BranchSpecLink>, AppError> {
    let open = manager.get(&repo_id)?;
    let path = open.path.to_string_lossy().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let base = {
            let repo = open.repo.lock().unwrap();
            base_branch(&repo, &branch)
        };
        let link = spec_link::resolve(&path, &branch, base.as_deref())?;
        Ok(link.map(|link| BranchSpecLink {
            change_id: link.change_id,
            explicit: link.source == LinkSource::Explicit,
        }))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn spec_link_set(
    manager: State<'_, RepoManager>,
    repo_id: String,
    branch: String,
    change_id: String,
) -> Result<(), AppError> {
    let open = manager.get(&repo_id)?;
    let path = open.path.to_string_lossy().to_string();
    tauri::async_runtime::spawn_blocking(move || spec_link::set_link(&path, &branch, &change_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn spec_link_clear(
    manager: State<'_, RepoManager>,
    repo_id: String,
    branch: String,
) -> Result<(), AppError> {
    let open = manager.get(&repo_id)?;
    let path = open.path.to_string_lossy().to_string();
    tauri::async_runtime::spawn_blocking(move || spec_link::clear_link(&path, &branch))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
}
