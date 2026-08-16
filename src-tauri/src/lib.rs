mod ai;
mod airun;
mod commands;
mod error;
mod git;
mod hosting;
mod logs;
mod missing_repos;
mod openspec;
mod perf;
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
/// Exposed for the spec_links integration test, which drives linking and
/// committing against a real repository rather than parsed strings.
pub use git::spec_link as git_spec_link;
pub use git::submodule as git_submodule;
/// Exposed for the submodule integration test: "discard everything" is only
/// meaningful against a real parent repo with a real nested checkout.
pub use commands::staging::discard_everything;
pub use git::trailers as git_trailers;
pub use git::worktree as git_worktree;
/// Exposed for the ssh_config integration test: this rewrites a file the app
/// does not own, so it is tested against real config shapes.
pub use git::ssh::rewrite_config as ssh_config_rewrite;
/// Exposed for the openspec_real_tree integration test, which parses this
/// repository's own plan folder -- the only fixture guaranteed to match how the
/// OpenSpec CLI formats a change.
/// Exposed for the copilot_acp integration test, which drives the real CLI
/// through this client -- the only check that the transport works end to end.
pub use ai::agent::acp as agent_acp;
pub use ai::agent::copilot_cli as agent_copilot_cli;
/// Exposed for the archive-repair integration test, which drives a real
/// half-finished archive through diagnosis and repair -- the one path whose
/// value is entirely in what the CLI actually does.
pub use openspec::archive_repair;
pub use openspec::openspec_dir;
pub use openspec::parse as openspec_parse;
pub use openspec::write as openspec_write;

use state::RepoManager;
use tauri::{Emitter, Manager};
use watcher::WatcherRegistry;

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
  tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
    commands::app::build_info,
    commands::app::path_exists,
    commands::app::list_logs,
    commands::app::read_log_file,
    commands::app::read_log_tail,
    commands::app::clear_log,
    commands::app::open_logs_folder,
    commands::external::reveal_in_file_manager,
    commands::external::open_in_editor,
    commands::external::get_editor_availability,
    commands::external::open_solution_in_visual_studio,
    commands::external::open_in_terminal,
    commands::external::open_in_opencode,
    commands::opencode::opencode_available,
    commands::openspec::openspec_recheck_cli,
    commands::openspec::openspec_draft_change,
    commands::openspec::openspec_create_drafted_change,
    commands::openspec::openspec_ask,
    commands::openspec::openspec_diagnose_archive,
    commands::openspec::openspec_repair_archive,
    commands::openspec::openspec_draft_fix,
    commands::openspec::openspec_add_drafted_delta,
    commands::openspec::openspec_read_file,
    commands::openspec::openspec_write_file,
    commands::openspec::openspec_draft_edit,
    commands::openspec::openspec_delete_change,
    commands::file::open_file_in_editor,
    commands::file::reveal_file_in_file_manager,
    commands::file::open_folder_in_file_manager,
    commands::file::delete_file,
    commands::file::restore_file,
    commands::file::get_file_history,
    commands::file::get_file_blame,
    commands::file::get_file_content,
    settings::get_settings,
    settings::get_usage_telemetry_enabled,
    settings::save_settings,
    commands::updates::check_for_update,
    commands::updates::install_update,
    commands::updates::changelog_since,
    commands::updates::download_update,
    commands::updates::install_downloaded_update,
    commands::updates::update_endpoint,
    commands::updates::toolset_status,
    commands::updates::install_toolset,
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
    commands::signing::test_ssh_host,
    commands::signing::ssh_key_for_host,
    commands::signing::set_ssh_key_for_host,
    commands::signing::repair_signing_format,
    commands::profiles::list_profiles,
    commands::profiles::get_active_profile_id,
    commands::profiles::save_profile,
    commands::profiles::delete_profile,
    commands::profiles::set_active_profile,
    commands::profiles::set_repo_profile,
    commands::profiles::clear_repo_profile,
    commands::profiles::profile_from_current_config,
    commands::profiles::seed_profile_from_config,
    commands::profiles::add_profile_without_applying,
    commands::profiles::list_signing_keys,
    commands::profiles::get_effective_identity,
    commands::repo::git_init,
    commands::tutorial::create_tutorial_repo,
    commands::tutorial::discard_tutorial_repo,
    commands::repo_icon::get_repo_icon,
    commands::repo_icon::get_cached_repo_icons,
    commands::repo_icon::find_repo_icons,
    commands::repo_icon::set_repo_icon,
    commands::repo_icon::clear_repo_icon,
    commands::repo_icon::hide_repo_icon,
    commands::log::get_log,
    commands::log::get_commit_stats,
    commands::status::get_status,
    commands::status::get_repo_counts,
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
    commands::stash::stash_folder,
    commands::stash::stash_pop,
    commands::stash::stash_apply,
    commands::stash::stash_drop,
    commands::submodule::list_submodules,
    commands::submodule::list_moved_submodules,
    commands::submodule::update_submodule,
    commands::submodule::bump_submodule,
    commands::submodule::init_all_submodules,
    commands::submodule::add_submodule,
    commands::submodule::remove_submodule,
    commands::submodule::submodule_workdir,
    commands::worktree::list_worktrees,
    commands::worktree::worktree_dirty_count,
    commands::worktree::suggest_worktree_path,
    commands::worktree::check_worktree_path,
    commands::worktree::branch_holder,
    commands::worktree::worktree_copyable_files,
    commands::worktree::add_worktree,
    commands::worktree::remove_worktree,
    commands::worktree::prune_worktrees,
    commands::worktree::repair_worktree,
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
    commands::remote::delete_remote_branch,
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
    commands::scan::read_repo_readme,
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
    commands::ai::ai_copilot_account,
    commands::ai::generate_commit_message,
    commands::ai_commits::generate_commits,
    commands::github::hosting_providers,
    commands::github::repo_host_provider,
    commands::github::host_connect_token,
    commands::github::host_sign_out,
    commands::github::github_device_start,
    commands::github::github_device_poll,
    commands::github::github_sign_out,
    commands::github::github_auth_status,
    commands::github::github_list_repositories,
    commands::github::github_repo_slug,
    commands::github::github_list_prs,
    commands::github::github_list_issues,
    commands::github::github_pr_detail,
    commands::github::github_pr_files,
    commands::github::github_pr_commits,
    commands::github::github_issue_detail,
    commands::github::github_comment,
    commands::github::github_approve_pr,
    commands::github::github_merge_pr,
    commands::github::github_close_issue,
    commands::github::github_ssh_key_pairings,
    commands::airun::ai_run_start,
    commands::airun::ai_run_start_demo,
    commands::airun::ai_run_answer_gate,
    commands::airun::ai_run_note,
    commands::airun::ai_run_stop,
    commands::airun::ai_run_current,
    commands::airun::ai_run_clear,
    commands::airun::ai_run_discard_plan,
    commands::airun::ai_run_completion,
    commands::openspec::openspec_status,
    commands::openspec::openspec_list_changes,
    commands::openspec::openspec_get_change,
    commands::openspec::openspec_archived_ids,
    commands::openspec::openspec_change_history,
    commands::openspec::openspec_toggle_task,
    commands::openspec::openspec_scaffold_change,
    commands::openspec::openspec_validate_change,
    commands::openspec::openspec_archive_change,
    commands::openspec::openspec_default_archive_commit_template,
    commands::spec_desk::open_spec_desk,
    commands::spec_link::spec_link_get,
    commands::spec_link::spec_link_set,
    commands::spec_link::spec_link_clear,
  ])
  .typ::<watcher::RepoChangedPayload>()
  .typ::<commands::remote::GitProgressPayload>()
  // Event payloads have to be named explicitly: specta only exports types a
  // command mentions, and this one only ever travels as an emitted event. Left
  // out, the frontend imports a type that is not there and the build fails.
  .typ::<airun::RunEventKind>()
}

const SENTRY_DSN: &str = "https://5cb301777a6d45efd4ddba81136bc6c9@o4511760444686336.ingest.us.sentry.io/4511760446717952";

/// Starts crash reporting and observability. The returned guard flushes pending
/// events on drop, so it has to stay alive for the whole process. Debug builds
/// are skipped so local crashes stay local.
///
/// Two independent opt-outs feed this, and they gate different things:
/// - Crash reports (errors and panics) are what `crash_reports` controls. Off
///   means no client at all, so nothing is sent.
/// - Usage telemetry (performance traces) is what `usage_telemetry` controls.
///   Off zeroes the sample rate, which keeps crash reporting working while no
///   transaction is ever recorded or sent.
///
/// Mirrors the frontend `initSentry`, which makes the same split.
fn init_sentry() -> Option<sentry::ClientInitGuard> {
  if cfg!(debug_assertions) {
    return None;
  }
  // Opting out has to mean nothing is sent, so this is checked before the
  // client is built rather than filtered in `before_send` -- an initialized
  // client still opens a connection and buffers events.
  if !settings::crash_reports_enabled() {
    return None;
  }
  // Read once: this touches the disk, and the value decides two options below.
  let usage_telemetry = settings::usage_telemetry_enabled();
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
      // Performance tracing is usage telemetry, not crash reporting: 0.0 means
      // no transaction is recorded or sent, while panics and errors still go
      // out under `crash_reports`.
      //
      // ALPHA: when on, trace 100% of transactions. Drop toward 0.1-0.2 before
      // launch, or the free-plan performance quota burns out fast.
      traces_sample_rate: if usage_telemetry { 1.0 } else { 0.0 },
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

/// Shared configuration for the log plugin: level, timestamps, and stdout.
///
/// The file target is added separately, in setup, because opening the file is
/// fallible and needs the app handle to find the log folder. See `logs` for the
/// daily file itself -- none of the plugin's own rotation settings apply to it.
fn log_builder() -> tauri_plugin_log::Builder {
  tauri_plugin_log::Builder::new()
    .targets([tauri_plugin_log::Target::new(
      tauri_plugin_log::TargetKind::Stdout,
    )])
    .level(if cfg!(debug_assertions) {
      log::LevelFilter::Debug
    } else {
      log::LevelFilter::Info
    })
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

  // libgit2 refuses to open a repository whose directory belongs to another
  // Windows account ("dubious ownership"), which is routine for repos on a
  // second internal drive, an external drive, or a folder cloned under a
  // different profile. Git CLI only relaxes this for paths listed in
  // safe.directory, so those repos open in every other client the user has
  // configured but not here.
  //
  // Must run before any Repository::open/discover: this flips a libgit2 global,
  // and doing it while another thread is inside libgit2 would be a data race.
  // `run()` is single-threaded at this point and no repo has been touched yet.
  unsafe {
    if let Err(e) = git2::opts::set_verify_owner_validation(false) {
      log::warn!("could not disable repository owner validation: {e}");
    }
  }

  // Route panics through the logger so a backend crash lands in the log folder
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
    // Write synchronously to a dedicated file in the log folder. The log plugin
    // can be killed before it flushes when a spawn_blocking thread aborts the
    // process, so bypass it entirely for panics. Falls back to a file next to
    // the exe when the panic beats the logger to opening its own file.
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open(logs::panic_log_path())
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
    // Remember where each window was, per label -- so the main window and every
    // repository's Spec Desk keep their own placement.
    //
    // SIZE | POSITION | MAXIMIZED only, deliberately:
    // - DECORATIONS would restore a saved value over `decorations(false)`, and
    //   GitWyrm draws its own titlebars app-wide.
    // - VISIBLE could reopen a window hidden, leaving the app running with
    //   nothing on screen and no way back.
    // - FULLSCREEN is not something GitWyrm puts a window into.
    //
    // The plugin only restores a position that a currently-attached monitor
    // intersects, and otherwise lets the OS place the window, so a Desk saved on
    // a since-unplugged screen still comes back visible.
    .plugin(
      tauri_plugin_window_state::Builder::default()
        .with_state_flags(
          tauri_plugin_window_state::StateFlags::SIZE
            | tauri_plugin_window_state::StateFlags::POSITION
            | tauri_plugin_window_state::StateFlags::MAXIMIZED,
        )
        .build(),
    )
    .setup(|app| {
      // `tauri_plugin_log` normally claims the global `log` logger for itself,
      // which left `log::error!` writing to the log file and nothing else --
      // the backend Sentry project only ever saw panics. Splitting hands back
      // the plugin's logger instead of installing it, so we wrap it in a
      // SentryLogger and attach the pair. Records still reach the file and
      // stdout exactly as before; errors additionally become Sentry events,
      // and warn/info become breadcrumbs that give those events context.
      //
      // The daily file target is attached here too. If it cannot be opened
      // GitWyrm still starts, with stdout and Sentry as the only destinations --
      // a log folder we cannot write to is not a reason to refuse to run.
      let mut builder = log_builder();
      match logs::file_target(app.handle()) {
        Ok(target) => builder = builder.target(target),
        Err(e) => eprintln!("could not open the log file: {e}"),
      }
      let (_plugin, max_level, logger) = builder.split(app.handle())?;
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

      // Tell the tool resolver where git and gpg live before any shell-out
      // happens. They are downloaded rather than installed with the app, and
      // sit outside the install directory so an app update leaves them alone;
      // see git::toolset. Absent in dev and before the first download, where
      // the system tools are used instead.
      let bundle_root = git::toolset::toolset_dir().filter(|dir| dir.is_dir());
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

      // Drop log files past the retention window, and the oldest ones beyond
      // that if the folder is still too big. The writer sweeps again whenever it
      // opens a new file, so this only has to cover what an idle app leaves
      // behind between sessions.
      logs::sweep(app.handle());

      // Stash any folder Explorer passed us. The webview does not exist yet, so
      // this waits in a slot for the frontend to collect once it is ready.
      commands::app::set_pending_launch_path(commands::app::repo_path_from_args(
        std::env::args(),
      ));
      Ok(())
    })
    .manage(crate::airun::SessionRegistry::new())
    .manage(commands::airun::DriverRegistry::default())
    .manage(RepoManager::default())
    .manage(WatcherRegistry::default())
    .manage(commands::updates::PendingUpdate::default())
    .invoke_handler(builder.invoke_handler())
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
