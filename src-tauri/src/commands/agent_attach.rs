//! What the Agent Desk message box can attach or point at: the project's own
//! files (for "@" mentions) and the add-ons a tool has (Claude Code plugins).

use std::path::Path;

/// Most files the "@" list will hold. A monorepo with more than this still
/// gets a useful list; the person types more letters to narrow it.
const MAX_FILES: usize = 50_000;

/// Every file in the project a person might mention: tracked files plus new
/// ones not yet committed, with ignored files left out. Paths are relative,
/// with forward slashes, in a stable order.
#[tauri::command]
#[specta::specta]
pub async fn agent_project_files(repo_path: String) -> Result<Vec<String>, crate::error::AppError> {
    tauri::async_runtime::spawn_blocking(move || project_files(Path::new(&repo_path)))
        .await
        .map_err(|e| crate::error::AppError::Other(e.to_string()))?
}

fn project_files(root: &Path) -> Result<Vec<String>, crate::error::AppError> {
    let repo = git2::Repository::open(root)
        .map_err(|e| crate::error::AppError::Other(format!("could not open the project: {e}")))?;
    let mut out: Vec<String> = Vec::new();

    if let Ok(index) = repo.index() {
        for entry in index.iter().take(MAX_FILES) {
            if let Ok(path) = std::str::from_utf8(&entry.path) {
                out.push(path.to_string());
            }
        }
    }

    // New files are often exactly what someone wants to talk about, and the
    // index does not have them yet.
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true);
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        for entry in statuses.iter() {
            if out.len() >= MAX_FILES {
                break;
            }
            if entry.status().contains(git2::Status::WT_NEW) {
                if let Ok(path) = entry.path() {
                    out.push(path.to_string());
                }
            }
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}

/// One Claude Code plugin and whether it is switched on.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentPlugin {
    pub name: String,
    pub marketplace: Option<String>,
    pub enabled: bool,
}

/// The plugins the chosen tool has installed. Only Claude Code has plugins
/// GitWyrm can read; every other tool returns an empty list.
#[tauri::command]
#[specta::specta]
pub async fn agent_plugins(provider: Option<String>) -> Result<Vec<AgentPlugin>, crate::error::AppError> {
    let is_claude = provider.as_deref().map(|p| p.eq_ignore_ascii_case("claude")).unwrap_or(false);
    if !is_claude {
        return Ok(Vec::new());
    }
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(windows)]
        let home = std::env::var_os("USERPROFILE");
        #[cfg(not(windows))]
        let home = std::env::var_os("HOME");
        home.map(|h| claude_plugins(Path::new(&h))).unwrap_or_default()
    })
    .await
    .map_err(|e| crate::error::AppError::Other(e.to_string()))
}

fn claude_plugins(home: &Path) -> Vec<AgentPlugin> {
    let claude = home.join(".claude");
    let installed: serde_json::Value = std::fs::read_to_string(claude.join("plugins").join("installed_plugins.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let settings: serde_json::Value = std::fs::read_to_string(claude.join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let mut out: Vec<AgentPlugin> = installed
        .get("plugins")
        .and_then(|p| p.as_object())
        .map(|plugins| {
            plugins
                .keys()
                .map(|key| {
                    let mut parts = key.splitn(2, '@');
                    let name = parts.next().unwrap_or(key).to_string();
                    let marketplace = parts.next().map(str::to_string);
                    // Claude Code treats an installed plugin as on unless
                    // settings switch it off.
                    let enabled = settings
                        .get("enabledPlugins")
                        .and_then(|e| e.get(key))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    AgentPlugin { name, marketplace, enabled }
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_tracked_and_new_files_but_not_ignored_ones() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join(".gitignore"), "secret.txt\n").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "a").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("src/a.rs")).unwrap();
        index.write().unwrap();
        std::fs::write(dir.path().join("new.md"), "n").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "s").unwrap();

        let files = project_files(dir.path()).unwrap();
        assert!(files.contains(&"src/a.rs".to_string()));
        assert!(files.contains(&"new.md".to_string()));
        assert!(!files.contains(&"secret.txt".to_string()));
    }

    #[test]
    fn plugins_are_on_unless_switched_off() {
        let home = tempfile::TempDir::new().unwrap();
        let plugins = home.path().join(".claude/plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(
            plugins.join("installed_plugins.json"),
            r#"{ "plugins": { "frontend-design@official": [], "hooks@gitkraken": [] } }"#,
        )
        .unwrap();
        std::fs::write(
            home.path().join(".claude/settings.json"),
            r#"{ "enabledPlugins": { "hooks@gitkraken": false } }"#,
        )
        .unwrap();
        let list = claude_plugins(home.path());
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p.name == "frontend-design" && p.enabled));
        assert!(list.iter().any(|p| p.name == "hooks" && !p.enabled && p.marketplace.as_deref() == Some("gitkraken")));
    }
}
