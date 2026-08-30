//! Per-client merge writers (tasks 4.1-4.6).
//!
//! A writer's only job is: given a destination file's current text and the
//! item to write, produce the *new full file text*. It never touches disk
//! itself -- [`super::plan`] owns hashing, backup, atomic write, and receipts
//! so every writer gets those guarantees identically instead of
//! reimplementing them per client.
//!
//! Only clients with a schema independently proven safe by fixtures get a
//! writer: Claude Code and OpenCode ship here (JSON, merged through
//! [`super::json_patch`], which preserves every byte outside the touched
//! key). Codex (TOML) has no writer -- Codex stays read-only until a real
//! TOML editor (`toml_edit`) is added as a dependency and proven with its own
//! fixtures (task 4.1's "Codex merge writer" is intentionally NOT delivered
//! in this pass; see the report). VS Code Copilot and OpenChamber also have
//! no writer yet (tasks 4.4, 4.5): their settings surfaces were not
//! independently reproduced against a real client in this pass, and writing
//! to VS Code's general `settings.json` in particular risks corrupting
//! unrelated editor configuration if the safe-surface boundary is guessed
//! rather than proven. All three remain readable (task 4.6): the inventory
//! shows their state as `Unsupported` and no apply path can target them.

use serde_json::Value;

use super::json_patch::{self, PatchError};
use super::model::{ClientId, ExtraFields, ItemKind};
use super::registry::{self, WriterKind};

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
    let value = Value::Object(extra.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect());
    // Dispatch is on the *kind of writing* the registry row declares, not on
    // which client it is. A client with no writer never reaches a branch that
    // can produce content, which is what keeps read-only clients read-only.
    match (registry::spec(client).writer, kind) {
        (Some(WriterKind::JsonMcpMap { key }), ItemKind::McpConnector) => {
            Ok(json_patch::set_path(current_text, &[key, identity], &value)?)
        }
        _ => Err(WriteContentError::Unsupported),
    }
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
        assert_eq!(parsed["mcp"]["linear"]["command"], json!("linear-mcp"));
    }

    #[test]
    fn unsupported_client_refuses_to_build_content() {
        let item = extra(&[("command", json!("x"))]);
        let result = build_new_content(ClientId::Codex, ItemKind::McpConnector, "x", &item, "");
        assert!(matches!(result, Err(WriteContentError::Unsupported)));
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
        assert!(!is_supported(ClientId::Codex));
        assert!(!is_supported(ClientId::VsCodeCopilot));
        assert!(!is_supported(ClientId::OpenChamber));
    }
}
