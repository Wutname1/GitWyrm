pub mod ai;
pub mod ai_commits;
pub mod airun;
pub mod app;
pub mod branch;
pub mod commit;
pub mod diff;
pub mod editors;
pub mod external;
pub mod file;
pub mod github;
pub mod gitignore;
pub mod log;
pub mod merge;
pub mod opencode;
pub mod openspec;
pub mod patch;
pub mod profiles;
pub mod remote;
pub mod repo;
pub mod repo_icon;
pub mod scan;
pub mod shell_integration;
pub mod signing;
pub mod spec_desk;
pub mod spec_link;
pub mod staging;
pub mod stash;
pub mod status;
pub mod submodule;
pub mod tutorial;
pub mod updates;
pub mod worktree;

/// A progress sink that emits `git-progress` events for a local operation.
///
/// Lives here rather than in `git::progress` so that module stays free of Tauri
/// types -- linking `tauri` under `git/` breaks the lib test harness.
pub fn progress_sink(
    app: tauri::AppHandle,
) -> std::sync::Arc<dyn Fn(crate::git::progress::ProgressUpdate<'_>) + Send + Sync> {
    use tauri::Emitter;
    std::sync::Arc::new(move |u: crate::git::progress::ProgressUpdate<'_>| {
        let _ = app.emit(
            "git-progress",
            crate::commands::remote::GitProgressPayload {
                repo_id: u.repo_id.to_string(),
                operation: u.operation.to_string(),
                line: u.line,
                // Zero total means "size not known yet"; the frontend shows an
                // indeterminate indicator rather than a 0% bar.
                completed: (u.total > 0).then_some(u.completed),
                total: (u.total > 0).then_some(u.total),
            },
        );
    })
}
