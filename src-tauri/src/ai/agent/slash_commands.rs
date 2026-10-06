//! The slash commands an AI tool offers, for the Agent Desk message box.
//!
//! Two sources, merged:
//!
//! - **Files on disk**, read before any chat runs: skills (a folder holding a
//!   `SKILL.md`), custom commands (a Markdown or TOML file per command) and,
//!   for Claude Code, the skills and commands of enabled plugins. These are
//!   what a person installed, so the menu works on a brand-new chat.
//! - **What a running tool announced**: ACP agents send
//!   `available_commands_update`, and Claude Code lists its commands in its
//!   `system`/`init` message. These add the built-ins no folder describes.
//!   They are kept for the rest of the app's life, per tool.
//!
//! Nothing here is a list of commands written into GitWyrm. A menu offering
//! something the tool cannot do is worse than no menu.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One command, as the menu shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandInfo {
    /// Without the slash. Sub-folders and plugins use a colon ("plugin:skill").
    pub name: String,
    pub description: String,
    pub kind: SlashCommandKind,
    /// What to type after it, when the tool says ("<issue number>").
    pub argument_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum SlashCommandKind {
    Skill,
    Command,
    Builtin,
}

/// Commands each tool announced while running, by tool id.
static ANNOUNCED: Mutex<Option<HashMap<String, Vec<SlashCommandInfo>>>> = Mutex::new(None);

/// Records what a running tool said it offers, replacing its earlier list.
pub fn remember_announced(agent_id: &str, commands: Vec<SlashCommandInfo>) {
    if commands.is_empty() {
        return;
    }
    if let Ok(mut guard) = ANNOUNCED.lock() {
        guard.get_or_insert_with(HashMap::new).insert(agent_id.to_ascii_lowercase(), commands);
    }
}

fn announced(agent_id: &str) -> Vec<SlashCommandInfo> {
    ANNOUNCED
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|m| m.get(&agent_id.to_ascii_lowercase()).cloned()))
        .unwrap_or_default()
}

/// ACP's `available_commands_update` payload, turned into menu rows.
pub fn from_acp_update(update: &serde_json::Value) -> Vec<SlashCommandInfo> {
    update
        .get("availableCommands")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|c| {
                    let name = c.get("name")?.as_str()?.trim().trim_start_matches('/').to_string();
                    if name.is_empty() {
                        return None;
                    }
                    Some(SlashCommandInfo {
                        name,
                        description: c.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string(),
                        kind: SlashCommandKind::Builtin,
                        argument_hint: c
                            .get("input")
                            .and_then(|i| i.get("hint"))
                            .and_then(|h| h.as_str())
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Claude Code's `system`/`init` message lists command names only.
pub fn from_claude_init(message: &serde_json::Value) -> Vec<SlashCommandInfo> {
    if message.get("type").and_then(|t| t.as_str()) != Some("system")
        || message.get("subtype").and_then(|t| t.as_str()) != Some("init")
    {
        return Vec::new();
    }
    message
        .get("slash_commands")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|n| n.as_str())
                .map(|n| SlashCommandInfo {
                    name: n.trim_start_matches('/').to_string(),
                    description: String::new(),
                    kind: SlashCommandKind::Builtin,
                    argument_hint: None,
                })
                .filter(|c| !c.name.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Everything the given tool offers for a chat in `repo_root`.
///
/// Files on disk come first, so their descriptions win over an announced
/// entry of the same name (Claude's init list has names only).
pub fn list(home: &Path, repo_root: Option<&Path>, agent_id: &str) -> Vec<SlashCommandInfo> {
    let mut out = Vec::new();
    for dir in skill_dirs(home, repo_root, agent_id) {
        out.extend(skills_in(&dir, ""));
    }
    for (dir, format) in command_dirs(home, repo_root, agent_id) {
        out.extend(commands_in(&dir, &dir, format));
    }
    if agent_id.eq_ignore_ascii_case("claude") {
        out.extend(claude_plugin_commands(home));
    }
    out.extend(announced(agent_id));

    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.name.to_ascii_lowercase()));
    out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    out
}

/// Where each tool looks for skills. Folders that do not exist are skipped.
fn skill_dirs(home: &Path, repo: Option<&Path>, agent_id: &str) -> Vec<PathBuf> {
    // `~/.agents/skills` is the shared folder several tools read, alongside
    // their own.
    let shared = home.join(".agents").join("skills");
    let mut dirs = match agent_id.to_ascii_lowercase().as_str() {
        "claude" => vec![home.join(".claude").join("skills")],
        "codex" => vec![home.join(".codex").join("skills"), shared],
        "gemini" => vec![home.join(".gemini").join("skills"), shared],
        "opencode" => vec![home.join(".config").join("opencode").join("skills"), shared],
        "copilot" => vec![home.join(".copilot").join("skills"), shared],
        _ => Vec::new(),
    };
    if let Some(repo) = repo {
        match agent_id.to_ascii_lowercase().as_str() {
            "claude" => dirs.push(repo.join(".claude").join("skills")),
            "opencode" => dirs.push(repo.join(".opencode").join("skills")),
            "gemini" => dirs.push(repo.join(".gemini").join("skills")),
            "copilot" => {
                dirs.push(repo.join(".github").join("skills"));
                dirs.push(repo.join(".claude").join("skills"));
            }
            _ => {}
        }
    }
    dirs
}

#[derive(Clone, Copy)]
enum CommandFormat {
    /// One `.md` file per command, optional front matter with `description`
    /// and `argument-hint`.
    Markdown,
    /// One `.toml` file per command with a `description = "..."` line (Gemini).
    Toml,
}

fn command_dirs(home: &Path, repo: Option<&Path>, agent_id: &str) -> Vec<(PathBuf, CommandFormat)> {
    use CommandFormat::*;
    let mut dirs = Vec::new();
    match agent_id.to_ascii_lowercase().as_str() {
        "claude" => {
            dirs.push((home.join(".claude").join("commands"), Markdown));
            if let Some(repo) = repo {
                dirs.push((repo.join(".claude").join("commands"), Markdown));
            }
        }
        "opencode" => {
            for name in ["command", "commands"] {
                dirs.push((home.join(".config").join("opencode").join(name), Markdown));
                if let Some(repo) = repo {
                    dirs.push((repo.join(".opencode").join(name), Markdown));
                }
            }
        }
        "gemini" => {
            dirs.push((home.join(".gemini").join("commands"), Toml));
            if let Some(repo) = repo {
                dirs.push((repo.join(".gemini").join("commands"), Toml));
            }
        }
        _ => {}
    }
    dirs
}

/// Skills one level under `dir`, named `<prefix><folder>`, each followed by
/// its sub-commands when it declares any (see [`sub_commands`]).
fn skills_in(dir: &Path, prefix: &str) -> Vec<SlashCommandInfo> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(text) = fs::read_to_string(path.join("SKILL.md")) else { continue };
        let Some(folder) = path.file_name().map(|f| f.to_string_lossy().into_owned()) else { continue };
        let front = front_matter(&text);
        // A skill that turns off its own slash entry stays out of the menu.
        if front.get("user-invocable").map(|v| v == "false").unwrap_or(false) {
            continue;
        }
        let name = format!("{prefix}{folder}");
        let hint = front.get("argument-hint").cloned();
        let subs = sub_commands(&name, hint.as_deref(), &text);
        out.push(SlashCommandInfo {
            name,
            description: front.get("description").cloned().unwrap_or_default(),
            kind: SlashCommandKind::Skill,
            // Once its sub-commands are rows of their own, the long
            // alternatives list says nothing the menu does not.
            argument_hint: if subs.is_empty() { hint } else { None },
        });
        out.extend(subs);
    }
    out
}

/// A skill's sub-commands, as "skill sub" rows.
///
/// Skills like `impeccable` declare these twice: the first bracket of
/// `argument-hint` lists the words ("[shape · audit|critique · live]"), and a
/// Markdown table in the body describes each one in a row that opens with the
/// word in backticks (`` | `audit [target]` | ... | Technical quality checks |``).
/// A word must appear in BOTH to count. Plenty of skills have tables of
/// backticked commands that are not sub-commands at all -- a CLI reference,
/// say -- and the hint is what says these words are ones the skill accepts.
/// A hint with no table still gives the rows, without descriptions.
fn sub_commands(skill: &str, hint: Option<&str>, text: &str) -> Vec<SlashCommandInfo> {
    let Some(hint) = hint else { return Vec::new() };
    let Some(first) = hint.trim().strip_prefix('[').and_then(|h| h.split(']').next()) else {
        return Vec::new();
    };
    let words: Vec<&str> = first
        .split(|c: char| c == '|' || c == '·' || c.is_whitespace())
        .map(str::trim)
        .filter(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .collect();
    // One alternative is a placeholder, not a menu ("[target]").
    if words.len() < 2 {
        return Vec::new();
    }
    let described = command_table(text);
    words
        .into_iter()
        .map(|word| {
            let (description, argument_hint) = described.get(word).cloned().unwrap_or_default();
            SlashCommandInfo {
                name: format!("{skill} {word}"),
                description,
                kind: SlashCommandKind::Skill,
                argument_hint,
            }
        })
        .collect()
}

/// Rows of Markdown tables whose first cell is a backticked command, keyed by
/// its first word: (the "Description" column, the rest of the backticked cell).
fn command_table(text: &str) -> HashMap<String, (String, Option<String>)> {
    let mut out = HashMap::new();
    let mut description_col: Option<usize> = None;
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            description_col = None;
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if let Some(col) = cells.iter().position(|c| c.eq_ignore_ascii_case("description")) {
            description_col = Some(col);
            continue;
        }
        let (Some(col), Some(first)) = (description_col, cells.first()) else { continue };
        let Some(code) = first.strip_prefix('`').and_then(|c| c.split('`').next()) else { continue };
        let mut parts = code.splitn(2, ' ');
        let Some(word) = parts.next().filter(|w| !w.is_empty()) else { continue };
        let rest = parts.next().map(str::trim).filter(|r| !r.is_empty()).map(str::to_string);
        let description = cells.get(col).map(|d| d.to_string()).unwrap_or_default();
        out.entry(word.to_string()).or_insert((description, rest));
    }
    out
}

/// Command files under `dir`, recursing; a sub-folder becomes a `folder:` prefix.
fn commands_in(root: &Path, dir: &Path, format: CommandFormat) -> Vec<SlashCommandInfo> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(commands_in(root, &path, format));
            continue;
        }
        let wanted = match format {
            CommandFormat::Markdown => "md",
            CommandFormat::Toml => "toml",
        };
        if path.extension().and_then(|e| e.to_str()) != Some(wanted) {
            continue;
        }
        let Ok(rel) = path.with_extension("").strip_prefix(root).map(Path::to_path_buf) else {
            continue;
        };
        let name = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(":");
        let text = fs::read_to_string(&path).unwrap_or_default();
        let (description, argument_hint) = match format {
            CommandFormat::Markdown => {
                let front = front_matter(&text);
                (front.get("description").cloned(), front.get("argument-hint").cloned())
            }
            CommandFormat::Toml => (toml_string(&text, "description"), None),
        };
        out.push(SlashCommandInfo {
            name,
            description: description.unwrap_or_default(),
            kind: SlashCommandKind::Command,
            argument_hint,
        });
    }
    out
}

/// Skills and commands of Claude Code plugins that are installed and not
/// switched off, named `plugin:name` the way Claude Code offers them.
fn claude_plugin_commands(home: &Path) -> Vec<SlashCommandInfo> {
    let claude = home.join(".claude");
    let Ok(raw) = fs::read_to_string(claude.join("plugins").join("installed_plugins.json")) else {
        return Vec::new();
    };
    let Ok(installed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let enabled: serde_json::Value = fs::read_to_string(claude.join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);

    let mut out = Vec::new();
    let Some(plugins) = installed.get("plugins").and_then(|p| p.as_object()) else {
        return out;
    };
    for (key, installs) in plugins {
        if enabled.get("enabledPlugins").and_then(|e| e.get(key)).and_then(|v| v.as_bool()) == Some(false) {
            continue;
        }
        let plugin = key.split('@').next().unwrap_or(key);
        let Some(path) = installs
            .as_array()
            .and_then(|a| a.last())
            .and_then(|i| i.get("installPath"))
            .and_then(|p| p.as_str())
        else {
            continue;
        };
        let root = PathBuf::from(path);
        let prefix = format!("{plugin}:");
        out.extend(skills_in(&root.join("skills"), &prefix));
        let commands = root.join("commands");
        out.extend(commands_in(&commands, &commands, CommandFormat::Markdown).into_iter().map(|mut c| {
            c.name = format!("{prefix}{}", c.name);
            c
        }));
    }
    out
}

/// The `key: value` pairs of a leading `---` block. Quotes are stripped;
/// anything this cannot read contributes nothing rather than failing.
fn front_matter(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return out;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
            if !value.is_empty() {
                out.insert(key.trim().to_string(), value);
            }
        }
    }
    out
}

/// A top-level `key = "value"` string from a TOML file, read by hand for the
/// same reason as `front_matter`: one field does not justify a parser.
fn toml_string(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        if k.trim() != key {
            return None;
        }
        let v = v.trim();
        let v = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(v);
        (!v.is_empty()).then(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn claude_lists_skills_commands_and_enabled_plugins() {
        let home = tempfile::TempDir::new().unwrap();
        let h = home.path();
        write(&h.join(".claude/skills/impeccable/SKILL.md"), "---\nname: impeccable\ndescription: Design work\n---\nbody");
        write(&h.join(".claude/skills/hidden/SKILL.md"), "---\nuser-invocable: false\n---\n");
        write(&h.join(".claude/commands/goal.md"), "---\ndescription: Set a goal\nargument-hint: <goal>\n---\n");
        write(&h.join(".claude/commands/git/sync.md"), "no front matter");
        let plugin = h.join("plugin-root");
        write(&plugin.join("skills/frontend-design/SKILL.md"), "---\ndescription: Distinctive UI\n---\n");
        let installed = serde_json::json!({ "plugins": {
            "frontend-design@official": [{ "installPath": plugin.to_string_lossy() }],
            "off@official": [{ "installPath": plugin.to_string_lossy() }]
        }});
        write(&h.join(".claude/plugins/installed_plugins.json"), &installed.to_string());
        write(&h.join(".claude/settings.json"), r#"{ "enabledPlugins": { "off@official": false } }"#);

        let names: Vec<_> = list(h, None, "claude").into_iter().map(|c| (c.name, c.kind)).collect();
        assert!(names.contains(&("impeccable".into(), SlashCommandKind::Skill)));
        assert!(names.contains(&("goal".into(), SlashCommandKind::Command)));
        assert!(names.contains(&("git:sync".into(), SlashCommandKind::Command)));
        assert!(names.contains(&("frontend-design:frontend-design".into(), SlashCommandKind::Skill)));
        assert!(!names.iter().any(|(n, _)| n == "hidden"), "a skill can opt out of the menu");
        assert!(!names.iter().any(|(n, _)| n.starts_with("off:")), "a disabled plugin is not offered");

        let goal = list(h, None, "claude").into_iter().find(|c| c.name == "goal").unwrap();
        assert_eq!(goal.description, "Set a goal");
        assert_eq!(goal.argument_hint.as_deref(), Some("<goal>"));
    }

    #[test]
    fn a_skill_with_a_hint_and_a_commands_table_gets_sub_command_rows() {
        let home = tempfile::TempDir::new().unwrap();
        let h = home.path();
        write(
            &h.join(".claude/skills/impeccable/SKILL.md"),
            "---\ndescription: Design\nargument-hint: \"[shape · audit|critique · live] [target]\"\n---\n\n\
             ## Commands\n\n| Command | Category | Description | Reference |\n|---|---|---|---|\n\
             | `audit [target]` | Evaluate | Technical quality checks | [a](a.md) |\n\
             | `live` | Iterate | Visual variant mode | [l](l.md) |\n\
             | `craft` | Build | Not in the hint, so not offered | [c](c.md) |\n",
        );
        // A table of backticked commands with no hint is a reference, not a menu.
        write(
            &h.join(".claude/skills/wrangler/SKILL.md"),
            "---\ndescription: CLI\n---\n| Command | Description |\n|---|---|\n| `deploy` | Ship it |\n",
        );
        let all = list(h, None, "claude");
        let get = |n: &str| all.iter().find(|c| c.name == n);
        assert_eq!(get("impeccable audit").map(|c| c.description.as_str()), Some("Technical quality checks"));
        assert_eq!(get("impeccable audit").and_then(|c| c.argument_hint.as_deref()), Some("[target]"));
        assert_eq!(get("impeccable live").map(|c| c.description.as_str()), Some("Visual variant mode"));
        assert!(get("impeccable shape").is_some(), "a hinted word with no table row still gets a row");
        assert!(get("impeccable craft").is_none());
        assert!(get("impeccable").unwrap().argument_hint.is_none(), "the parent drops the long hint");
        assert!(!all.iter().any(|c| c.name.starts_with("wrangler ")));
    }

    #[test]
    fn gemini_reads_toml_commands_and_shared_skills() {
        let home = tempfile::TempDir::new().unwrap();
        let h = home.path();
        write(&h.join(".gemini/commands/review.toml"), "description = \"Review the diff\"\nprompt = \"...\"");
        write(&h.join(".agents/skills/wrangler/SKILL.md"), "---\ndescription: Deploy\n---\n");
        let all = list(h, None, "gemini");
        assert!(all.iter().any(|c| c.name == "review" && c.description == "Review the diff"));
        assert!(all.iter().any(|c| c.name == "wrangler"));
    }

    #[test]
    fn announced_commands_join_the_list_without_duplicating_file_ones() {
        let home = tempfile::TempDir::new().unwrap();
        let h = home.path();
        write(&h.join(".copilot/skills/review/SKILL.md"), "---\ndescription: From disk\n---\n");
        let update = serde_json::json!({
            "sessionUpdate": "available_commands_update",
            "availableCommands": [
                { "name": "review", "description": "Built in" },
                { "name": "compact", "description": "Shrink the chat", "input": { "hint": "focus" } }
            ]
        });
        remember_announced("copilot-test", from_acp_update(&update));
        let all = list(h, None, "copilot-test");
        // Unknown tool id: no folders, only what it announced.
        assert_eq!(all.iter().filter(|c| c.name == "review").count(), 1);
        let compact = all.iter().find(|c| c.name == "compact").unwrap();
        assert_eq!(compact.argument_hint.as_deref(), Some("focus"));
    }

    #[test]
    fn claude_init_names_become_built_ins() {
        let init = serde_json::json!({ "type": "system", "subtype": "init", "slash_commands": ["compact", "/review"] });
        let names: Vec<_> = from_claude_init(&init).into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["compact", "review"]);
        assert!(from_claude_init(&serde_json::json!({ "type": "assistant" })).is_empty());
    }
}
