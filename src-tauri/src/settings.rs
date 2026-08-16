//! App settings: a JSON file in the app data dir holding the open/recent
//! repos and other UI preferences, so they survive relaunch. Mirrors the
//! `ai::auth` module's pattern (plain JSON, no encryption needed here).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecentRepo {
  pub name: String,
  pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
  Stable,
  Beta,
}

/// What to do with uncommitted changes when switching branches.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BranchSwitchMode {
  /// Stash before switching, reapply after. A conflicting reapply leaves the
  /// stash intact as a backup. Most protective; the default.
  AutoStash,
  /// Plain `git checkout`: carry changes to the new branch, but refuse the
  /// switch if any change would be overwritten.
  Carry,
  /// Refuse to switch while the working tree is dirty.
  Refuse,
}

fn default_branch_switch_mode() -> BranchSwitchMode {
  BranchSwitchMode::AutoStash
}

/// Plain merge: it is what GitHub preselects, and it loses no history.
fn default_merge_method() -> crate::hosting::MergeMethod {
  crate::hosting::MergeMethod::Merge
}

/// Where commit change size appears in the graph.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSizeDisplay {
  /// A compact Changes column beside the author.
  Column,
  /// A second line below each commit message.
  Row,
}

fn default_change_size_display() -> ChangeSizeDisplay {
  ChangeSizeDisplay::Column
}

fn default_show_change_indicator() -> bool {
  true
}

/// Commit-graph column layout. Column ids are validated on the frontend, so
/// unknown values here are ignored rather than rejected.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ColumnLayout {
  /// Column ids in display order.
  #[serde(default)]
  pub order: Vec<String>,
  /// Column ids the user has hidden.
  #[serde(default)]
  pub hidden: Vec<String>,
  /// Saved widths in logical pixels, keyed by column id.
  #[serde(default)]
  pub widths: HashMap<String, f64>,
}

/// A named set of repository tabs. Repository paths are stable across app
/// restarts, unlike the in-memory repo ids assigned when a repository opens.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TabGroupSetting {
  pub id: String,
  pub name: String,
  pub color: String,
  #[serde(default)]
  pub collapsed: bool,
  #[serde(default)]
  pub repo_paths: Vec<String>,
  /// Set only on the auto-built group holding a project's opened submodules:
  /// the path of that project. Absent on groups the user made themselves.
  #[serde(default)]
  pub parent_path: Option<String>,
}

/// A folder scanned for repositories. Several can be watched at once, each
/// scanned on its own so an unplugged drive never holds up the others.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CodeFolderSetting {
  pub path: String,
  /// Short name shown beside the path ("Main work"). None shows the path alone.
  #[serde(default)]
  pub label: Option<String>,
  /// The default destination for clones and new repositories, and the folder
  /// sorted first. Exactly one folder carries this; enforced on the frontend.
  #[serde(default)]
  pub primary: bool,
}

/// One repository's tag-setting overrides, keyed by repo path in `Settings`.
/// Each field is optional: `Some` overrides the app-wide default for that repo,
/// `None` (the default) means the repo follows the app-wide setting. Validated
/// on the frontend.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct TagOverrideSetting {
  /// Per-repo push default: "ask", "always", "never". None follows the app default.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub push_default: Option<String>,
  /// Per-repo default for the New Tag send box. None follows the app default.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub push_on_create: Option<bool>,
  /// Per-repo default for the Delete Tag "also remove from the remote" box.
  /// None follows the app default.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub delete_on_remote: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Settings {
  /// Paths of repos open in tabs, in tab order, so they can be reopened on launch.
  #[serde(default)]
  pub open_repos: Vec<String>,
  /// Id of the repo that was active when the app last closed.
  #[serde(default)]
  pub active_repo_path: Option<String>,
  #[serde(default)]
  pub recents: Vec<RecentRepo>,
  /// The single watched folder, superseded by `code_folders`. Still written as
  /// the primary folder's path so an older build keeps working after a
  /// downgrade, and read once on upgrade to seed `code_folders`.
  #[serde(default)]
  pub code_folder: Option<String>,
  /// Every folder scanned for repositories. The source of truth; `code_folder`
  /// mirrors whichever entry is primary.
  #[serde(default)]
  pub code_folders: Vec<CodeFolderSetting>,
  #[serde(default)]
  pub clone_directory: Option<String>,
  /// Path to the git executable used for fetch, pull, push, and clone. None
  /// (the default) resolves git from PATH, then the copy bundled with GitWyrm.
  #[serde(default)]
  pub git_executable: Option<String>,
  /// Path to the gpg executable used to sign commits. None (the default)
  /// resolves gpg from PATH, then the copy bundled with GitWyrm.
  #[serde(default)]
  pub gpg_executable: Option<String>,
  #[serde(default = "default_update_channel")]
  pub update_channel: UpdateChannel,
  /// Install updates automatically on the launch splash instead of waiting for
  /// the user to press the update button. On by default.
  #[serde(default = "default_auto_update")]
  pub auto_update: bool,
  /// Restart as soon as a manually-triggered download finishes, rather than
  /// waiting for the user to press "Restart to update".
  ///
  /// Distinct from `auto_update`, which decides whether the launch splash
  /// installs silently before the app is ever shown. This one only governs the
  /// step after a download the user asked for, so someone who wants to choose
  /// their moment to update can still skip the second click once they have
  /// decided. Off by default: restarting on its own is a surprise the first
  /// time, and the user is already at the keyboard.
  #[serde(default)]
  pub auto_restart_after_download: bool,
  #[serde(default = "default_branch_switch_mode")]
  pub branch_switch_mode: BranchSwitchMode,
  /// How the last pull request was merged, reused as the default for the next
  /// one. Remembered rather than configured: it has no Settings control, it just
  /// keeps whatever was picked from the merge button's dropdown, because someone
  /// who rebase-merges does it every time.
  #[serde(default = "default_merge_method")]
  pub merge_method: crate::hosting::MergeMethod,
  /// Provider every AI feature uses unless pointed elsewhere: commit messages,
  /// commit generation, and Spec Desk runs.
  ///
  /// Kept as `ai_provider` rather than renamed to `ai_default_provider`: several
  /// providers can be signed in at once (auth.json is keyed by provider id), and
  /// this field has always meant "the one in use". Renaming would drop the
  /// existing value on upgrade for no gain.
  #[serde(default)]
  pub ai_provider: Option<String>,
  /// Model chosen for the default provider.
  ///
  /// Read on upgrade to seed `ai_models` for that provider, and still written so
  /// a downgrade keeps working. `ai_models` is the source of truth.
  #[serde(default)]
  pub ai_model: Option<String>,
  /// Model chosen per provider, keyed by provider id, so switching which
  /// provider is default does not discard the other's model.
  #[serde(default)]
  pub ai_models: Option<std::collections::BTreeMap<String, String>>,
  /// Whether AI features are switched on.
  ///
  /// Distinct from having no provider: off means "configured, but not right
  /// now", and never touches credentials, so turning it back on needs no
  /// sign-in. Defaults to on so an existing install is unaffected by the
  /// field appearing.
  #[serde(default = "default_ai_enabled")]
  pub ai_enabled: bool,
  /// Custom system instruction for commit-message generation. None uses the
  /// built-in default (see `crate::ai::prompt::DEFAULT_INSTRUCTION`).
  #[serde(default)]
  pub ai_instruction: Option<String>,
  /// Commit message template used when a spec archive commits automatically.
  /// `{id}` is replaced with the change id. None uses the built-in default
  /// (see `crate::openspec::cli::DEFAULT_ARCHIVE_COMMIT_TEMPLATE`).
  #[serde(default)]
  pub openspec_archive_commit_template: Option<String>,
  /// Commit-graph column order and visibility.
  #[serde(default)]
  pub column_layout: Option<ColumnLayout>,
  /// Saved width of the branches and tags pane in logical pixels.
  #[serde(default = "default_left_panel_width")]
  pub left_panel_width: f64,
  /// Saved width of the changes and commit pane in logical pixels.
  #[serde(default = "default_right_panel_width")]
  pub right_panel_width: f64,
  /// Saved height of the commit details drawer in logical pixels.
  #[serde(default = "default_drawer_height")]
  pub drawer_height: f64,
  /// Saved width of the commit list pane inside the multi-select drawer.
  #[serde(default = "default_drawer_commit_list_width")]
  pub drawer_commit_list_width: f64,
  /// Percent of the changes pane given to the unstaged list (30-70).
  #[serde(default = "default_changes_split")]
  pub changes_split: f64,
  /// Percent of the conflict view's width given to the OURS pane (20-80).
  #[serde(default = "default_conflict_side_split")]
  pub conflict_side_split: f64,
  /// Percent of the conflict view's height given to OURS/THEIRS (20-80).
  #[serde(default = "default_conflict_result_split")]
  pub conflict_result_split: f64,
  /// Whether change size appears below the message or in its own column.
  #[serde(default = "default_change_size_display")]
  pub change_size_display: ChangeSizeDisplay,
  /// Show the change-size indicator in the commit graph.
  #[serde(default = "default_show_change_indicator")]
  pub show_change_indicator: bool,
  /// Show exact added and removed line counts beside the size bar.
  #[serde(default)]
  pub show_change_line_counts: bool,
  /// Default action for the commit button: "commit" or "commit_push". None
  /// falls back to plain commit. Validated on the frontend.
  #[serde(default)]
  pub commit_button_mode: Option<String>,
  /// Editor the "open in editor" actions launch: "vs_code", "cursor",
  /// "windsurf", "jetbrains", or "zed". None falls back to VS Code. Kept as a
  /// string so a settings file naming an editor this build does not know is
  /// ignored rather than rejected.
  #[serde(default)]
  pub default_editor: Option<String>,
  /// Offer to *create* worktrees. Off by default; the frontend turns it on the
  /// first time it meets a repository that already has one.
  ///
  /// This governs creation, not visibility. A worktree that exists is part of
  /// the repository's real state and is listed regardless -- hiding it would
  /// mean the user cannot open, remove, or repair a checkout another tool put
  /// there.
  #[serde(default)]
  pub enable_worktrees: bool,
  /// True once the user has deliberately set the worktrees toggle either way.
  ///
  /// The auto-enable is a first-encounter courtesy, so it must never fire again
  /// after a deliberate off. Without this flag, "off" and "never asked" look
  /// identical and the setting would keep turning itself back on.
  #[serde(default)]
  pub worktrees_setting_touched: bool,
  /// Show the Spec Desk button and the sidebar Specs section. On by default, and
  /// on regardless of whether the repository has an `openspec/` folder yet, so a
  /// repo can be onboarded to OpenSpec from inside the app.
  #[serde(default = "default_enable_spec_desk")]
  pub enable_spec_desk: bool,
  /// Skip the confirmation when archiving a change from a row button. Set by the
  /// "Don't ask again" checkbox in that confirmation. Off by default: the opt-out
  /// is the user's to make, never a default we ship.
  #[serde(default)]
  pub openspec_archive_without_asking: bool,
  /// Skip the confirmation when deleting a change from a row button. Same
  /// origin, and off by default for the same reason -- more so, since deleting
  /// throws the work away.
  #[serde(default)]
  pub openspec_delete_without_asking: bool,
  /// Reopen the tabs from the last session on launch. On by default; when off
  /// the app starts with no repository open.
  #[serde(default = "default_restore_tabs")]
  pub restore_tabs: bool,
  /// Fetch open repositories in the background so ahead/behind counts and
  /// remote branches are current without the user asking. On by default.
  #[serde(default = "default_auto_fetch")]
  pub auto_fetch: bool,
  /// Show the short explanations that teach a feature in the sidebar and
  /// panels. On by default so the features get found; off leaves the controls
  /// and the plain empty-state labels, which is what a returning user wants.
  #[serde(default = "default_show_tips")]
  pub show_tips: bool,
  /// Send anonymous crash reports and error diagnostics. On by default; turning
  /// it off stops both the frontend and backend reporters at their next start.
  ///
  /// Read straight off disk during startup by `crash_reports_enabled`, before
  /// the Tauri app handle exists, so the reporters can honour it from the very
  /// first line rather than after settings finish loading.
  #[serde(default = "default_crash_reports")]
  pub crash_reports: bool,
  /// Send anonymous usage telemetry: performance traces, profiling, and
  /// forwarded logs. Separate from `crash_reports` so someone can report the
  /// crashes that would otherwise go unfixed without also being measured.
  ///
  /// `None` means the user has never chosen, so the default applies -- and that
  /// default is not fixed, it depends on the build (see
  /// `usage_telemetry_default`). Storing the un-chosen state rather than a
  /// resolved bool is what lets a pre-1.0 install become opt-in at 1.0 without a
  /// migration: nobody's explicit choice is ever overwritten, and everyone who
  /// never expressed one follows the new default.
  #[serde(default)]
  pub usage_telemetry: Option<bool>,
  /// Whether the welcome tour has been shown. Without this the tour reopens on
  /// every launch that starts with no repository, which is the normal state for
  /// anyone who keeps "reopen my last tabs" off.
  #[serde(default)]
  pub onboarding_seen: bool,
  /// Named identities: who you commit as and the key you sign with.
  #[serde(default)]
  pub profiles: Vec<crate::git::profiles::Profile>,
  /// Id of the profile applied to the global git config. None means the user
  /// has not adopted profiles, so their existing git config is left alone.
  #[serde(default)]
  pub active_profile_id: Option<String>,
  /// Whether the first profile has been seeded from an existing git config.
  /// Without this a user who deletes the seeded profile gets it back on the
  /// next launch, which reads as the app ignoring them.
  #[serde(default)]
  pub profiles_seeded: bool,
  /// Fingerprints of signing keys the user has confirmed they uploaded to their
  /// host. Drives the "finish setting this key up" checklist, which has to
  /// survive a reload: a key is only half-usable until its public half is on the
  /// host, and that is a job people leave half-done. Keys made before this
  /// existed are treated as already handled rather than nagged about.
  #[serde(default)]
  pub signing_keys_published: Vec<String>,
  /// Whole-app zoom factor (1.0 = 100%). None uses the default of 1.0.
  /// Clamped on the frontend before display.
  #[serde(default)]
  pub ui_scale: Option<f64>,
  /// Selected UI font id (see the frontend font registry). None means the
  /// default font. Validated on the frontend.
  #[serde(default)]
  pub font_family: Option<String>,
  /// Base UI text size in rem, before whole-app zoom. None uses the default.
  /// Clamped on the frontend before display.
  #[serde(default)]
  pub font_size: Option<f64>,
  /// Base UI text weight. None uses the default. Clamped on the frontend.
  #[serde(default)]
  pub font_weight: Option<f64>,
  /// Custom tab names, keyed by repo path. Absent paths use the repo folder name.
  #[serde(default)]
  pub tab_aliases: HashMap<String, String>,
  /// Show repository favicon or logo images in repository tabs.
  #[serde(default = "default_show_repo_icons")]
  pub show_repo_icons: bool,
  /// Hide repository names until the user points at a tab.
  #[serde(default)]
  pub tab_icon_only: bool,
  /// Saved width of the vertical repository rail in logical pixels.
  #[serde(default = "default_vertical_tab_width")]
  pub vertical_tab_width: f64,
  /// "horizontal" or "vertical". Unknown values fall back to vertical on
  /// the frontend so older and hand-edited settings remain safe.
  #[serde(default)]
  pub tab_layout: Option<String>,
  /// Give horizontal tabs their own row under the app bar instead of sharing it.
  #[serde(default)]
  pub horizontal_tab_row: bool,
  /// Show the count of open pull requests on each repository tab. Off by
  /// default: it costs a network call per repository, so the user opts in.
  #[serde(default)]
  pub show_tab_pr_count: bool,
  /// Show the count of open issues on each repository tab. Off by default for
  /// the same reason as `show_tab_pr_count`.
  #[serde(default)]
  pub show_tab_issue_count: bool,
  /// Open tab groups. These disappear when their last repository is closed.
  #[serde(default)]
  pub tab_groups: Vec<TabGroupSetting>,
  /// The shared order of loose repositories and groups. Group entries use a
  /// `group:<id>` marker; every other entry is a repository path.
  #[serde(default)]
  pub tab_order: Vec<String>,
  /// How the tab strip is arranged: "manual", "name", or "changes". Unknown
  /// values fall back to manual on the frontend. Name and changes are display
  /// views, so `tab_order` still holds the order the user dragged.
  #[serde(default)]
  pub tab_sort: Option<String>,
  /// Which way `tab_sort` runs: "forward" or "reverse". None means forward.
  /// Manual ignores it.
  #[serde(default)]
  pub tab_sort_direction: Option<String>,
  /// Repository paths kept at the front of the tab strip, in pin order.
  #[serde(default)]
  pub pinned_tab_paths: Vec<String>,
  /// Reusable group snapshots shown in Open a repository > Groups.
  #[serde(default)]
  pub saved_tab_groups: Vec<TabGroupSetting>,
  /// Repository shortcuts shown first on the add screen, most recently pinned
  /// first.
  #[serde(default)]
  pub pinned_repo_paths: Vec<String>,
  /// Saved-group shortcuts shown first on the add screen, most recently pinned
  /// first. None lets older settings start with their first three groups pinned.
  #[serde(default)]
  pub pinned_saved_group_ids: Option<Vec<String>>,
  /// Repository-picker sections the user has hidden.
  #[serde(default)]
  pub repo_picker_collapsed_sections: Vec<String>,
  /// What to do about local-only tags after a push: "ask", "always", "never".
  /// None means ask. Validated on the frontend.
  #[serde(default)]
  pub tag_push_default: Option<String>,
  /// Whether the New Tag dialog's "send it to the remote" box starts checked.
  #[serde(default)]
  pub tag_push_on_create: bool,
  /// Whether the Delete Tag dialog's "also remove it from the remote" box
  /// starts checked.
  #[serde(default)]
  pub tag_delete_on_remote: bool,
  /// Whether the "Discard all changes" dialog's "also reset submodules" box
  /// starts checked. The box only appears when a submodule has actually moved.
  #[serde(default = "default_discard_resets_submodules")]
  pub discard_resets_submodules: bool,
  /// Per-repo tag overrides, keyed by repo path. Absent repos follow the
  /// app-wide `tag_push_default` / `tag_push_on_create` / `tag_delete_on_remote`.
  #[serde(default)]
  pub tag_overrides_by_repo: HashMap<String, TagOverrideSetting>,
  /// Selected color theme id ("slate", "onyx", "midnight", "paper"). None means
  /// Auto: the app picks Slate in dark mode and Paper in light mode. Validated
  /// on the frontend.
  #[serde(default)]
  pub theme: Option<String>,
  /// Light/dark preference ("light", "dark", "system"). None means system,
  /// which follows the OS setting. Validated on the frontend.
  #[serde(default)]
  pub theme_mode: Option<String>,
  /// Use the GitWyrm mint accent across every theme. When off, each theme shows
  /// its own native accent color.
  #[serde(default = "default_mint_accent")]
  pub mint_accent: bool,
  /// Folders left open in the changes trees, keyed by `<repo path>|<staged
  /// |unstaged>`. Only open folders are stored; anything absent is collapsed.
  #[serde(default)]
  pub expanded_change_folders: HashMap<String, Vec<String>>,
  /// How the changed-file lists are arranged: "tree" groups by folder, "list"
  /// shows one flat row per file. None means tree. Validated on the frontend.
  #[serde(default)]
  pub changes_view_mode: Option<String>,
}

fn default_mint_accent() -> bool {
  true
}

fn default_update_channel() -> UpdateChannel {
  UpdateChannel::Stable
}

fn default_auto_update() -> bool {
  true
}

fn default_ai_enabled() -> bool {
  true
}

fn default_show_repo_icons() -> bool {
  true
}

fn default_enable_spec_desk() -> bool {
  true
}

fn default_restore_tabs() -> bool {
  true
}

fn default_auto_fetch() -> bool {
  true
}

fn default_show_tips() -> bool {
  true
}

fn default_crash_reports() -> bool {
  true
}

fn default_discard_resets_submodules() -> bool {
  true
}

/// Whether usage telemetry is on for someone who has never chosen, for the
/// build identified by `version` on `channel`.
///
/// Three cases, in order:
/// - Before 1.0.0 every build is a pre-release, so this is on. Tuning the app
///   during alpha is the whole reason the traces exist.
/// - From 1.0.0, a stable release is opt-in: shipped software should not
///   measure people who did not ask to be measured.
/// - Beta builds stay opt-out at any version. Running a beta is itself a
///   choice to help test, and the perf data is the point of the channel.
///
/// Crash reporting is deliberately not part of this and stays on by default at
/// every version -- an unreported crash is a crash nobody fixes.
fn usage_telemetry_default(version: &str, channel: &UpdateChannel) -> bool {
  if matches!(channel, UpdateChannel::Beta) {
    return true;
  }
  is_pre_1_0(version)
}

/// Whether a semver-ish version string is below 1.0.0. Only the major component
/// is read, so build metadata and pre-release suffixes (`1.0.0-rc1`) do not
/// matter. An unparseable version is treated as pre-1.0: that is the state of
/// the in-repo `0.0.0` placeholder, and erring toward the development default
/// is better than silently opting a dev build into release behaviour.
fn is_pre_1_0(version: &str) -> bool {
  let major = version
    .trim_start_matches('v')
    .split(['.', '-', '+'])
    .next()
    .and_then(|part| part.parse::<u64>().ok());
  match major {
    Some(major) => major < 1,
    None => true,
  }
}

/// Whether usage telemetry should run, resolving the un-chosen state against
/// this build. Used by both the startup gate and the settings command so the
/// UI and the reporters never disagree about what "default" means.
pub fn usage_telemetry_enabled_for(settings: &Settings, version: &str) -> bool {
  settings
    .usage_telemetry
    .unwrap_or_else(|| usage_telemetry_default(version, &settings.update_channel))
}

fn default_vertical_tab_width() -> f64 {
  248.0
}

fn default_left_panel_width() -> f64 {
  240.0
}

fn default_right_panel_width() -> f64 {
  320.0
}

fn default_drawer_height() -> f64 {
  212.0
}

fn default_drawer_commit_list_width() -> f64 {
  280.0
}

fn default_changes_split() -> f64 {
  50.0
}

fn default_conflict_side_split() -> f64 {
  50.0
}

fn default_conflict_result_split() -> f64 {
  45.0
}

impl Default for Settings {
  fn default() -> Self {
    Self {
      open_repos: Vec::new(),
      active_repo_path: None,
      recents: Vec::new(),
      code_folder: None,
      code_folders: Vec::new(),
      clone_directory: None,
      git_executable: None,
      gpg_executable: None,
      update_channel: default_update_channel(),
      auto_update: default_auto_update(),
      auto_restart_after_download: false,
      branch_switch_mode: default_branch_switch_mode(),
      merge_method: default_merge_method(),
      ai_provider: None,
      ai_model: None,
      ai_models: None,
      ai_enabled: default_ai_enabled(),
      ai_instruction: None,
      openspec_archive_commit_template: None,
      column_layout: None,
      left_panel_width: default_left_panel_width(),
      right_panel_width: default_right_panel_width(),
      drawer_height: default_drawer_height(),
      drawer_commit_list_width: default_drawer_commit_list_width(),
      changes_split: default_changes_split(),
      conflict_side_split: default_conflict_side_split(),
      conflict_result_split: default_conflict_result_split(),
      change_size_display: default_change_size_display(),
      show_change_indicator: default_show_change_indicator(),
      show_change_line_counts: false,
      commit_button_mode: None,
      default_editor: None,
      enable_worktrees: false,
      worktrees_setting_touched: false,
      enable_spec_desk: default_enable_spec_desk(),
      openspec_archive_without_asking: false,
      openspec_delete_without_asking: false,
      restore_tabs: true,
      auto_fetch: true,
      show_tips: true,
      crash_reports: true,
      // No stored choice; resolved per build by `usage_telemetry_enabled_for`.
      usage_telemetry: None,
      onboarding_seen: false,
      profiles: Vec::new(),
      active_profile_id: None,
      profiles_seeded: false,
      signing_keys_published: Vec::new(),
      ui_scale: None,
      font_family: None,
      font_size: None,
      font_weight: None,
      tab_aliases: HashMap::new(),
      show_repo_icons: true,
      tab_icon_only: false,
      vertical_tab_width: default_vertical_tab_width(),
      tab_layout: None,
      horizontal_tab_row: false,
      show_tab_pr_count: false,
      show_tab_issue_count: false,
      tab_groups: Vec::new(),
      tab_order: Vec::new(),
      tab_sort: None,
      tab_sort_direction: None,
      pinned_tab_paths: Vec::new(),
      saved_tab_groups: Vec::new(),
      pinned_repo_paths: Vec::new(),
      pinned_saved_group_ids: None,
      repo_picker_collapsed_sections: Vec::new(),
      expanded_change_folders: HashMap::new(),
      changes_view_mode: None,
      tag_push_default: None,
      tag_push_on_create: false,
      tag_delete_on_remote: false,
      discard_resets_submodules: default_discard_resets_submodules(),
      tag_overrides_by_repo: HashMap::new(),
      theme: None,
      theme_mode: None,
      mint_accent: default_mint_accent(),
    }
  }
}

/// Compare repository paths the way the rest of the app does: case-insensitively
/// with separators normalized, so `C:/code/app` and `C:\Code\App` are one repo.
fn same_repo_path(left: &str, right: &str) -> bool {
  normalize_repo_path(left) == normalize_repo_path(right)
}

/// Lowercase, backslash-separated, no trailing separator. The one spelling used
/// whenever repository paths are compared or used as keys.
pub fn normalize_repo_path(path: &str) -> String {
  path
    .replace('/', "\\")
    .trim_end_matches('\\')
    .to_ascii_lowercase()
}

impl Settings {
  /// Forget everything tied to `paths`. Used by the missing-repository sweep;
  /// the repository's icon files are removed by the caller.
  pub fn forget_repos(&mut self, paths: &[String]) {
    let gone = |candidate: &str| paths.iter().any(|path| same_repo_path(path, candidate));

    self.open_repos.retain(|path| !gone(path));
    self.recents.retain(|recent| !gone(&recent.path));
    self.pinned_tab_paths.retain(|path| !gone(path));
    self.pinned_repo_paths.retain(|path| !gone(path));
    self.tab_order.retain(|entry| {
      // Group markers are not repository paths and must survive the sweep.
      entry.starts_with("group:") || !gone(entry)
    });
    self.tab_aliases.retain(|path, _| !gone(path));
    self.tag_overrides_by_repo.retain(|path, _| !gone(path));
    self.expanded_change_folders.retain(|key, _| {
      let repo = key.rsplit_once('|').map(|(repo, _)| repo).unwrap_or(key);
      !gone(repo)
    });
    for group in self
      .tab_groups
      .iter_mut()
      .chain(self.saved_tab_groups.iter_mut())
    {
      group.repo_paths.retain(|path| !gone(path));
    }
    // A submodule group exists only to hang under its project. Once that
    // project is forgotten the group has nothing to attach to, so it goes too
    // rather than leaving submodule tabs nested under nothing. Its marker goes
    // with it; groups the user built keep theirs even when emptied, since the
    // frontend restores them as repositories come back.
    let dropped: Vec<String> = self
      .tab_groups
      .iter()
      .filter(|group| group.parent_path.as_deref().is_some_and(&gone))
      .map(|group| format!("group:{}", group.id))
      .collect();
    self
      .tab_groups
      .retain(|group| !group.parent_path.as_deref().is_some_and(&gone));
    self.tab_order.retain(|entry| !dropped.contains(entry));
    if self
      .active_repo_path
      .as_deref()
      .is_some_and(|path| gone(path))
    {
      self.active_repo_path = None;
    }
  }

  /// Every place a repository path can still be referenced. A repository absent
  /// from this list is one GitWyrm no longer remembers at all.
  pub fn referenced_repo_paths(&self) -> Vec<String> {
    let mut paths: Vec<String> = self.recents.iter().map(|r| r.path.clone()).collect();
    paths.extend(self.open_repos.iter().cloned());
    paths.extend(self.pinned_repo_paths.iter().cloned());
    paths.extend(self.pinned_tab_paths.iter().cloned());
    paths.extend(self.tab_aliases.keys().cloned());
    paths.extend(self.tag_overrides_by_repo.keys().cloned());
    for group in self.tab_groups.iter().chain(self.saved_tab_groups.iter()) {
      paths.extend(group.repo_paths.iter().cloned());
    }
    paths.sort_by_key(|path| normalize_repo_path(path));
    paths.dedup_by_key(|path| normalize_repo_path(path));
    paths
  }
}

/// The app data directory, created if missing. Everything GitWyrm persists lives
/// here; taking it as a parameter elsewhere in this module is what lets tests run
/// the real read and write code against a temporary directory.
pub fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
  let dir = app
    .path()
    .app_data_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  fs::create_dir_all(&dir)?;
  Ok(dir)
}

/// Whether the user has left crash reporting on, read directly from disk.
///
/// Crash reporting has to start before anything else does, which is earlier
/// than the Tauri app handle exists -- so this cannot go through
/// `app_data_dir`. It resolves the same directory Tauri would (the identifier
/// from tauri.conf.json under the platform's app-data root) and reads the file
/// itself.
///
/// Defaults to `true` on any failure. A missing or unreadable settings file is
/// a first launch or a corrupt file, not an opt-out, and the opt-out is only
/// ever recorded by the user explicitly turning the setting off.
pub fn crash_reports_enabled() -> bool {
  let Some(dir) = app_data_dir_unmanaged() else {
    return true;
  };
  read_settings_in(&dir).crash_reports
}

/// Whether the user has usage telemetry on, read directly from disk during
/// startup. Same early-startup constraint as `crash_reports_enabled`.
///
/// On a failure to locate the settings directory this falls back to the
/// build's default rather than to `true`: a first launch should follow the
/// release/beta rule, not opt everyone in.
pub fn usage_telemetry_enabled() -> bool {
  let version = env!("CARGO_PKG_VERSION");
  let Some(dir) = app_data_dir_unmanaged() else {
    return usage_telemetry_default(version, &UpdateChannel::Stable);
  };
  usage_telemetry_enabled_for(&read_settings_in(&dir), version)
}

/// The app data directory, resolved without a Tauri handle. Mirrors what
/// `AppHandle::path().app_data_dir()` returns for this app's identifier.
fn app_data_dir_unmanaged() -> Option<PathBuf> {
  // Same identifier as tauri.conf.json; Tauri appends it to the OS app-data root.
  const IDENTIFIER: &str = "dev.gitwyrm.app";
  let base = if cfg!(target_os = "windows") {
    std::env::var_os("APPDATA").map(PathBuf::from)
  } else if cfg!(target_os = "macos") {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
  } else {
    std::env::var_os("XDG_DATA_HOME")
      .map(PathBuf::from)
      .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
  };
  Some(base?.join(IDENTIFIER))
}

/// Read settings from `dir`, falling back to defaults when the file is absent or
/// unreadable.
pub fn read_settings_in(dir: &Path) -> Settings {
  let Ok(raw) = fs::read_to_string(dir.join("settings.json")) else {
    return Settings::default();
  };
  match serde_json::from_str(&raw) {
    Ok(settings) => settings,
    Err(error) => {
      log::warn!("settings.json could not be parsed, using defaults: {error}");
      Settings::default()
    }
  }
}

/// Carry over the fields the backend owns and the frontend never sends.
///
/// The frontend rebuilds the whole settings object from its own store, which
/// has no concept of profiles. Writing that object as-is would erase them every
/// time any unrelated preference changed -- silently, and long after the profile
/// was created.
pub fn keep_backend_owned_fields(incoming: &mut Settings, stored: &Settings) {
  incoming.profiles = stored.profiles.clone();
  incoming.active_profile_id = stored.active_profile_id.clone();
  incoming.profiles_seeded = stored.profiles_seeded;
}

/// Write settings to `dir` exactly as given.
pub fn write_settings_in(dir: &Path, settings: &Settings) -> Result<(), AppError> {
  let json = serde_json::to_string_pretty(settings).map_err(|e| AppError::Other(e.to_string()))?;
  fs::write(dir.join("settings.json"), json)?;
  Ok(())
}

/// Read settings without going through the async command. For backend callers
/// that are already off the IPC thread.
pub fn read_settings(app: &tauri::AppHandle) -> Result<Settings, AppError> {
  Ok(read_settings_in(&app_data_dir(app)?))
}

/// Settings are read during startup by nearly every screen, so this must not
/// run on the IPC thread even though the file is small -- a disk read there
/// stalls every other command waiting to be dispatched.
#[tauri::command]
#[specta::specta]
pub async fn get_settings(app: tauri::AppHandle) -> Result<Settings, AppError> {
  tauri::async_runtime::spawn_blocking(move || read_settings(&app))
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Whether usage telemetry is on for this build, with the un-chosen state
/// already resolved against the version and update channel.
///
/// The frontend needs the effective answer both to configure its reporter and
/// to render the toggle, and the rule that resolves it is deliberately not
/// duplicated there -- one copy means the checkbox and the reporters can never
/// disagree about what the default is.
#[tauri::command]
#[specta::specta]
pub async fn get_usage_telemetry_enabled(app: tauri::AppHandle) -> Result<bool, AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    let settings = read_settings(&app)?;
    Ok(usage_telemetry_enabled_for(&settings, env!("CARGO_PKG_VERSION")))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn save_settings(app: tauri::AppHandle, mut settings: Settings) -> Result<(), AppError> {
  // Apply the tool paths immediately so a change takes effect without a
  // restart. Every shell-out reads these globals.
  crate::git::shell::set_git_program(settings.git_executable.as_deref());
  crate::git::signing::set_gpg_program(settings.gpg_executable.as_deref());

  tauri::async_runtime::spawn_blocking(move || {
    let dir = app_data_dir(&app)?;
    keep_backend_owned_fields(&mut settings, &read_settings_in(&dir));
    write_settings_in(&dir, &settings)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Load the persisted git executable and apply it to the shell global. Called
/// once at startup so saved settings take effect before the first git command.
pub fn apply_startup_git_executable(app: &tauri::AppHandle) {
  if let Ok(settings) = read_settings(app) {
    crate::git::shell::set_git_program(settings.git_executable.as_deref());
    crate::git::signing::set_gpg_program(settings.gpg_executable.as_deref());
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn group(id: &str, repo_paths: &[&str]) -> TabGroupSetting {
    TabGroupSetting {
      id: id.into(),
      name: id.into(),
      color: "#000".into(),
      collapsed: false,
      repo_paths: repo_paths.iter().map(|p| (*p).to_string()).collect(),
      parent_path: None,
    }
  }

  /// A submodule group: the auto-built one that hangs under `parent_path`.
  fn submodule_group(id: &str, parent_path: &str, repo_paths: &[&str]) -> TabGroupSetting {
    TabGroupSetting {
      parent_path: Some(parent_path.to_string()),
      ..group(id, repo_paths)
    }
  }


  #[test]
  fn repo_paths_compare_case_and_separator_insensitively() {
    assert!(same_repo_path("C:\\code\\App", "c:/code/app"));
    assert!(same_repo_path("C:/code/app\\", "C:\\code\\app"));
    assert!(!same_repo_path("C:\\code\\app", "C:\\code\\app2"));
  }

  #[test]
  fn forgetting_a_repo_clears_every_place_it_is_stored() {
    let gone = "C:\\code\\gone";
    let kept = "C:\\code\\kept";
    let mut settings = Settings {
      open_repos: vec![gone.into(), kept.into()],
      active_repo_path: Some(gone.into()),
      recents: vec![
        RecentRepo { name: "gone".into(), path: gone.into() },
        RecentRepo { name: "kept".into(), path: kept.into() },
      ],
      pinned_tab_paths: vec![gone.into()],
      pinned_repo_paths: vec![gone.into(), kept.into()],
      // Group markers share this list with repository paths.
      tab_order: vec!["group:g1".into(), gone.into(), kept.into()],
      tab_groups: vec![group("g1", &[gone, kept])],
      saved_tab_groups: vec![group("saved", &[gone])],
      ..Settings::default()
    };
    settings.tab_aliases.insert(gone.into(), "Gone".into());
    settings.tab_aliases.insert(kept.into(), "Kept".into());
    settings
      .tag_overrides_by_repo
      .insert(gone.into(), TagOverrideSetting::default());
    settings
      .expanded_change_folders
      .insert(format!("{gone}|staged"), vec!["src".into()]);
    settings
      .expanded_change_folders
      .insert(format!("{kept}|staged"), vec!["src".into()]);

    // Mixed case and separators still match the stored entries.
    settings.forget_repos(&["c:/code/gone".to_string()]);

    assert_eq!(settings.open_repos, vec![kept.to_string()]);
    assert_eq!(settings.recents.len(), 1);
    assert_eq!(settings.recents[0].path, kept);
    assert!(settings.pinned_tab_paths.is_empty());
    assert_eq!(settings.pinned_repo_paths, vec![kept.to_string()]);
    assert_eq!(settings.tab_order, vec!["group:g1".to_string(), kept.to_string()]);
    assert_eq!(settings.tab_groups[0].repo_paths, vec![kept.to_string()]);
    assert!(settings.saved_tab_groups[0].repo_paths.is_empty());
    assert_eq!(settings.tab_aliases.len(), 1);
    assert!(settings.tab_aliases.contains_key(kept));
    assert!(settings.tag_overrides_by_repo.is_empty());
    assert_eq!(settings.expanded_change_folders.len(), 1);
    assert!(settings.active_repo_path.is_none());
  }

  #[test]
  fn forgetting_a_project_drops_its_submodule_group() {
    let parent = "C:\\code\\parent";
    let sub = "C:\\code\\parent\\vendor\\lib";
    let other = "C:\\code\\other";
    let mut settings = Settings {
      open_repos: vec![parent.into(), sub.into(), other.into()],
      tab_groups: vec![
        submodule_group("subs", parent, &[sub]),
        group("mine", &[other]),
      ],
      tab_order: vec![
        parent.to_string(),
        "group:subs".to_string(),
        "group:mine".to_string(),
      ],
      ..Settings::default()
    };

    settings.forget_repos(&[parent.to_string()]);

    // The submodule group went with its project, marker and all. The group the
    // user built is untouched.
    assert_eq!(settings.tab_groups.len(), 1);
    assert_eq!(settings.tab_groups[0].id, "mine");
    assert_eq!(settings.tab_order, vec!["group:mine".to_string()]);
  }

  #[test]
  fn forgetting_a_submodule_leaves_its_project_alone() {
    let parent = "C:\\code\\parent";
    let sub = "C:\\code\\parent\\vendor\\lib";
    let mut settings = Settings {
      open_repos: vec![parent.into(), sub.into()],
      tab_groups: vec![submodule_group("subs", parent, &[sub])],
      tab_order: vec![parent.to_string(), "group:subs".to_string()],
      ..Settings::default()
    };

    // Case and separators differ from what was stored, as in the sweep.
    settings.forget_repos(&["c:/code/parent/vendor/lib".to_string()]);

    // Only the submodule went. The group survives (emptied) because its project
    // is still here, matching how user-built groups are treated.
    assert_eq!(settings.open_repos, vec![parent.to_string()]);
    assert_eq!(settings.tab_groups.len(), 1);
    assert!(settings.tab_groups[0].repo_paths.is_empty());
    assert_eq!(
      settings.tab_order,
      vec![parent.to_string(), "group:subs".to_string()]
    );
  }
















  #[test]
  fn older_settings_default_tab_group_fields() {
    let settings: Settings = serde_json::from_str("{}").expect("empty settings should load");

    assert!(settings.tab_layout.is_none());
    assert!(settings.show_repo_icons);
    assert!(!settings.tab_icon_only);
    assert_eq!(settings.vertical_tab_width, 248.0);
    assert_eq!(settings.left_panel_width, 240.0);
    assert_eq!(settings.right_panel_width, 320.0);
    assert_eq!(settings.drawer_height, 212.0);
    assert_eq!(settings.drawer_commit_list_width, 280.0);
    assert_eq!(settings.changes_split, 50.0);
    assert_eq!(settings.change_size_display, ChangeSizeDisplay::Column);
    assert!(settings.show_change_indicator);
    assert!(!settings.show_change_line_counts);
    assert!(settings.tab_groups.is_empty());
    assert!(settings.tab_order.is_empty());
    assert!(settings.tab_sort.is_none());
    assert!(settings.tab_sort_direction.is_none());
    assert!(settings.pinned_tab_paths.is_empty());
    assert!(settings.saved_tab_groups.is_empty());
    assert!(settings.pinned_repo_paths.is_empty());
    assert!(settings.pinned_saved_group_ids.is_none());
    assert!(settings.repo_picker_collapsed_sections.is_empty());
    assert!(settings.expanded_change_folders.is_empty());
    // Theme fields default to Auto / system / mint-on.
    assert!(settings.theme.is_none());
    assert!(settings.theme_mode.is_none());
    assert!(settings.mint_accent);
    // Settings written before this key existed keep reopening their tabs.
    assert!(settings.restore_tabs);
    // Tips read as on for a settings file written before the switch existed --
    // that is what those users were already seeing, so nothing moves for them.
    assert!(settings.show_tips);
    // Crash reporting is opt-out, so a settings file written before the switch
    // existed reads as on -- that user was reporting already and nothing
    // changes for them. Only an explicit `false` turns it off.
    assert!(settings.crash_reports);
    // An existing user's settings file has no onboarding flag, so it reads as
    // false. They see the tour once, which is the intended one-time cost of
    // adding the identity step to it.
    assert!(!settings.onboarding_seen);
    // Settings written before the editor picker existed fall back to VS Code
    // on the frontend.
    assert!(settings.default_editor.is_none());
    // The AI switch defaults to on, so adding it does not silently turn AI off
    // for someone who already had a provider set up.
    assert!(settings.ai_enabled);
  }

  /// Turning the AI off has to survive a restart, and turning it back on has to
  /// stick too -- a switch that forgets is worse than no switch.
  #[test]
  fn the_ai_switch_round_trips_through_settings_json() {
    for enabled in [false, true] {
      let settings = Settings {
        ai_enabled: enabled,
        ..Default::default()
      };
      let json = serde_json::to_string(&settings).expect("serialize");
      let back: Settings = serde_json::from_str(&json).expect("deserialize");
      assert_eq!(back.ai_enabled, enabled, "ai_enabled should survive a round trip");
    }
  }

  /// The crash-reporting opt-out is read back off disk at the very start of the
  /// next launch, before any Tauri machinery exists, so it has to survive the
  /// real write-then-read path -- not just a serde round trip. If this breaks,
  /// a user who opted out silently starts reporting again on restart.
  #[test]
  fn the_crash_report_switch_round_trips_through_settings_file() {
    for enabled in [false, true] {
      let dir = tempfile::tempdir().expect("temp dir");
      let settings = Settings {
        crash_reports: enabled,
        ..Default::default()
      };
      write_settings_in(dir.path(), &settings).expect("write");
      let back = read_settings_in(dir.path());
      assert_eq!(
        back.crash_reports, enabled,
        "crash_reports should survive a write and read"
      );
    }
  }

  /// These two flags are the only thing between a row click and a destructive
  /// action, so a settings file that predates them -- or one where they were
  /// never ticked -- must mean "ask", never "just do it".
  #[test]
  fn spec_confirmation_opt_outs_default_to_asking_and_round_trip() {
    let older: Settings = serde_json::from_str("{}").expect("empty settings should load");
    assert!(!older.openspec_archive_without_asking);
    assert!(!older.openspec_delete_without_asking);
    assert!(!Settings::default().openspec_archive_without_asking);
    assert!(!Settings::default().openspec_delete_without_asking);

    let dir = tempfile::tempdir().expect("temp dir");
    let settings = Settings {
      openspec_archive_without_asking: true,
      openspec_delete_without_asking: true,
      ..Default::default()
    };
    write_settings_in(dir.path(), &settings).expect("write");
    let back = read_settings_in(dir.path());
    assert!(back.openspec_archive_without_asking);
    assert!(back.openspec_delete_without_asking);
  }

  /// A settings file written before this flag existed has to keep the helpful
  /// behavior, so an absent value means "yes, reset submodules too" -- and an
  /// explicit no has to survive, or the box the user unticked comes back.
  #[test]
  fn discard_resets_submodules_defaults_on_and_round_trips() {
    let older: Settings = serde_json::from_str("{}").expect("empty settings should load");
    assert!(older.discard_resets_submodules);
    assert!(Settings::default().discard_resets_submodules);

    let dir = tempfile::tempdir().expect("temp dir");
    let settings = Settings {
      discard_resets_submodules: false,
      ..Default::default()
    };
    write_settings_in(dir.path(), &settings).expect("write");
    assert!(!read_settings_in(dir.path()).discard_resets_submodules);
  }

  #[test]
  fn default_editor_round_trips_through_settings_json() {
    let settings = Settings {
      default_editor: Some("cursor".to_string()),
      ..Settings::default()
    };

    let json = serde_json::to_string(&settings).expect("settings should serialize");
    let restored: Settings = serde_json::from_str(&json).expect("settings should deserialize");

    assert_eq!(restored.default_editor.as_deref(), Some("cursor"));
  }

  #[test]
  fn theme_settings_round_trip_through_settings_json() {
    let settings = Settings {
      theme: Some("midnight".to_string()),
      theme_mode: Some("dark".to_string()),
      mint_accent: false,
      ..Settings::default()
    };

    let json = serde_json::to_string(&settings).expect("settings should serialize");
    let restored: Settings = serde_json::from_str(&json).expect("settings should deserialize");

    assert_eq!(restored.theme.as_deref(), Some("midnight"));
    assert_eq!(restored.theme_mode.as_deref(), Some("dark"));
    assert!(!restored.mint_accent);
  }

  #[test]
  fn settings_written_before_code_folders_existed_still_load() {
    // read_settings_in falls back to Settings::default() for the WHOLE file on a
    // parse error, so a missing #[serde(default)] here would wipe every setting
    // the user had. This pins that down.
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = r#"{"code_folder":"C:\\code","clone_directory":"D:\\clones"}"#;
    fs::write(dir.path().join("settings.json"), legacy).expect("write legacy");

    let loaded = read_settings_in(dir.path());

    assert_eq!(loaded.code_folder.as_deref(), Some("C:\\code"));
    assert_eq!(loaded.clone_directory.as_deref(), Some("D:\\clones"));
    assert!(
      loaded.code_folders.is_empty(),
      "absent code_folders defaults to empty, leaving the frontend to migrate"
    );
  }

  #[test]
  fn code_folders_survive_a_frontend_save() {
    // code_folders is frontend-owned: it must NOT be carried over by
    // keep_backend_owned_fields, or the list could never be changed.
    let dir = tempfile::tempdir().expect("tempdir");
    write_settings_in(dir.path(), &Settings::default()).expect("seed write");

    let mut from_frontend = Settings {
      code_folder: Some("C:\\code".into()),
      code_folders: vec![
        CodeFolderSetting {
          path: "C:\\code".into(),
          label: Some("Main work".into()),
          primary: true,
        },
        CodeFolderSetting {
          path: "D:\\code".into(),
          label: None,
          primary: false,
        },
      ],
      ..Settings::default()
    };
    keep_backend_owned_fields(&mut from_frontend, &read_settings_in(dir.path()));
    write_settings_in(dir.path(), &from_frontend).expect("frontend write");

    let after = read_settings_in(dir.path());
    assert_eq!(after.code_folders.len(), 2);
    assert_eq!(after.code_folders[0].path, "C:\\code");
    assert_eq!(after.code_folders[0].label.as_deref(), Some("Main work"));
    assert!(after.code_folders[0].primary);
    assert!(!after.code_folders[1].primary);
    // Mirrored for a downgrade to a build that only knows the single folder.
    assert_eq!(after.code_folder.as_deref(), Some("C:\\code"));
  }

  #[test]
  fn profiles_survive_a_write_and_reread() {
    // The seed writes profiles, then the frontend's debounced save fires.
    // This is that exact sequence against a real file.
    let dir = tempfile::tempdir().expect("tempdir");

    let mut seeded = Settings::default();
    seeded.profiles.push(crate::git::profiles::Profile {
      id: "p1".into(),
      label: "jeremy12@gmail.com".into(),
      name: "Wutname1".into(),
      email: "jeremy12@gmail.com".into(),
      signing: crate::git::profiles::SigningMethod::None,
      sign_commits: false,
      folders: vec![],
    });
    seeded.active_profile_id = Some("p1".into());
    seeded.profiles_seeded = true;
    write_settings_in(dir.path(), &seeded).expect("seed write");

    // Frontend save: rebuilt from its own store, so profiles arrive empty.
    let mut from_frontend = Settings {
      code_folder: Some("C:\\code".into()),
      ..Settings::default()
    };
    keep_backend_owned_fields(&mut from_frontend, &read_settings_in(dir.path()));
    write_settings_in(dir.path(), &from_frontend).expect("frontend write");

    let after = read_settings_in(dir.path());
    assert_eq!(after.profiles.len(), 1, "profile survived the frontend save");
    assert_eq!(after.profiles[0].email, "jeremy12@gmail.com");
    assert!(after.profiles_seeded);
    assert_eq!(after.code_folder.as_deref(), Some("C:\\code"));
  }

  #[test]
  fn saving_preferences_does_not_erase_profiles() {
    // The frontend store has no profiles field, so every save sends them empty.
    // Without the carry-over a user's profiles vanish the next time they change
    // any unrelated setting -- silently, long after they set them up.
    let stored = Settings {
      profiles: vec![crate::git::profiles::Profile {
        id: "p1".into(),
        label: "Personal".into(),
        name: "Wutname1".into(),
        email: "jeremy12@gmail.com".into(),
        signing: crate::git::profiles::SigningMethod::None,
        sign_commits: false,
        folders: vec![],
      }],
      active_profile_id: Some("p1".into()),
      profiles_seeded: true,
      ..Settings::default()
    };

    let mut incoming = Settings {
      code_folder: Some("C:\\code".into()),
      ..Settings::default()
    };
    assert!(incoming.profiles.is_empty(), "frontend sends no profiles");

    keep_backend_owned_fields(&mut incoming, &stored);

    assert_eq!(incoming.profiles.len(), 1);
    assert_eq!(incoming.profiles[0].email, "jeremy12@gmail.com");
    assert_eq!(incoming.active_profile_id.as_deref(), Some("p1"));
    assert!(incoming.profiles_seeded);
    // The preference the user actually changed still goes through.
    assert_eq!(incoming.code_folder.as_deref(), Some("C:\\code"));
  }

  #[test]
  fn tab_groups_round_trip_through_settings_json() {
    let mut settings = Settings {
      tab_layout: Some("vertical".to_string()),
      tab_order: vec!["group:work".to_string(), "C:\\code\\loose".to_string()],
      tab_sort: Some("changes".to_string()),
      tab_sort_direction: Some("reverse".to_string()),
      pinned_tab_paths: vec!["C:\\code\\GitWyrm".to_string()],
      ..Settings::default()
    };
    settings.tab_groups.push(TabGroupSetting {
      id: "work".to_string(),
      name: "Work".to_string(),
      color: "#2dd4bf".to_string(),
      collapsed: true,
      repo_paths: vec!["C:\\code\\GitWyrm".to_string()],
      parent_path: None,
    });
    settings.saved_tab_groups = settings.tab_groups.clone();
    settings.pinned_repo_paths = vec!["C:\\code\\GitWyrm".to_string()];
    settings.pinned_saved_group_ids = Some(vec!["work".to_string()]);
    settings.repo_picker_collapsed_sections =
      vec!["recent".to_string(), "watched".to_string()];
    settings.expanded_change_folders.insert(
      "c:\\code\\gitwyrm|unstaged".to_string(),
      vec!["src".to_string(), "src/components".to_string()],
    );

    let json = serde_json::to_string(&settings).expect("settings should serialize");
    let restored: Settings = serde_json::from_str(&json).expect("settings should deserialize");

    assert_eq!(restored.tab_layout.as_deref(), Some("vertical"));
    assert_eq!(restored.tab_order, settings.tab_order);
    assert_eq!(restored.tab_sort.as_deref(), Some("changes"));
    assert_eq!(restored.tab_sort_direction.as_deref(), Some("reverse"));
    assert_eq!(restored.pinned_tab_paths, vec!["C:\\code\\GitWyrm"]);
    assert_eq!(restored.tab_groups[0].name, "Work");
    assert!(restored.tab_groups[0].collapsed);
    assert_eq!(restored.saved_tab_groups[0].repo_paths, vec!["C:\\code\\GitWyrm"]);
    assert_eq!(restored.pinned_repo_paths, vec!["C:\\code\\GitWyrm"]);
    assert_eq!(restored.pinned_saved_group_ids, Some(vec!["work".to_string()]));
    assert_eq!(
      restored.repo_picker_collapsed_sections,
      vec!["recent".to_string(), "watched".to_string()]
    );
    assert_eq!(
      restored.expanded_change_folders["c:\\code\\gitwyrm|unstaged"],
      vec!["src".to_string(), "src/components".to_string()]
    );
  }

  #[test]
  fn changes_view_mode_round_trips_through_settings_json() {
    let settings = Settings {
      changes_view_mode: Some("list".to_string()),
      ..Settings::default()
    };

    let json = serde_json::to_string(&settings).expect("settings should serialize");
    let restored: Settings = serde_json::from_str(&json).expect("settings should deserialize");

    assert_eq!(restored.changes_view_mode.as_deref(), Some("list"));
    assert_eq!(Settings::default().changes_view_mode, None);
  }

  #[test]
  fn repo_icon_visibility_round_trips_through_settings_json() {
    let settings = Settings {
      show_repo_icons: false,
      tab_icon_only: true,
      vertical_tab_width: 156.0,
      ..Settings::default()
    };

    let json = serde_json::to_string(&settings).expect("settings should serialize");
    let restored: Settings = serde_json::from_str(&json).expect("settings should deserialize");

    assert!(!restored.show_repo_icons);
    assert!(restored.tab_icon_only);
    assert_eq!(restored.vertical_tab_width, 156.0);
  }

  #[test]
  fn change_size_options_round_trip_through_settings_json() {
    let settings = Settings {
      change_size_display: ChangeSizeDisplay::Row,
      show_change_indicator: false,
      show_change_line_counts: true,
      ..Settings::default()
    };

    let json = serde_json::to_string(&settings).expect("settings should serialize");
    let restored: Settings = serde_json::from_str(&json).expect("settings should deserialize");

    assert_eq!(restored.change_size_display, ChangeSizeDisplay::Row);
    assert!(!restored.show_change_indicator);
    assert!(restored.show_change_line_counts);
  }

  #[test]
  fn resized_layout_round_trips_through_settings_json() {
    let mut widths = HashMap::new();
    widths.insert("graph".to_string(), 184.0);
    let settings = Settings {
      column_layout: Some(ColumnLayout {
        order: vec!["graph".to_string(), "message".to_string()],
        hidden: vec!["sha".to_string()],
        widths,
      }),
      left_panel_width: 276.0,
      right_panel_width: 388.0,
      changes_split: 64.0,
      ..Settings::default()
    };

    let json = serde_json::to_string(&settings).expect("settings should serialize");
    let restored: Settings = serde_json::from_str(&json).expect("settings should deserialize");

    assert_eq!(restored.left_panel_width, 276.0);
    assert_eq!(restored.right_panel_width, 388.0);
    assert_eq!(restored.changes_split, 64.0);
    assert_eq!(restored.column_layout.unwrap().widths.get("graph"), Some(&184.0));
  }

  #[test]
  fn usage_telemetry_defaults_on_before_1_0() {
    assert!(usage_telemetry_default("0.0.0", &UpdateChannel::Stable));
    assert!(usage_telemetry_default("0.9.12", &UpdateChannel::Stable));
  }

  #[test]
  fn usage_telemetry_defaults_off_for_stable_releases() {
    assert!(!usage_telemetry_default("1.0.0", &UpdateChannel::Stable));
    assert!(!usage_telemetry_default("2.4.1", &UpdateChannel::Stable));
  }

  #[test]
  fn usage_telemetry_defaults_on_for_beta_at_any_version() {
    assert!(usage_telemetry_default("0.4.0", &UpdateChannel::Beta));
    assert!(usage_telemetry_default("1.0.0", &UpdateChannel::Beta));
    assert!(usage_telemetry_default("3.2.0", &UpdateChannel::Beta));
  }

  #[test]
  fn usage_telemetry_reads_major_past_suffixes() {
    assert!(is_pre_1_0("0.1.0-rc1"));
    assert!(!is_pre_1_0("1.0.0-rc1"));
    assert!(!is_pre_1_0("v1.2.3"));
    assert!(!is_pre_1_0("1.0.0+build.7"));
    // Unparseable falls back to the pre-1.0 development default.
    assert!(is_pre_1_0("nightly"));
  }

  #[test]
  fn stored_usage_telemetry_choice_beats_the_default() {
    // A 1.0 stable release defaults off, but an explicit yes is honoured...
    let mut settings = Settings {
      usage_telemetry: Some(true),
      update_channel: UpdateChannel::Stable,
      ..Settings::default()
    };
    assert!(usage_telemetry_enabled_for(&settings, "1.0.0"));

    // ...and an explicit no survives a beta build, which would default on.
    settings.usage_telemetry = Some(false);
    settings.update_channel = UpdateChannel::Beta;
    assert!(!usage_telemetry_enabled_for(&settings, "0.4.0"));
  }

  #[test]
  fn unset_usage_telemetry_follows_the_build() {
    let settings = Settings {
      usage_telemetry: None,
      update_channel: UpdateChannel::Stable,
      ..Settings::default()
    };
    assert!(usage_telemetry_enabled_for(&settings, "0.4.0"));
    // The same install, unchanged, flips to opt-in once it reaches 1.0.
    assert!(!usage_telemetry_enabled_for(&settings, "1.0.0"));
  }

  #[test]
  fn usage_telemetry_survives_a_settings_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let settings = Settings {
      usage_telemetry: Some(false),
      ..Settings::default()
    };
    write_settings_in(dir.path(), &settings).unwrap();
    assert_eq!(read_settings_in(dir.path()).usage_telemetry, Some(false));
  }

  #[test]
  fn missing_usage_telemetry_key_reads_as_unchosen() {
    // Settings written by a build that predates the field must not read as an
    // opt-out; they have to stay un-chosen so the default still applies.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), "{}").unwrap();
    assert_eq!(read_settings_in(dir.path()).usage_telemetry, None);
  }
}
