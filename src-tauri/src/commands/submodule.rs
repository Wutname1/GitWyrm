use tauri::State;

use crate::error::AppError;
use crate::git::shell::run_git;
use crate::git::submodule::{all_submodules, moved_submodules};
use crate::git::types::{SubmoduleMove, SubmoduleStatus};
use crate::state::RepoManager;

/// Reject a path that git would read as an option, and normalize Windows
/// backslashes to the forward slashes git records in .gitmodules.
fn clean_path(path: &str) -> Result<String, AppError> {
    let cleaned = path.trim().replace('\\', "/");
    if cleaned.is_empty() {
        return Err(AppError::Other("submodule path is required".into()));
    }
    if cleaned.starts_with('-') {
        return Err(AppError::Other(
            "submodule path cannot start with '-'".into(),
        ));
    }
    Ok(cleaned)
}

/// The name `.gitmodules` records for a submodule at `path`, which can differ
/// from the path itself. None when there is no matching entry.
fn submodule_name_for_path(repo_path: &str, path: &str) -> Option<String> {
    let out = run_git(
        Some(repo_path),
        &[
            "config",
            "--file",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ],
    )
    .ok()?;

    out.stdout.lines().find_map(|line| {
        let (key, value) = line.split_once(' ')?;
        if value.trim() != path {
            return None;
        }
        // "submodule.<name>.path" -> "<name>", where the name may contain dots.
        key.strip_prefix("submodule.")
            .and_then(|rest| rest.strip_suffix(".path"))
            .map(str::to_string)
    })
}

/// Every submodule in the repo, in sync or not. The list view's source.
#[tauri::command]
#[specta::specta]
pub async fn list_submodules(
    manager: State<'_, RepoManager>,
    repo_id: String,
) -> Result<Vec<SubmoduleStatus>, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        Ok(all_submodules(&repo))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// The absolute working directory of a submodule, for opening it as its own
/// repository tab.
///
/// Resolves through libgit2's submodule handle rather than joining the path
/// onto the parent, because `Repository::discover` walks *upward*: an empty or
/// half-checked-out submodule folder would otherwise resolve to the parent
/// repository and open a confusing duplicate tab. Opening it here first turns
/// that case into an error the caller can show.
#[tauri::command]
#[specta::specta]
pub async fn submodule_workdir(
    manager: State<'_, RepoManager>,
    repo_id: String,
    path: String,
) -> Result<String, AppError> {
    let open = manager.get(&repo_id)?;
    let path = clean_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        let sub = repo
            .find_submodule(&path)
            .map_err(|_| AppError::Other(format!("{path} is not a submodule of this project")))?;
        let nested = sub.open().map_err(|_| {
            AppError::Other(format!(
                "{path} has not been downloaded yet, so there is nothing to open"
            ))
        })?;
        let workdir = nested
            .workdir()
            .ok_or_else(|| AppError::Other(format!("{path} has no working folder to open")))?
            .to_path_buf();
        Ok(workdir.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Only the submodules that are out of sync with what the parent records.
/// Kept separate from [`list_submodules`] because the changed-files view asks a
/// narrower question than the sidebar list does.
#[tauri::command]
#[specta::specta]
pub async fn list_moved_submodules(
    manager: State<'_, RepoManager>,
    repo_id: String,
) -> Result<Vec<SubmoduleMove>, AppError> {
    let open = manager.get(&repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        let mut moves: Vec<SubmoduleMove> = moved_submodules(&repo).into_values().collect();
        moves.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(moves)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Snap a submodule back to the commit the parent repo records -- the submodule
/// equivalent of discarding changes. `init` also checks out an uninitialized
/// submodule for the first time. The recorded commit is fetched only if it is
/// not already present locally.
#[tauri::command]
#[specta::specta]
pub async fn update_submodule(
    manager: State<'_, RepoManager>,
    repo_id: String,
    path: String,
    init: bool,
) -> Result<(), AppError> {
    let open = manager.get(&repo_id)?;
    let repo_path = open.path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let path = clean_path(&path)?;
        // Shell git rather than libgit2's `Submodule::update`: that one checks
        // out the recorded commit but never fetches, so it fails outright when
        // the commit is not in the nested clone yet -- exactly the case after
        // someone else bumps the pointer. `git submodule update` fetches first.
        let mut args = vec!["submodule", "update"];
        if init {
            args.push("--init");
        }
        args.extend_from_slice(&["--", &path]);
        run_git(Some(&repo_path), &args)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Move a submodule to the newest commit on the branch it follows, and stage
/// the new pointer so the bump is ready to commit.
///
/// This is `git submodule update --remote`: it fetches the submodule's remote
/// first, so it needs the network. Staging afterwards means the user sees a
/// staged change they can commit, rather than a bare working-tree edit whose
/// origin is unclear.
#[tauri::command]
#[specta::specta]
pub async fn bump_submodule(
    manager: State<'_, RepoManager>,
    repo_id: String,
    path: String,
) -> Result<(), AppError> {
    let open = manager.get(&repo_id)?;
    let repo_path = open.path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let path = clean_path(&path)?;
        run_git(
            Some(&repo_path),
            &["submodule", "update", "--remote", "--init", "--", &path],
        )?;
        // Stage the moved pointer; the bump is not useful until it is committed.
        run_git(Some(&repo_path), &["add", "--", &path])?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Download and check out every submodule that is not initialized yet, the
/// fresh-clone case where submodule folders exist but are empty.
#[tauri::command]
#[specta::specta]
pub async fn init_all_submodules(
    manager: State<'_, RepoManager>,
    repo_id: String,
) -> Result<(), AppError> {
    let open = manager.get(&repo_id)?;
    let repo_path = open.path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        run_git(
            Some(&repo_path),
            &["submodule", "update", "--init", "--recursive"],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Add a new submodule at `path` pointing at `url`, optionally following a
/// branch. Leaves the addition staged, which is how `git submodule add` works.
#[tauri::command]
#[specta::specta]
pub async fn add_submodule(
    manager: State<'_, RepoManager>,
    repo_id: String,
    url: String,
    path: String,
    branch: Option<String>,
) -> Result<(), AppError> {
    let open = manager.get(&repo_id)?;
    let repo_path = open.path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let path = clean_path(&path)?;
        let url = url.trim().to_string();
        if url.is_empty() {
            return Err(AppError::Other("submodule URL is required".into()));
        }
        if url.starts_with('-') {
            return Err(AppError::Other(
                "submodule URL cannot start with '-'".into(),
            ));
        }

        let branch = branch
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty());
        let mut args: Vec<&str> = vec!["submodule", "add"];
        if let Some(b) = branch.as_deref() {
            if b.starts_with('-') {
                return Err(AppError::Other("branch name cannot start with '-'".into()));
            }
            args.push("-b");
            args.push(b);
        }
        // `--` keeps a URL or path that looks like a flag from being read as one.
        args.extend_from_slice(&["--", &url, &path]);

        run_git(Some(&repo_path), &args)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Remove a submodule: stop tracking it, drop it from .gitmodules and the
/// index, and clear the cached git data so the same path can be re-added later.
///
/// `delete_files` also removes the checked-out folder from disk. The removal is
/// left staged, matching `git rm`.
#[tauri::command]
#[specta::specta]
pub async fn remove_submodule(
    manager: State<'_, RepoManager>,
    repo_id: String,
    path: String,
    delete_files: bool,
) -> Result<(), AppError> {
    let open = manager.get(&repo_id)?;
    let repo_path = open.path.to_string_lossy().into_owned();
    let root = open.path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = clean_path(&path)?;

        // deinit clears the working copy and the submodule's config entry. It fails
        // on an already-deinitialized submodule, which is fine to continue past --
        // the goal is only that it is not initialized by the time git rm runs.
        let _ = run_git(
            Some(&repo_path),
            &["submodule", "deinit", "-f", "--", &path],
        );

        // Drop the index entry (and the folder, when asked).
        if delete_files {
            run_git(Some(&repo_path), &["rm", "-f", "--", &path])?;
        } else {
            run_git(Some(&repo_path), &["rm", "--cached", "--", &path])?;
        }

        // `git rm` strips the .gitmodules stanza itself, but the section can survive
        // when the path and the submodule's name differ. Removing it by name is
        // best-effort: "no such section" just means git already handled it.
        let gitmodules = root.join(".gitmodules");
        if gitmodules.exists() {
            if let Some(name) = submodule_name_for_path(&repo_path, &path) {
                let _ = run_git(
                    Some(&repo_path),
                    &[
                        "config",
                        "--file",
                        ".gitmodules",
                        "--remove-section",
                        &format!("submodule.{name}"),
                    ],
                );
            }

            // git leaves a 0-byte .gitmodules behind once the last submodule is gone.
            // Committing a blank file is noise, so drop it; otherwise stage the edit
            // so the whole removal lands in one commit.
            let empty = std::fs::read_to_string(&gitmodules)
                .map(|c| c.trim().is_empty())
                .unwrap_or(false);
            if empty {
                let _ = std::fs::remove_file(&gitmodules);
                let _ = run_git(Some(&repo_path), &["rm", "--cached", "--", ".gitmodules"]);
            } else {
                let _ = run_git(Some(&repo_path), &["add", "--", ".gitmodules"]);
            }
        }

        // The submodule's entry in .git/config outlives deinit in some states;
        // clearing it stops git treating the path as a submodule.
        let _ = run_git(
            Some(&repo_path),
            &["config", "--remove-section", &format!("submodule.{path}")],
        );

        // Stale data under .git/modules would block re-adding the same path later.
        let cached = root.join(".git").join("modules").join(&path);
        if cached.exists() {
            let _ = std::fs::remove_dir_all(&cached);
        }

        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
