//! Format-preserving TOML edits, the TOML counterpart to [`super::json_patch`].
//!
//! Codex keeps its connectors in `~/.codex/config.toml`, a file a person
//! writes by hand and comments. A writer that reformatted it would be a
//! writer nobody trusts twice, so this uses `toml_edit`'s document model,
//! which keeps every byte it does not deliberately change: comments, blank
//! lines, key order, and the choice between inline and block tables all
//! survive.
//!
//! The contract matches `json_patch::set_path` on purpose. Both take the
//! current file text and return the new full text, so [`super::writers`] can
//! dispatch on the registry row without caring which syntax it produced.

use serde_json::Value;
use toml_edit::{Array, DocumentMut, Item, Table, Value as TomlValue};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TomlPatchError {
    #[error("source is not valid TOML: {0}")]
    InvalidToml(String),
    #[error("path segment {0:?} exists but is not a table")]
    PathSegmentNotTable(String),
    #[error("value for {0:?} cannot be represented in TOML")]
    UnrepresentableValue(String),
}

/// Set the table at `path` (nested keys, e.g. `["mcp_servers", "github"]`) to
/// `new_value`, returning the full updated document text.
///
/// An existing table at that path is replaced wholesale rather than merged
/// into, which matches the JSON writer: writing a connector means the
/// connector as given, not a blend of the old and new definitions. Everything
/// outside that one table is untouched.
pub fn set_path(source: &str, path: &[&str], new_value: &Value) -> Result<String, TomlPatchError> {
    let mut doc: DocumentMut = source
        .parse()
        .map_err(|e: toml_edit::TomlError| TomlPatchError::InvalidToml(e.to_string()))?;

    let (last, parents) = match path.split_last() {
        Some(split) => split,
        // No path is a no-op rather than an error: there is nothing to write
        // and nothing to corrupt.
        None => return Ok(doc.to_string()),
    };

    // Walk to the parent table, creating implicit tables on the way. An
    // implicit parent means `[mcp_servers.github]` is written without also
    // emitting an empty `[mcp_servers]` header the file never had.
    let mut table = doc.as_table_mut();
    for segment in parents {
        let entry = table.entry(segment).or_insert_with(|| {
            let mut created = Table::new();
            created.set_implicit(true);
            Item::Table(created)
        });
        table = entry
            .as_table_mut()
            .ok_or_else(|| TomlPatchError::PathSegmentNotTable((*segment).to_string()))?;
    }

    let written = to_table(new_value, last)?;
    table.insert(last, Item::Table(written));
    Ok(doc.to_string())
}

/// Convert a JSON object into a TOML table.
///
/// Only an object can become a table, because the only thing written through
/// this path is a connector definition, and a connector is a set of named
/// fields.
fn to_table(value: &Value, label: &str) -> Result<Table, TomlPatchError> {
    let object = value
        .as_object()
        .ok_or_else(|| TomlPatchError::UnrepresentableValue(label.to_string()))?;
    let mut table = Table::new();
    for (key, field) in object {
        match field {
            // A nested object becomes its own sub-table, so `env` lands as
            // `[mcp_servers.github.env]` the way Codex writes it by hand.
            Value::Object(_) => {
                table.insert(key, Item::Table(to_table(field, key)?));
            }
            other => {
                table.insert(key, Item::Value(to_value(other, key)?));
            }
        }
    }
    Ok(table)
}

fn to_value(value: &Value, label: &str) -> Result<TomlValue, TomlPatchError> {
    Ok(match value {
        Value::String(s) => TomlValue::from(s.as_str()),
        Value::Bool(b) => TomlValue::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                TomlValue::from(i)
            } else if let Some(f) = n.as_f64() {
                TomlValue::from(f)
            } else {
                return Err(TomlPatchError::UnrepresentableValue(label.to_string()));
            }
        }
        Value::Array(items) => {
            let mut array = Array::new();
            for item in items {
                array.push(to_value(item, label)?);
            }
            TomlValue::Array(array)
        }
        // TOML has no null. Refusing is better than inventing an empty string,
        // which would look like a deliberate setting to whoever reads the file
        // next.
        Value::Null => return Err(TomlPatchError::UnrepresentableValue(label.to_string())),
        Value::Object(_) => return Err(TomlPatchError::UnrepresentableValue(label.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn connector() -> Value {
        json!({ "command": "npx", "args": ["gh-mcp"] })
    }

    #[test]
    fn writes_a_connector_into_an_empty_document() {
        let out = set_path("", &["mcp_servers", "github"], &connector()).unwrap();
        assert!(out.contains("[mcp_servers.github]"), "{out}");
        assert!(out.contains("command = \"npx\""), "{out}");
        assert!(out.contains("args = [\"gh-mcp\"]"), "{out}");
    }

    #[test]
    fn comments_and_unrelated_settings_survive() {
        // The whole reason this module exists: a hand-written config keeps its
        // comments and its layout.
        let source = "# my codex setup\nmodel = \"gpt-5\"\n\n# tools\n[mcp_servers.local]\ncommand = \"local-mcp\"\n";
        let out = set_path(source, &["mcp_servers", "github"], &connector()).unwrap();
        assert!(out.contains("# my codex setup"), "{out}");
        assert!(out.contains("# tools"), "{out}");
        assert!(out.contains("model = \"gpt-5\""), "{out}");
        assert!(out.contains("[mcp_servers.local]"), "{out}");
        assert!(out.contains("command = \"local-mcp\""), "{out}");
        assert!(out.contains("[mcp_servers.github]"), "{out}");
    }

    #[test]
    fn replacing_an_existing_connector_leaves_its_neighbours_alone() {
        let source = "[mcp_servers.github]\ncommand = \"old\"\n\n[mcp_servers.other]\ncommand = \"keep\"\n";
        let out = set_path(source, &["mcp_servers", "github"], &connector()).unwrap();
        assert!(out.contains("command = \"npx\""), "{out}");
        assert!(!out.contains("\"old\""), "old definition should be gone: {out}");
        assert!(out.contains("[mcp_servers.other]"), "{out}");
        assert!(out.contains("command = \"keep\""), "{out}");
    }

    #[test]
    fn a_nested_object_becomes_a_sub_table() {
        let value = json!({ "command": "npx", "env": { "TOKEN": "abc" } });
        let out = set_path("", &["mcp_servers", "github"], &value).unwrap();
        assert!(out.contains("[mcp_servers.github.env]"), "{out}");
        assert!(out.contains("TOKEN = \"abc\""), "{out}");
    }

    #[test]
    fn writing_twice_is_stable() {
        // Applying the same connector to its own output must not drift the
        // file, or every sync would show a change that is not one.
        let once = set_path("model = \"gpt-5\"\n", &["mcp_servers", "github"], &connector()).unwrap();
        let twice = set_path(&once, &["mcp_servers", "github"], &connector()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn invalid_toml_is_refused_rather_than_rewritten() {
        let err = set_path("this is [not toml", &["mcp_servers", "github"], &connector()).unwrap_err();
        assert!(matches!(err, TomlPatchError::InvalidToml(_)), "{err:?}");
    }

    #[test]
    fn a_null_field_is_refused() {
        let value = json!({ "command": "npx", "cwd": null });
        let err = set_path("", &["mcp_servers", "github"], &value).unwrap_err();
        assert!(matches!(err, TomlPatchError::UnrepresentableValue(_)), "{err:?}");
    }

    #[test]
    fn a_scalar_where_a_table_is_expected_is_refused() {
        let err = set_path("mcp_servers = 1\n", &["mcp_servers", "github"], &connector()).unwrap_err();
        assert!(matches!(err, TomlPatchError::PathSegmentNotTable(_)), "{err:?}");
    }

    #[test]
    fn the_written_table_reads_back_through_the_codex_reader_shape() {
        // Proves the writer's output is the shape the reader looks for, so a
        // written connector is one GitWyrm can list again afterwards.
        let out = set_path("", &["mcp_servers", "github"], &connector()).unwrap();
        let parsed: DocumentMut = out.parse().unwrap();
        let table = parsed["mcp_servers"]["github"].as_table().expect("table");
        assert_eq!(table["command"].as_str(), Some("npx"));
    }
}
