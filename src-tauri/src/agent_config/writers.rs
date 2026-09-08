//! Per-client merge writers (tasks 4.1-4.6).
//!
//! A writer's only job is: given a destination file's current text and the
//! item to write, produce the *new full file text*. It never touches disk
//! itself -- [`super::plan`] owns hashing, backup, atomic write, and receipts
//! so every writer gets those guarantees identically instead of
//! reimplementing them per client.
//!
//! Only clients with a schema independently proven safe by fixtures get a
//! writer. The JSON clients merge through [`super::json_patch`] and Codex
//! merges through [`super::toml_patch`]; both preserve every byte outside the
//! key they touch, which is what makes writing to a file a person hand-edits
//! defensible at all.
//!
//! A client with no writer stays readable: the inventory shows its state as
//! `Unsupported` and no apply path can target it. That is a real state, not a
//! gap to paper over -- writing to a surface whose boundary was guessed rather
//! than proven is how unrelated configuration gets corrupted.

use serde_json::Value;

use super::json_patch::{self, PatchError};
use super::model::{ClientId, ExtraFields, ItemKind};
use super::registry::{self, WriterKind};
use super::toml_patch::{self, TomlPatchError};

/// Which clients currently have a working writer. Read-only clients still
/// appear in the inventory and their detected presence is reported --
/// they simply cannot be an apply destination (task 4.6).
///
/// This is a lookup into [`registry::CLIENTS`] rather than its own list, so
/// a client cannot end up declared writable in one place and read-only in
/// another. A row with no writer is read-only, full stop.
pub fn is_supported(client: ClientId) -> bool {
    registry::spec(client).can_write()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteContentError {
    #[error("{0}")]
    Patch(#[from] PatchError),
    #[error("{0}")]
    TomlPatch(#[from] TomlPatchError),
    #[error("no writer is implemented for this client")]
    Unsupported,
}

/// Produce the new full file text for writing `identity`'s `extra` fields as
/// an item of `kind` into `client`'s config at `destination_path`, merged
/// into `current_text` (the destination's current content, or an empty
/// object `"{}"` when the file does not exist yet).
pub fn build_new_content(
    client: ClientId,
    kind: ItemKind,
    identity: &str,
    extra: &ExtraFields,
    current_text: &str,
) -> Result<String, WriteContentError> {
    // Translate into the destination's dialect first. Copying the source's
    // fields across unchanged produces an entry the destination parses and
    // then ignores -- it appears in the config and never starts -- so no
    // write path is allowed to skip this. See `connector`.
    let translated = match kind {
        ItemKind::McpConnector => Some(super::connector::translate(extra, client).0),
        // Skills are directories copied whole; they have no field shape to
        // reconcile.
        ItemKind::Skill => None,
    };
    let effective = translated.as_ref().unwrap_or(extra);
    let value = Value::Object(effective.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect());
    // Dispatch is on the *kind of writing* the registry row declares, not on
    // which client it is. A client with no writer never reaches a branch that
    // can produce content, which is what keeps read-only clients read-only.
    match (registry::spec(client).writer, kind) {
        (Some(WriterKind::JsonMcpMap { key }), ItemKind::McpConnector) => {
            Ok(json_patch::set_path(current_text, &[key, identity], &value)?)
        }
        (
            Some(WriterKind::JsonMcpMapFirstPresent {
                candidates,
                default_path,
            }),
            ItemKind::McpConnector,
        ) => {
            // `?`, not `unwrap_or`. A file GitWyrm cannot read is not a file
            // with no server map in it, and defaulting on that is how a second
            // map gets written into someone else's config.
            //
            // No test covers the refusal, deliberately: it cannot currently be
            // reached. `path_is_object` and `set_path` both start from
            // `check_editable`, so a document the detector cannot read is one
            // the write refuses moments later anyway. The `?` is here so that
            // stays true if the two ever stop sharing that gate -- swallowing
            // the error with `.ok().flatten()` would be the same defect this
            // fix removes, waiting for its opportunity.
            let base = first_present_path(current_text, candidates)?.unwrap_or(default_path);
            let mut path: Vec<&str> = base.to_vec();
            path.push(identity);
            Ok(json_patch::set_path(current_text, &path, &value)?)
        }
        (Some(WriterKind::TomlTableMap { key }), ItemKind::McpConnector) => {
            Ok(toml_patch::set_path(current_text, &[key, identity], &value)?)
        }
        _ => Err(WriteContentError::Unsupported),
    }
}

/// Which of `candidates` this document already uses for its server map.
///
/// Writing a fixed key into a file that keeps its servers somewhere else
/// would add a second map the client never reads: the write would appear to
/// succeed and change nothing. Following the file is what makes that
/// impossible.
///
/// Three answers, not two. `Ok(Some(path))` is the map the file uses.
/// `Ok(None)` is a file GitWyrm read and found no map in -- a new or empty
/// settings file, where creating the documented one is right. `Err` is a file
/// it could not read, where nothing is known about where the servers live.
///
/// This used to parse with `serde_json` and fold the third answer into the
/// second. VS Code's settings file is JSONC and VS Code itself writes comments
/// and trailing commas into it, so strict parsing fails on ordinary files --
/// which meant the common case took the fallback, and a connector was written
/// to `mcp.servers` for a file whose servers were under `servers`. It read as
/// a success and Copilot saw nothing. `json_patch::path_is_object` is the same
/// walk the write itself performs, so detection and write now agree about what
/// this file contains rather than agreeing by luck.
fn first_present_path<'a>(
    current_text: &str,
    candidates: &'a [&'a [&'a str]],
) -> Result<Option<&'a [&'a str]>, PatchError> {
    for path in candidates.iter().copied() {
        if json_patch::path_is_object(current_text, path)? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// The empty-document text a writer starts from when the destination file
/// does not exist yet.
pub fn empty_document(client: ClientId) -> &'static str {
    registry::spec(client).empty_document
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extra(pairs: &[(&str, Value)]) -> ExtraFields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), super::super::model::JsonValue(v.clone())))
            .collect()
    }

    /// A real VS Code settings file keeps its servers where the file says.
    ///
    /// The detector used to parse with `serde_json`, which fails on the
    /// comments and trailing commas VS Code writes into its own settings
    /// file. So the ordinary case took the fallback and the connector was
    /// written to `mcp.servers` while the file kept its servers under
    /// `servers` -- a second map Copilot never reads. The apply reported
    /// success and nothing changed.
    #[test]
    fn a_settings_file_with_comments_keeps_its_own_server_map() {
        let existing = concat!(
            "{
",
            "  // Where Copilot keeps them on this machine
",
            "  \"servers\": {
",
            "    \"already-here\": { \"command\": \"node\" },
",
            "  },
",
            "}
"
        );
        let out = build_new_content(
            ClientId::VsCodeCopilot,
            ItemKind::McpConnector,
            "added",
            &extra(&[("command", json!("node"))]),
            existing,
        )
        .expect("an ordinary settings file must be writable");

        assert!(out.contains("already-here"), "the existing entry must survive: {out}");
        assert!(out.contains("added"), "the new entry must be written: {out}");
        assert!(
            !out.contains("\"mcp\""),
            "must not create a second map the editor never reads: {out}"
        );
    }

    /// A file with no map yet gets the documented one. This is the case
    /// `default_path` exists for, and it stays working: an absent file
    /// arrives as `{}`, which reads fine and simply has no candidate in it.
    #[test]
    fn a_file_with_no_server_map_gets_the_documented_one() {
        let out = build_new_content(
            ClientId::VsCodeCopilot,
            ItemKind::McpConnector,
            "added",
            &extra(&[("command", json!("node"))]),
            "{}
",
        )
        .expect("an empty settings file must be writable");
        assert!(out.contains("\"mcp\""), "{out}");
        assert!(out.contains("added"), "{out}");
    }

    #[test]
    fn claude_code_writer_merges_into_mcp_servers() {
        let current = "{\n  \"mcpServers\": {\n    \"existing\": { \"command\": \"old\" }\n  }\n}";
        let item = extra(&[("command", json!("npx")), ("args", json!(["gh-mcp"]))]);
        let out = build_new_content(ClientId::ClaudeCode, ItemKind::McpConnector, "github", &item, current).unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["mcpServers"]["github"]["command"], json!("npx"));
        assert_eq!(parsed["mcpServers"]["existing"]["command"], json!("old"));
    }

    #[test]
    fn opencode_writer_merges_into_mcp() {
        let current = "{}\n";
        let item = extra(&[("command", json!("linear-mcp"))]);
        let out = build_new_content(ClientId::OpenCode, ItemKind::McpConnector, "linear", &item, current).unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        // Translated on the way in, not copied across: OpenCode reads a
        // command array and would parse-then-ignore the command string this
        // entry arrived as. See `connector`.
        assert_eq!(parsed["mcp"]["linear"]["command"], json!(["linear-mcp"]));
        assert_eq!(parsed["mcp"]["linear"]["type"], json!("local"));
        assert_eq!(parsed["mcp"]["linear"]["enabled"], json!(true));
    }

    #[test]
    fn codex_writer_merges_into_the_toml_server_table() {
        let current = "model = \"gpt-5\"
";
        let item = extra(&[("command", json!("linear-mcp"))]);
        let out = build_new_content(ClientId::Codex, ItemKind::McpConnector, "linear", &item, current).unwrap();
        assert!(out.contains("[mcp_servers.linear]"), "{out}");
        assert!(out.contains("command = \"linear-mcp\""), "{out}");
        // The setting that was already there is still there.
        assert!(out.contains("model = \"gpt-5\""), "{out}");
    }

    #[test]
    fn codex_refuses_a_kind_its_writer_does_not_handle() {
        // Same guarantee as the JSON clients: a skill must not be written into
        // the connector table just because the client is writable.
        let item = extra(&[("command", json!("x"))]);
        let result = build_new_content(ClientId::Codex, ItemKind::Skill, "x", &item, "");
        assert!(matches!(result, Err(WriteContentError::Unsupported)));
    }

    #[test]
    fn a_client_with_no_writer_refuses_to_build_content() {
        // Guards the mechanism rather than one client: whichever rows are
        // read-only, none of them can produce content. The table decides.
        let item = extra(&[("command", json!("x"))]);
        for spec in super::registry::CLIENTS.iter().filter(|s| !s.can_write()) {
            let result = build_new_content(spec.id, ItemKind::McpConnector, "x", &item, "{}");
            assert!(
                matches!(result, Err(WriteContentError::Unsupported)),
                "{:?} has no writer and must refuse",
                spec.id
            );
        }
    }

    #[test]
    fn every_read_only_client_refuses_to_build_content_for_any_kind() {
        // The read-only promise, checked against the table rather than a
        // hand-listed set, so a client added later is covered too.
        let item = extra(&[("command", json!("x"))]);
        for spec in super::registry::CLIENTS.iter().filter(|s| !s.can_write()) {
            for kind in [ItemKind::Skill, ItemKind::McpConnector] {
                let result = build_new_content(spec.id, kind, "x", &item, "{}");
                assert!(
                    matches!(result, Err(WriteContentError::Unsupported)),
                    "{:?} is read-only and must refuse {kind:?}",
                    spec.id
                );
            }
        }
    }

    #[test]
    fn a_writable_client_still_refuses_a_kind_its_writer_does_not_handle() {
        // The JSON map writer only knows how to place an MCP connector; a
        // skill must not be silently written into the connector map.
        let item = extra(&[("command", json!("x"))]);
        let result = build_new_content(ClientId::ClaudeCode, ItemKind::Skill, "x", &item, "{}");
        assert!(matches!(result, Err(WriteContentError::Unsupported)));
    }

    #[test]
    fn is_supported_matches_the_writers_actually_implemented() {
        assert!(is_supported(ClientId::ClaudeCode));
        assert!(is_supported(ClientId::OpenCode));
        assert!(is_supported(ClientId::VsCodeCopilot));
        // TOML, written through a real document model so the comments and
        // formatting the person wrote themselves survive the merge.
        assert!(is_supported(ClientId::Codex));
    }

    /// VS Code reads three different places for its server map depending on
    /// how it was set up. Writing a fixed key into a file that uses another
    /// one would add a second map the editor never reads: the write would
    /// look like it worked and change nothing.
    #[test]
    fn the_writer_follows_the_map_the_file_already_uses() {
        let item = extra(&[("command", json!("npx"))]);
        let cases = [
            (r#"{"servers":{"old":{"command":"x"}}}"#, "/servers/github"),
            (r#"{"mcp":{"servers":{"old":{"command":"x"}}}}"#, "/mcp/servers/github"),
            (r#"{"mcpServers":{"old":{"command":"x"}}}"#, "/mcpServers/github"),
        ];
        for (current, pointer) in cases {
            let out =
                build_new_content(ClientId::VsCodeCopilot, ItemKind::McpConnector, "github", &item, current)
                    .unwrap();
            let parsed: Value = serde_json::from_str(&out).unwrap();
            assert!(parsed.pointer(pointer).is_some(), "{current} -> {pointer}: {out}");
            // The map that was already there keeps its own entry.
            assert!(out.contains("\"old\""), "an existing server was dropped: {out}");
        }
    }

    #[test]
    fn a_file_with_no_map_yet_gets_the_clients_documented_default() {
        let item = extra(&[("command", json!("npx"))]);
        let vs = build_new_content(ClientId::VsCodeCopilot, ItemKind::McpConnector, "github", &item, "{}").unwrap();
        let parsed: Value = serde_json::from_str(&vs).unwrap();
        assert!(parsed.pointer("/mcp/servers/github").is_some(), "{vs}");

        // OpenCode's own default key, on a file that declares none yet.
        let oc = build_new_content(ClientId::OpenCode, ItemKind::McpConnector, "github", &item, "{}").unwrap();
        let parsed: Value = serde_json::from_str(&oc).unwrap();
        assert!(parsed.pointer("/mcp/github").is_some(), "{oc}");
    }

    /// Everything outside the one member being written survives untouched --
    /// the whole reason these files are patched rather than re-serialised.
    #[test]
    fn unrelated_editor_settings_are_left_exactly_as_they_were() {
        let current = "{\n  \"editor.fontSize\": 13,\n  \"servers\": {},\n  \"files.autoSave\": \"off\"\n}";
        let item = extra(&[("command", json!("npx"))]);
        let out =
            build_new_content(ClientId::VsCodeCopilot, ItemKind::McpConnector, "github", &item, current).unwrap();
        assert!(out.contains("\"editor.fontSize\": 13"), "{out}");
        assert!(out.contains("\"files.autoSave\": \"off\""), "{out}");
    }
}
