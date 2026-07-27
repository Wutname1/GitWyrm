mod ai;
mod commands;
mod error;
mod git;
mod missing_repos;
mod scrub;
mod settings;
mod state;
mod watcher;

pub use error::AppError;
pub use git::graph as git_graph;
pub use git::history as git_history;
pub use git::merge_ops as git_merge_ops;
pub use git::types as git_types;
pub use git::refs as git_refs;
pub use git::submodule as git_submodule;

use state::RepoManager;
use tauri::{Emitter, Manager};
use watcher::WatcherRegistry;

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
  tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
    commands::app::build_info,
    commands::app::path_exists,
    commands::app::read_log,
    commands::app::clear_log,
    commands::app::open_logs_folder,
    commands::external::reveal_in_file_manager,
    commands::external::open_in_editor,
    commands::external::get_editor_availability,
    commands::external::open_solution_in_visual_studio,
    commands::external::open_in_terminal,
    commands::file::open_file_in_editor,
    commands::file::reveal_file_in_file_manager,
    commands::file::delete_file,
    commands::file::restore_file,
    commands::file::get_file_history,
    commands::file::get_file_blame,
    settings::get_settings,
    settings::save_settings,
    missing_repos::mark_repo_missing,
    commands::repo::open_repo,
    commands::repo::close_repo,
    commands::repo::git_available,
    commands::signing::get_git_identity,
    commands::signing::set_git_identity,
    commands::signing::git_tool_info,
    commands::signing::gpg_tool_info,
    commands::signing::get_signing_status,
    commands::signing::create_signing_key,
    commands::signing::export_signing_key,
    commands::signing::set_signing_enabled,
    commands::signing::delete_signing_key,
    commands::signing::list_ssh_keys,
    commands::signing::create_ssh_key,
    commands::signing::delete_ssh_key,
    commands::signing::read_ssh_public_key,
    commands::signing::enable_ssh_signing,
    commands::signing::use_gpg_signing,
    commands::signing::repair_signing_format,
    commands::profiles::list_profiles,
    commands::profiles::get_active_profile_id,
    commands::profiles::save_profile,
    commands::profiles::delete_profile,
    commands::profiles::set_active_profile,
    commands::profiles::set_repo_profile,
    commands::profiles::clear_repo_profile,
    commands::profiles::profile_from_current_config,
    commands::profiles::list_signing_keys,
    commands::profiles::get_effective_identity,
    commands::repo::git_init,
    commands::repo_icon::get_repo_icon,
    commands::repo_icon::get_cached_repo_icons,
    commands::repo_icon::find_repo_icons,
    commands::repo_icon::set_repo_icon,
    commands::repo_icon::clear_repo_icon,
    commands::repo_icon::hide_repo_icon,
    commands::log::get_log,
    commands::status::get_status,
    commands::branch::list_branches,
    commands::branch::branch_relation,
    commands::branch::list_tags,
    commands::stash::list_stashes,
    commands::diff::get_file_diff,
    commands::diff::get_commit_detail,
    commands::staging::stage_file,
    commands::staging::stage_files,
    commands::staging::unstage_file,
    commands::staging::unstage_files,
    commands::staging::stage_all,
    commands::staging::unstage_all,
    commands::staging::discard_file,
    commands::staging::discard_files,
    commands::staging::discard_all,
    commands::gitignore::add_to_gitignore,
    commands::commit::create_commit,
    commands::branch::checkout_branch,
    commands::branch::create_branch,
    commands::branch::delete_branch,
    commands::branch::rename_branch,
    commands::branch::create_tag,
    commands::branch::delete_tag,
    commands::branch::reset_current,
    commands::branch::reset_current_to_ref,
    commands::branch::move_current_branch,
    commands::branch::fast_forward_branch,
    commands::branch::checkout_commit,
    commands::branch::reword_commit,
    commands::branch::revert_commit,
    commands::branch::drop_commit,
    commands::branch::squash_commits,
    commands::branch::drop_commits,
    commands::branch::has_worktrees,
    commands::branch::commit_web_url,
    commands::stash::stash_save,
    commands::stash::stash_pop,
    commands::stash::stash_apply,
    commands::stash::stash_drop,
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
    commands::remote::remote_branch_web_url,
    commands::remote::list_remote_tags,
    commands::remote::unpushed_tags,
    commands::remote::push_tag,
    commands::remote::delete_remote_tag,
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
    commands::shell_integration::context_menu_registered,
    commands::shell_integration::set_context_menu_registered,
    commands::app::launch_repo_path,
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
    commands::github::github_list_repositories,
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
      // `send_default_pii: false` does not touch payloads: a panic message or a
      // logged error still carries whatever text built it. Every log::error!
      // becomes a Sentry event via SentryLogger, and those messages embed repo
      // paths, author emails, and provider error bodies. Scrub on the way out.
      before_send: Some(std::sync::Arc::new(|mut event: sentry::protocol::Event| {
        if let Some(message) = event.message.take() {
          event.message = Some(scrub::scrub_text(&message));
        }
        for exception in event.exception.iter_mut() {
          if let Some(value) = exception.value.take() {
            exception.value = Some(scrub::scrub_text(&value));
          }
        }
        for entry in event.logentry.iter_mut() {
          entry.message = scrub::scrub_text(&entry.message);
        }
        Some(event)
      })),
      before_breadcrumb: Some(std::sync::Arc::new(
        |mut crumb: sentry::protocol::Breadcrumb| {
          if let Some(message) = crumb.message.take() {
            crumb.message = Some(scrub::scrub_text(&message));
          }
          Some(crumb)
        },
      )),
      ..Default::default()
    },
  )))
}

/// Shared configuration for the log plugin: stdout + a rotating gitwyrm.log.
fn log_builder() -> tauri_plugin_log::Builder {
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
}

/// Write `src/lib/bindings.ts` using the same builder the app exports with.
/// Exposed so the bindings can be regenerated (and checked in CI) without
/// launching the whole app - `tauri dev` still exports on every start.
#[cfg(debug_assertions)]
pub fn export_bindings(out_path: &str) -> Result<(), String> {
  specta_builder()
    .export(
      specta_typescript::Typescript::default()
        .header("// @ts-nocheck\n// GENERATED by tauri-specta. Do not edit.\n"),
      out_path,
    )
    .map_err(|e| e.to_string())
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
    // Must be registered before any other plugin (Tauri's requirement). When
    // Explorer's right-click entry launches a second GitWyrm while one is
    // already running, this hands that process's arguments to the original and
    // exits, so the folder opens as a tab in the window the user already has
    // instead of starting a duplicate app with its own file watchers.
    .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
      let path = commands::app::repo_path_from_args(argv);
      // The UI is already running in this case, so the event has a listener and
      // no slot is needed.
      if let Some(path) = path {
        log::info!("Second launch asked for {path}; opening it in this window");
        let _ = app.emit("open-repo-path", path);
      }
      // Bring the existing window forward -- otherwise the click appears to do
      // nothing when GitWyrm is running but buried behind other windows.
      if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
      }
    }))
    // The log plugin is registered inside setup via `split()` so we can wrap
    // its logger with Sentry. `skip_logger()` keeps the plugin from claiming
    // the global logger slot before we get there.
    .plugin(log_builder().skip_logger().build())
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_process::init())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .setup(|app| {
      // `tauri_plugin_log` normally claims the global `log` logger for itself,
      // which left `log::error!` writing to gitwyrm.log and nothing else --
      // the backend Sentry project only ever saw panics. Splitting hands back
      // the plugin's logger instead of installing it, so we wrap it in a
      // SentryLogger and attach the pair. Records still reach the file and
      // stdout exactly as before; errors additionally become Sentry events,
      // and warn/info become breadcrumbs that give those events context.
      let (_plugin, max_level, logger) = log_builder().split(app.handle())?;
      let bridged = sentry_log::SentryLogger::with_dest(logger);
      tauri_plugin_log::attach_logger(max_level, Box::new(bridged))?;

      let info = commands::app::build_info();
      log::info!(
        "GitWyrm {} starting (build {}, git {}, debug {}) on {} {}",
        info.version,
        info.build_date,
        info.git_hash,
        info.debug,
        std::env::consts::OS,
        std::env::consts::ARCH,
      );

      // Tell the tool resolver where the bundled git and gpg live before any
      // shell-out happens. Resources sit under the install dir in a packaged
      // build and are simply absent in dev, where the system tools are used.
      let bundle_root = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("resources"))
        .filter(|dir| dir.is_dir());
      git::bundled::set_bundle_root(bundle_root);

      // Point git and gpg shell-outs at the saved executables (if any) before
      // the first command runs.
      settings::apply_startup_git_executable(app.handle());

      // Reconcile repositories whose folder has gone away, and forget the ones
      // that have been gone a week. Runs inline rather than on a background
      // thread: the frontend saves the whole settings object once it hydrates,
      // so a sweep still in flight at that moment would lose its write. Doing it
      // here keeps it strictly before the webview can issue any save. Cost is
      // one `is_dir` per known repository, so it stays off the critical path.
      missing_repos::sweep(app.handle());

      // Stash any folder Explorer passed us. The webview does not exist yet, so
      // this waits in a slot for the frontend to collect once it is ready.
      commands::app::set_pending_launch_path(commands::app::repo_path_from_args(
        std::env::args(),
      ));
      Ok(())
    })
    .manage(RepoManager::default())
    .manage(WatcherRegistry::default())
    .invoke_handler(builder.invoke_handler())
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
