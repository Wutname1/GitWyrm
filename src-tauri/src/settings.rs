//! App settings: a JSON file in the app data dir holding the open/recent
//! repos and other UI preferences, so they survive relaunch. Mirrors the
//! `ai::auth` module's pattern (plain JSON, no encryption needed here).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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
  #[serde(default)]
  pub code_folder: Option<String>,
  #[serde(default)]
  pub clone_directory: Option<String>,
  /// Path to the git executable used for fetch, pull, push, and clone. None
  /// (the default) uses `git` from PATH.
  #[serde(default)]
  pub git_executable: Option<String>,
  #[serde(default = "default_update_channel")]
  pub update_channel: UpdateChannel,
  #[serde(default = "default_branch_switch_mode")]
  pub branch_switch_mode: BranchSwitchMode,
  #[serde(default)]
  pub ai_provider: Option<String>,
  #[serde(default)]
  pub ai_model: Option<String>,
  /// Custom system instruction for commit-message generation. None uses the
  /// built-in default (see `crate::ai::prompt::DEFAULT_INSTRUCTION`).
  #[serde(default)]
  pub ai_instruction: Option<String>,
  /// Commit-graph column order and visibility.
  #[serde(default)]
  pub column_layout: Option<ColumnLayout>,
  /// Saved width of the branches and tags pane in logical pixels.
  #[serde(default = "default_left_panel_width")]
  pub left_panel_width: f64,
  /// Saved width of the changes and commit pane in logical pixels.
  #[serde(default = "default_right_panel_width")]
  pub right_panel_width: f64,
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
  /// Show worktree actions and the worktree sidebar section. Off by default;
  /// the frontend auto-enables it when a repo already has extra worktrees.
  #[serde(default)]
  pub enable_worktrees: bool,
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
  /// "horizontal" or "vertical". Unknown values fall back to horizontal on
  /// the frontend so older and hand-edited settings remain safe.
  #[serde(default)]
  pub tab_layout: Option<String>,
  /// Give horizontal tabs their own row under the app bar instead of sharing it.
  #[serde(default)]
  pub horizontal_tab_row: bool,
  /// Open tab groups. These disappear when their last repository is closed.
  #[serde(default)]
  pub tab_groups: Vec<TabGroupSetting>,
  /// The shared order of loose repositories and groups. Group entries use a
  /// `group:<id>` marker; every other entry is a repository path.
  #[serde(default)]
  pub tab_order: Vec<String>,
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
  /// Per-repo tag overrides, keyed by repo path. Absent repos follow the
  /// app-wide `tag_push_default` / `tag_push_on_create`.
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
}

fn default_mint_accent() -> bool {
  true
}

fn default_update_channel() -> UpdateChannel {
  UpdateChannel::Stable
}

fn default_show_repo_icons() -> bool {
  true
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

impl Default for Settings {
  fn default() -> Self {
    Self {
      open_repos: Vec::new(),
      active_repo_path: None,
      recents: Vec::new(),
      code_folder: None,
      clone_directory: None,
      git_executable: None,
      update_channel: default_update_channel(),
      branch_switch_mode: default_branch_switch_mode(),
      ai_provider: None,
      ai_model: None,
      ai_instruction: None,
      column_layout: None,
      left_panel_width: default_left_panel_width(),
      right_panel_width: default_right_panel_width(),
      change_size_display: default_change_size_display(),
      show_change_indicator: default_show_change_indicator(),
      show_change_line_counts: false,
      commit_button_mode: None,
      enable_worktrees: false,
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
      tab_groups: Vec::new(),
      tab_order: Vec::new(),
      saved_tab_groups: Vec::new(),
      pinned_repo_paths: Vec::new(),
      pinned_saved_group_ids: None,
      repo_picker_collapsed_sections: Vec::new(),
      expanded_change_folders: HashMap::new(),
      tag_push_default: None,
      tag_push_on_create: false,
      tag_overrides_by_repo: HashMap::new(),
      theme: None,
      theme_mode: None,
      mint_accent: default_mint_accent(),
    }
  }
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
  let dir = app
    .path()
    .app_data_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  fs::create_dir_all(&dir)?;
  Ok(dir.join("settings.json"))
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(app: tauri::AppHandle) -> Result<Settings, AppError> {
  let path = settings_path(&app)?;
  let Ok(raw) = fs::read_to_string(&path) else {
    return Ok(Settings::default());
  };
  Ok(serde_json::from_str(&raw).unwrap_or_default())
}

#[tauri::command]
#[specta::specta]
pub fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), AppError> {
  // Apply the git executable immediately so a change takes effect without a
  // restart. Every git shell-out reads this global.
  crate::git::shell::set_git_program(settings.git_executable.as_deref());

  let path = settings_path(&app)?;
  let json = serde_json::to_string_pretty(&settings).map_err(|e| AppError::Other(e.to_string()))?;
  fs::write(path, json)?;
  Ok(())
}

/// Load the persisted git executable and apply it to the shell global. Called
/// once at startup so saved settings take effect before the first git command.
pub fn apply_startup_git_executable(app: &tauri::AppHandle) {
  if let Ok(settings) = get_settings(app.clone()) {
    crate::git::shell::set_git_program(settings.git_executable.as_deref());
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn older_settings_default_tab_group_fields() {
    let settings: Settings = serde_json::from_str("{}").expect("empty settings should load");

    assert!(settings.tab_layout.is_none());
    assert!(settings.show_repo_icons);
    assert!(!settings.tab_icon_only);
    assert_eq!(settings.vertical_tab_width, 248.0);
    assert_eq!(settings.left_panel_width, 240.0);
    assert_eq!(settings.right_panel_width, 320.0);
    assert_eq!(settings.change_size_display, ChangeSizeDisplay::Column);
    assert!(settings.show_change_indicator);
    assert!(!settings.show_change_line_counts);
    assert!(settings.tab_groups.is_empty());
    assert!(settings.tab_order.is_empty());
    assert!(settings.saved_tab_groups.is_empty());
    assert!(settings.pinned_repo_paths.is_empty());
    assert!(settings.pinned_saved_group_ids.is_none());
    assert!(settings.repo_picker_collapsed_sections.is_empty());
    assert!(settings.expanded_change_folders.is_empty());
    // Theme fields default to Auto / system / mint-on.
    assert!(settings.theme.is_none());
    assert!(settings.theme_mode.is_none());
    assert!(settings.mint_accent);
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
  fn tab_groups_round_trip_through_settings_json() {
    let mut settings = Settings {
      tab_layout: Some("vertical".to_string()),
      tab_order: vec!["group:work".to_string(), "C:\\code\\loose".to_string()],
      ..Settings::default()
    };
    settings.tab_groups.push(TabGroupSetting {
      id: "work".to_string(),
      name: "Work".to_string(),
      color: "#2dd4bf".to_string(),
      collapsed: true,
      repo_paths: vec!["C:\\code\\GitWyrm".to_string()],
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
      ..Settings::default()
    };

    let json = serde_json::to_string(&settings).expect("settings should serialize");
    let restored: Settings = serde_json::from_str(&json).expect("settings should deserialize");

    assert_eq!(restored.left_panel_width, 276.0);
    assert_eq!(restored.right_panel_width, 388.0);
    assert_eq!(restored.column_layout.unwrap().widths.get("graph"), Some(&184.0));
  }
}
