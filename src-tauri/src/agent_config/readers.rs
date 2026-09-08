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

/// Read a JSON config that may be JSONC, the way its own application does.
///
/// VS Code's `settings.json` permits `//` and block comments and trailing
/// commas, and VS Code writes them into it. Strict `serde_json` therefore
/// fails on an ordinary settings file -- `json_patch::check_editable` records
/// finding one that failed at line 7, on a trailing comma the editor had put
/// there itself.
///
/// The write side was taught this and the read side was not, so GitWyrm could
/// write to a file it refused to read. That is not a cosmetic split: the
/// inventory showed a parse error where the connectors should be, and the
/// apply plan warned it "cannot show what this would replace" -- so replacing
/// an existing connector looked exactly like adding a new one, on the screen
/// where a person decides whether to go ahead.
///
/// Applied to every JSON client, not only VS Code. The three readers differ
/// in which key they look under, not in which dialect they accept, and a
/// hand-edited `settings.json` with a note in it is the same file whichever
/// application owns it. Being more forgiving on read can only turn "could not
/// parse" into real data; it can never invent a connector that is not there.
fn parse_json_permissively(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(&strip_jsonc(text))
}

/// JSONC text as plain JSON: comments and trailing commas removed, everything
/// else byte-for-byte.
///
/// Comments are replaced with spaces rather than deleted so every byte offset
/// in a `serde_json` error still points at the same place in the real file --
/// an error message naming line 7 should mean line 7 of what the person has
/// open.
///
/// Tracks strings and escapes for the same reason `json_patch`'s own walkers
/// do: `"https://example.com"` contains `//` and is not a comment, and a `,`
/// before a `}` inside a string is not a trailing comma. That is the one way
/// this can go wrong, so `stripping_never_changes_a_readable_document` checks
/// it against every fixture rather than trusting the reasoning.
fn strip_jsonc(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escape = false;

    while i < bytes.len() {
        let c = bytes[i] as char;

        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }

        // A line comment, blanked to its end so offsets survive.
        if c == '/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }

        // A block comment. Newlines inside it are kept so line numbers in a
        // later parse error still match the file.
        if c == '/' && bytes.get(i + 1) == Some(&b'*') {
            out.push_str("  ");
            i += 2;
            while i < bytes.len() {
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    out.push_str("  ");
                    i += 2;
                    break;
                }
                out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }

        // A comma with nothing but space and comments between it and a closing
        // bracket is a trailing comma. Looked for here rather than in a second
        // pass so the string state above is the only place that decides what
        // is inside a string.
        if c == ',' && next_meaningful_is_close(bytes, i + 1) {
            out.push(' ');
            i += 1;
            continue;
        }

        out.push(c);
        i += 1;
    }
    out
}

/// Whether the next thing that is not whitespace or a comment closes a
/// bracket.
fn next_meaningful_is_close(bytes: &[u8], mut i: usize) -> bool {
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\n' => i += 1,
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() {
                    if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            b'}' | b']' => return true,
            _ => return false,
        }
    }
    false
}

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
    let value: Value = parse_json_permissively(&text).map_err(|e| ReadError::Parse {
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
    let value: Value = parse_json_permissively(&text).map_err(|e| ReadError::Parse {
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

/// Generic reader for a JSON config whose MCP servers live under one of
/// several possible keys, used for VS Code Copilot.
///
/// The candidate keys and their order come from the registry row -- the same
/// list the writer follows. They used to be spelled out again here, in a
/// different order, and one key short: the writer looks under `servers` and
/// the reader never did. So a file keeping its connectors there showed none,
/// while a write to the same file found them and joined that map.
///
/// A rule about where a client keeps its servers has one place to live.
fn read_generic_json_mcp(location: &ConfigLocation, raw: &[u8]) -> Result<Vec<RawItem>, ReadError> {
    let text = String::from_utf8_lossy(raw);
    let value: Value = parse_json_permissively(&text).map_err(|e| ReadError::Parse {
        path: location.path.clone(),
        detail: e.to_string(),
    })?;

    let servers = server_map_candidates(location.client)
        .iter()
        .find_map(|path| lookup_path(&value, path));

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

/// Where this client may keep its server map, in the order the writer tries.
fn server_map_candidates(client: ClientId) -> &'static [&'static [&'static str]] {
    match super::registry::spec(client).writer {
        Some(super::registry::WriterKind::JsonMcpMapFirstPresent { candidates, .. }) => candidates,
        _ => &[],
    }
}

/// Follow a key path into a parsed document.
fn lookup_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut node = value;
    for segment in path {
        node = node.get(*segment)?;
    }
    node.is_object().then_some(node)
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

    /// The invariant that makes the stripper safe to trust.
    ///
    /// For anything `serde_json` can already read, stripping must not change
    /// what it means. A `//` inside a URL mistaken for a comment, or a comma
    /// inside a string mistaken for a trailing one, shows up here -- these are
    /// the shapes that would otherwise be argued about rather than checked.
    #[test]
    fn stripping_never_changes_a_readable_document() {
        let readable = [
            r#"{}"#,
            r#"{ "a": 1 }"#,
            r#"{ "url": "https://example.com//path" }"#,
            r#"{ "note": "a, b, c" }"#,
            r#"{ "ends": "trailing," }"#,
            r#"{ "slash": "/* not a comment */" }"#,
            r#"{ "quoted": "he said \" then // still inside" }"#,
            r#"{ "nested": { "deep": [1, 2, { "x": "y//z" }] } }"#,
            r#"{ "backslash": "ends with \\" }"#,
            r#"{ "empty": "" , "after": 1 }"#,
        ];
        for source in readable {
            let before: Value = serde_json::from_str(source).expect("fixture must be readable");
            let after: Value =
                serde_json::from_str(&strip_jsonc(source)).expect("stripping must keep it readable");
            assert_eq!(before, after, "stripping changed this document: {source}");
        }
    }

    /// Byte offsets survive, so a parse error still names the right line.
    #[test]
    fn stripping_keeps_the_document_the_same_length() {
        let source = "{\n  // a note\n  \"a\": 1,\n}\n";
        assert_eq!(strip_jsonc(source).len(), source.len());
        assert_eq!(strip_jsonc(source).lines().count(), source.lines().count());
    }

    /// The file this was written for: comments and a trailing comma, which
    /// VS Code writes into its own settings and strict parsing refuses.
    ///
    /// The write side was taught to accept this and the read side was not, so
    /// GitWyrm could write to a file it reported as unreadable -- and the
    /// apply plan then could not show whether a copy would replace something.
    #[test]
    fn a_settings_file_with_comments_is_read_rather_than_refused() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            concat!(
                "{\n",
                "  // Copilot's connectors\n",
                "  \"mcpServers\": {\n",
                "    \"fetch\": { \"command\": \"npx\" },\n",
                "  },\n",
                "  /* editor settings below */\n",
                "  \"editor.fontSize\": 13\n",
                "}\n"
            ),
        )
        .unwrap();

        let items = read_items(&loc(ClientId::VsCodeCopilot, &path))
            .expect("an ordinary settings file must be readable");
        assert_eq!(items.len(), 1, "the connector should be found: {items:?}");
        assert_eq!(items[0].identity, "fetch");
    }

    /// A genuinely damaged file is still an error. Tolerating JSONC is not the
    /// same as accepting anything, and "could not read this" has to stay a
    /// real answer.
    #[test]
    fn a_damaged_file_is_still_reported_as_unreadable() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ \"mcpServers\": { \"a\": { \"command\": \"npx\" }\n").unwrap();
        assert!(read_items(&loc(ClientId::VsCodeCopilot, &path)).is_err());
    }

    /// The reader looks everywhere the writer does.
    ///
    /// It used to keep its own list, in a different order and one key short:
    /// the writer follows `servers` and the reader never looked there. So a
    /// settings file keeping its connectors under that key showed none in the
    /// inventory, while a write to the same file found them and joined that
    /// very map -- GitWyrm adding to a list it was telling the person was
    /// empty.
    #[test]
    fn the_reader_looks_under_every_key_the_writer_follows() {
        let dir = TempDir::new().unwrap();
        for (i, key) in ["servers", "mcpServers"].iter().enumerate() {
            let path = dir.path().join(format!("settings-{i}.json"));
            std::fs::write(
                &path,
                format!("{{ \"{key}\": {{ \"fetch\": {{ \"command\": \"npx\" }} }} }}"),
            )
            .unwrap();
            let items = read_items(&loc(ClientId::VsCodeCopilot, &path)).unwrap();
            assert_eq!(items.len(), 1, "a connector under {key} should be found");
            assert_eq!(items[0].identity, "fetch");
        }
    }

    /// Nested keys still work, and a key holding something other than an
    /// object is not a server map.
    #[test]
    fn a_nested_map_is_found_and_a_non_object_is_not_a_map() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("nested.json");
        std::fs::write(
            &nested,
            "{ \"mcp\": { \"servers\": { \"fetch\": { \"command\": \"npx\" } } } }",
        )
        .unwrap();
        assert_eq!(read_items(&loc(ClientId::VsCodeCopilot, &nested)).unwrap().len(), 1);

        let wrong = dir.path().join("wrong.json");
        std::fs::write(&wrong, "{ \"servers\": 3 }").unwrap();
        assert!(read_items(&loc(ClientId::VsCodeCopilot, &wrong)).unwrap().is_empty());
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
