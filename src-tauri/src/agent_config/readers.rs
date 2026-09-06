//! Parse each client's on-disk configuration into [`RawItem`]s.
//!
//! Reading never mutates a file (task 1.2). Every parser keeps whatever
//! fields it does not specifically model in [`RawItem::extra`] so a later
//! write can round-trip them (task 1.3).

use std::collections::BTreeMap;
use std::fs;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::model::{ClientId, ConfigLocation, ExtraFields, ItemKind, JsonValue, RawItem};
use super::redact::find_secret_fields;

fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

/// Lowercase hex encoding (see `agent_config::plan::hex` for why this is
/// hand-rolled rather than a trait call).
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn extra_from_object(map: &serde_json::Map<String, Value>) -> ExtraFields {
    map.iter().map(|(k, v)| (k.clone(), JsonValue(v.clone()))).collect()
}

fn display_name_for(identity: &str, extra: &ExtraFields) -> String {
    extra
        .get("displayName")
        .or_else(|| extra.get("name"))
        .and_then(|v| v.0.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| identity.to_string())
}

fn description_for(extra: &ExtraFields) -> Option<String> {
    extra.get("description").and_then(|v| v.0.as_str()).map(str::to_string)
}

fn build_item(location: &ConfigLocation, kind: ItemKind, identity: &str, extra: ExtraFields, raw_bytes: &[u8]) -> RawItem {
    RawItem {
        location: location.clone(),
        kind,
        identity: identity.to_string(),
        display_name: display_name_for(identity, &extra),
        description: description_for(&extra),
        secret_fields: find_secret_fields(&extra),
        extra,
        content_hash: content_hash(raw_bytes),
    }
}

/// Read every item at `location`, dispatching by client. Returns an empty
/// vec (not an error) when the file does not exist -- an absent file simply
/// contributes no items, which is the common and expected case for a client
/// that has not configured anything yet.
pub fn read_items(location: &ConfigLocation) -> Result<Vec<RawItem>, ReadError> {
    let raw = match fs::read(&location.path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(ReadError::Io {
                path: location.path.clone(),
                detail: e.to_string(),
            })
        }
    };

    match location.client {
        ClientId::ClaudeCode => read_claude_code(location, &raw),
        ClientId::OpenCode => read_opencode(location, &raw),
        ClientId::Codex => read_codex(location, &raw),
        ClientId::VsCodeCopilot => read_generic_json_mcp(location, &raw),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error("could not read {path}: {detail}")]
    Io { path: String, detail: String },
    #[error("could not parse {path}: {detail}")]
    Parse { path: String, detail: String },
}

/// `~/.claude/settings.json` and `.claude/settings.json`: MCP servers live
/// under `mcpServers`, skills are not represented in this file today (Claude
/// Code's skill discovery is filesystem-directory based, out of scope for
/// this JSON reader) -- only MCP connectors are read from this location.
fn read_claude_code(location: &ConfigLocation, raw: &[u8]) -> Result<Vec<RawItem>, ReadError> {
    let text = String::from_utf8_lossy(raw);
    let value: Value = serde_json::from_str(&text).map_err(|e| ReadError::Parse {
        path: location.path.clone(),
        detail: e.to_string(),
    })?;

    let mut items = Vec::new();
    if let Some(Value::Object(servers)) = value.get("mcpServers") {
        for (name, server) in servers {
            if let Value::Object(map) = server {
                let extra = extra_from_object(map);
                items.push(build_item(location, ItemKind::McpConnector, name, extra, raw));
            }
        }
    }
    Ok(items)
}

/// `opencode.json` / `~/.config/opencode/opencode.json`: MCP servers live
/// under `mcp`.
fn read_opencode(location: &ConfigLocation, raw: &[u8]) -> Result<Vec<RawItem>, ReadError> {
    let text = String::from_utf8_lossy(raw);
    let value: Value = serde_json::from_str(&text).map_err(|e| ReadError::Parse {
        path: location.path.clone(),
        detail: e.to_string(),
    })?;

    let mut items = Vec::new();
    if let Some(Value::Object(servers)) = value.get("mcp") {
        for (name, server) in servers {
            if let Value::Object(map) = server {
                let extra = extra_from_object(map);
                items.push(build_item(location, ItemKind::McpConnector, name, extra, raw));
            }
        }
    }
    Ok(items)
}

/// Generic reader for any JSON config that keeps MCP servers under a
/// `mcpServers` or `mcp.servers` key, used for clients whose shape has not
/// been independently proven yet (VS Code Copilot). Read-only:
/// no writer exists for these until their schema is proven (task 4.4, 4.5).
fn read_generic_json_mcp(location: &ConfigLocation, raw: &[u8]) -> Result<Vec<RawItem>, ReadError> {
    let text = String::from_utf8_lossy(raw);
    let value: Value = serde_json::from_str(&text).map_err(|e| ReadError::Parse {
        path: location.path.clone(),
        detail: e.to_string(),
    })?;

    let servers = value
        .get("mcpServers")
        .or_else(|| value.get("mcp").and_then(|m| m.get("servers")))
        .or_else(|| value.pointer("/github.copilot.chat.mcp.servers"));

    let mut items = Vec::new();
    if let Some(Value::Object(servers)) = servers {
        for (name, server) in servers {
            if let Value::Object(map) = server {
                let extra = extra_from_object(map);
                items.push(build_item(location, ItemKind::McpConnector, name, extra, raw));
            }
        }
    }
    Ok(items)
}

/// Minimal, read-only TOML reader for Codex's `config.toml`, sufficient to
/// extract `[mcp_servers.<name>]` tables. Deliberately narrow: no writer
/// exists for Codex (task 4.6, "unsupported" stays read-only), so this only
/// has to parse well enough to *display* what is there, not to guarantee
/// round-trip fidelity for arbitrary TOML. A dedicated `toml` crate is not a
/// dependency of this workspace; adding one for a read-only display path was
/// judged not worth the new dependency surface. If Codex ever gets a writer,
/// pull in `toml_edit` at that point for real comment/formatting
/// preservation rather than extending this parser.
fn read_codex(location: &ConfigLocation, raw: &[u8]) -> Result<Vec<RawItem>, ReadError> {
    let text = String::from_utf8_lossy(raw);
    let mut items = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_fields: ExtraFields = BTreeMap::new();

    let flush = |name: &Option<String>, fields: &mut ExtraFields, items: &mut Vec<RawItem>| {
        if let Some(name) = name {
            if !fields.is_empty() {
                let extra: ExtraFields = fields.clone();
                items.push(build_item(location, ItemKind::McpConnector, name, extra, raw));
            }
        }
        fields.clear();
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(header) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(&current_name, &mut current_fields, &mut items);
            current_name = header
                .strip_prefix("mcp_servers.")
                .map(|s| s.trim_matches('"').to_string());
            continue;
        }
        if current_name.is_none() {
            continue;
        }
        if let Some((key, raw_value)) = trimmed.split_once('=') {
            let key = key.trim();
            let raw_value = raw_value.trim();
            let value = parse_toml_scalar(raw_value);
            current_fields.insert(key.to_string(), JsonValue(value));
        }
    }
    flush(&current_name, &mut current_fields, &mut items);

    Ok(items)
}

/// Parses a small subset of TOML scalar syntax: quoted strings, arrays of
/// quoted strings, booleans, and integers. Anything else is kept as the raw
/// trimmed text under a JSON string, which is enough for read-only display.
fn parse_toml_scalar(raw: &str) -> Value {
    if let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Value::String(inner.to_string());
    }
    if let Some(inner) = raw.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let items = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| parse_toml_scalar(s))
            .collect();
        return Value::Array(items);
    }
    if raw == "true" || raw == "false" {
        return Value::Bool(raw == "true");
    }
    if let Ok(n) = raw.parse::<i64>() {
        return Value::Number(n.into());
    }
    Value::String(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_config::model::ConfigScope;
    use tempfile::TempDir;

    fn loc(client: ClientId, path: &std::path::Path) -> ConfigLocation {
        ConfigLocation {
            client,
            scope: ConfigScope::Personal,
            path: path.to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn missing_file_yields_no_items_not_an_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nope.json");
        let items = read_items(&loc(ClientId::ClaudeCode, &path)).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn claude_code_mcp_servers_round_trip_into_raw_items() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{
  "mcpServers": {
    "github": { "command": "npx", "args": ["gh-mcp"], "env": { "GITHUB_TOKEN": "abc123" } }
  },
  "someOtherSetting": true
}"#,
        )
        .unwrap();

        let items = read_items(&loc(ClientId::ClaudeCode, &path)).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].identity, "github");
        assert_eq!(items[0].kind, ItemKind::McpConnector);
        assert!(!items[0].secret_fields.is_empty(), "GITHUB_TOKEN should be flagged");
    }

    #[test]
    fn opencode_mcp_servers_are_read() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("opencode.json");
        fs::write(
            &path,
            r#"{ "mcp": { "linear": { "command": "linear-mcp" } } }"#,
        )
        .unwrap();
        let items = read_items(&loc(ClientId::OpenCode, &path)).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].identity, "linear");
    }

    #[test]
    fn codex_toml_mcp_servers_are_read_without_writing_anything() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "model = \"gpt-5\"\n\n[mcp_servers.github]\ncommand = \"npx\"\nargs = [\"gh-mcp\"]\n",
        )
        .unwrap();
        let items = read_items(&loc(ClientId::Codex, &path)).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].identity, "github");
        assert_eq!(items[0].extra.get("command").map(|v| &v.0), Some(&Value::String("npx".into())));

        // Reading must not have modified the file on disk.
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("model = \"gpt-5\""));
    }

    #[test]
    fn malformed_json_is_a_typed_parse_error_not_a_panic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ not json").unwrap();
        let result = read_items(&loc(ClientId::ClaudeCode, &path));
        assert!(matches!(result, Err(ReadError::Parse { .. })));
    }

    /// The distinction the copy preview depends on.
    ///
    /// A file that is simply not there is genuinely empty, and reading it is
    /// not a failure. A file that exists and cannot be parsed is an
    /// unanswered question. `build_destination_preview` used to collapse the
    /// two with `unwrap_or_default()`, so an unreadable destination was
    /// described as having nothing in it -- and the preview then said "adding
    /// a new item" for what may have been an overwrite, on the screen whose
    /// only job is to say what a copy will do before it does it.
    #[test]
    fn a_missing_file_and_an_unreadable_one_are_not_the_same_answer() {
        let dir = TempDir::new().unwrap();

        let absent = dir.path().join("never-created.json");
        assert_eq!(
            read_items(&loc(ClientId::ClaudeCode, &absent)).unwrap(),
            Vec::new(),
            "an absent file is empty, and that is a real answer"
        );

        let unreadable = dir.path().join("settings.json");
        fs::write(&unreadable, "{ not json").unwrap();
        assert!(
            read_items(&loc(ClientId::ClaudeCode, &unreadable)).is_err(),
            "an unparseable file must not read as empty"
        );
    }

    #[test]
    fn unknown_fields_on_an_item_are_preserved_in_extra() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "mcpServers": { "svc": { "command": "x", "clientSpecificFlag": { "nested": 1 } } } }"#,
        )
        .unwrap();
        let items = read_items(&loc(ClientId::ClaudeCode, &path)).unwrap();
        assert!(items[0].extra.contains_key("clientSpecificFlag"));
    }
}
