use tauri::State;

use crate::error::AppError;
use crate::git::submodule::moved_submodules;
use crate::git::types::SubmoduleMove;
use crate::state::RepoManager;

/// Every submodule whose checked-out commit differs from the commit the parent
/// repo pins (plus uninitialized ones). Empty when all submodules are in sync.
#[tauri::command]
#[specta::specta]
pub async fn list_submodules(
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
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let mut sub = repo.find_submodule(&path)?;
    sub.update(init, None)?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
