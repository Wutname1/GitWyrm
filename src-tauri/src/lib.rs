mod ai;
mod commands;
mod error;
mod git;
mod settings;
mod state;
mod watcher;

pub use error::AppError;
pub use git::graph as git_graph;
pub use git::merge_ops as git_merge_ops;
pub use git::types as git_types;
pub use git::refs as git_refs;
pub use git::submodule as git_submodule;

use state::RepoManager;
use watcher::WatcherRegistry;

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
  tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
    commands::app::build_info,
    commands::app::read_log,
    commands::app::clear_log,
    commands::app::open_logs_folder,
    commands::external::reveal_in_file_manager,
    commands::external::open_in_editor,
    commands::external::open_in_terminal,
    settings::get_settings,
    settings::save_settings,
    commands::repo::open_repo,
    commands::repo::close_repo,
    commands::repo::git_available,
    commands::log::get_log,
    commands::status::get_status,
    commands::branch::list_branches,
    commands::branch::branch_relation,
    commands::branch::list_tags,
    commands::stash::list_stashes,
    commands::diff::get_file_diff,
    commands::diff::get_commit_detail,
    commands::staging::stage_file,
    commands::staging::unstage_file,
    commands::staging::stage_all,
    commands::staging::unstage_all,
    commands::staging::discard_file,
    commands::staging::discard_all,
    commands::commit::create_commit,
    commands::branch::checkout_branch,
    commands::branch::create_branch,
    commands::branch::delete_branch,
    commands::branch::rename_branch,
    commands::branch::create_tag,
    commands::branch::delete_tag,
    commands::branch::reset_current,
    commands::branch::move_current_branch,
    commands::branch::checkout_commit,
    commands::branch::reword_commit,
    commands::branch::revert_commit,
    commands::branch::drop_commit,
    commands::branch::has_worktrees,
    commands::branch::commit_web_url,
    commands::stash::stash_save,
    commands::stash::stash_pop,
    commands::submodule::list_submodules,
    commands::submodule::update_submodule,
    commands::remote::git_fetch,
    commands::remote::git_pull,
    commands::remote::git_push,
    commands::remote::git_push_branch,
    commands::remote::git_pull_branch,
    commands::remote::set_branch_upstream,
    commands::remote::git_push_force,
    commands::remote::git_rebase,
    commands::remote::rebase_continue,
    commands::remote::rebase_abort,
    commands::remote::git_clone,
    commands::remote::list_remotes,
    commands::remote::add_remote,
    commands::remote::rename_remote,
    commands::remote::set_remote_url,
    commands::remote::remove_remote,
    commands::remote::set_upstream,
    commands::merge::merge_analysis,
    commands::merge::merge_branch,
    commands::merge::merge_directional,
    commands::merge::get_merge_state,
    commands::merge::abort_merge,
    commands::merge::get_conflict,
    commands::merge::resolve_conflict,
    commands::merge::commit_merge,
    commands::merge::cherry_pick,
    commands::patch::stage_lines,
    commands::patch::unstage_lines,
    commands::patch::discard_lines,
    commands::scan::scan_code_folder,
    commands::ai::ai_get_catalog,
    commands::ai::ai_list_configured,
    commands::ai::ai_set_api_key,
    commands::ai::ai_remove_provider,
    commands::ai::ai_list_models,
    commands::ai::ai_default_instruction,
    commands::ai::ai_copilot_device_start,
    commands::ai::ai_copilot_device_poll,
    commands::ai::generate_commit_message,
    commands::ai_commits::generate_commits,
    commands::github::github_device_start,
    commands::github::github_device_poll,
    commands::github::github_sign_out,
    commands::github::github_auth_status,
    commands::github::github_repo_slug,
    commands::github::github_list_prs,
    commands::github::github_list_issues,
    commands::github::github_pr_detail,
    commands::github::github_issue_detail,
    commands::github::github_comment,
    commands::github::github_approve_pr,
    commands::github::github_merge_pr,
    commands::github::github_close_issue,
  ])
  .typ::<watcher::RepoChangedPayload>()
  .typ::<commands::remote::GitProgressPayload>()
}

const SENTRY_DSN: &str = "https://5cb301777a6d45efd4ddba81136bc6c9@o4511760444686336.ingest.us.sentry.io/4511760446717952";

/// Starts crash reporting and observability. The returned guard flushes pending
/// events on drop, so it has to stay alive for the whole process. Debug builds
/// are skipped so local crashes stay local.
///
/// During the alpha this mirrors the frontend `initSentry`: everything on, full
/// sampling, even the paid-tier features. `traces_sample_rate` is the dial to
/// turn down once the free-plan quota gets tight. See the `ALPHA:` comments.
fn init_sentry() -> Option<sentry::ClientInitGuard> {
  if cfg!(debug_assertions) {
    return None;
  }
  Some(sentry::init((
    SENTRY_DSN,
    sentry::ClientOptions {
      release: Some(env!("CARGO_PKG_VERSION").into()),
      environment: Some("alpha".into()),
      // Repo paths and branch names reach Sentry through panic messages, so
      // keep the extra user identifiers off.
      send_default_pii: false,
      // Report every panic as a Sentry event, not just the process-fatal ones.
      attach_stacktrace: true,
      // ALPHA: trace 100% of transactions. Drop toward 0.1-0.2 before launch,
      // or the free-plan performance quota burns out fast.
      traces_sample_rate: 1.0,
      max_breadcrumbs: 100,
      ..Default::default()
    },
  )))
}

pub fn run() {
  let _sentry = init_sentry();

  // Route panics through the logger so a backend crash lands in gitwyrm.log
  // (with location + payload) instead of only the detached dev terminal.
  let default_hook = std::panic::take_hook();
  std::panic::set_hook(Box::new(move |info| {
    let location = info
      .location()
      .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
      .unwrap_or_else(|| "unknown".into());
    let payload = info
      .payload()
      .downcast_ref::<&str>()
      .map(|s| s.to_string())
      .or_else(|| info.payload().downcast_ref::<String>().cloned())
      .unwrap_or_else(|| "<non-string panic payload>".into());
    let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
    // Write synchronously to a dedicated file next to the exe. The async log
    // plugin can be killed before it flushes when a spawn_blocking thread
    // aborts the process, so bypass it entirely for panics.
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open("gitwyrm-panic.log")
    {
      let _ = writeln!(f, "PANIC [thread {thread}] at {location}: {payload}");
    }
    default_hook(info);
  }));

  let builder = specta_builder();

  #[cfg(debug_assertions)]
  builder
    .export(
      specta_typescript::Typescript::default()
        .header("// @ts-nocheck\n// GENERATED by tauri-specta. Do not edit.\n"),
      "../src/lib/bindings.ts",
    )
    .expect("failed to export typescript bindings");

  tauri::Builder::default()
    .plugin(
      tauri_plugin_log::Builder::new()
        .targets([
          tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
          tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
            file_name: Some(commands::app::LOG_FILE_NAME.into()),
          }),
        ])
        .level(if cfg!(debug_assertions) {
          log::LevelFilter::Debug
        } else {
          log::LevelFilter::Info
        })
        .max_file_size(5_000_000)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
        .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
        .build(),
    )
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_process::init())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .manage(RepoManager::default())
    .manage(WatcherRegistry::default())
    .invoke_handler(builder.invoke_handler())
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
