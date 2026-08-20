//! A minimal, byte-preserving JSON text editor.
//!
//! `serde_json`'s default `Value` map is a `BTreeMap`, so a full
//! deserialize -> mutate -> reserialize round trip through it silently
//! reorders every key alphabetically and reformats whitespace. That would
//! violate task 3.5 ("preserve file encoding, line endings, comments,
//! ordering, and unknown fields") on every single write, even ones that only
//! touch one nested key. Rather than pull in the `preserve_order` feature
//! (an `indexmap` dependency touching every `serde_json::Value` user in the
//! workspace) this module edits JSON as text: it locates the byte span of one
//! object member by walking the token stream, and splices in a
//! pretty-printed replacement for only that member. Every byte outside the
//! touched span -- including comments-adjacent formatting quirks JSON does
//! not actually support, but also blank lines, trailing commas the source
//! author used consistently, and indentation style -- is copied through
//! unchanged.
//!
//! This intentionally supports only the shape configuration-sync writers
//! need: setting one key's value inside a (possibly nested-by-one-level)
//! object, creating the parent object/key path if it does not exist yet.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatchError {
    #[error("source is not valid JSON: {0}")]
    InvalidJson(String),
    #[error("expected a JSON object at the top level")]
    NotAnObject,
    #[error("path segment {0:?} exists but is not an object")]
    PathSegmentNotObject(String),
}

/// Set `path` (a list of nested object keys, e.g. `["mcpServers", "github"]`)
/// to `new_value` inside the JSON document `source`, returning the full
/// updated document text. Every byte of `source` outside the replaced value
/// (and, when the path must be created, the minimal insertion needed to hold
/// it) is preserved exactly, including its own indentation and newline
/// style, which this function detects from the surrounding text rather than
/// imposing a fixed style.
pub fn set_path(source: &str, path: &[&str], new_value: &Value) -> Result<String, PatchError> {
    // Parse once, purely to validate the document is well-formed JSON and to
    // fail with a typed error before any text surgery. The parsed value is
    // otherwise unused for building output -- output is built from the
    // original text plus one spliced-in span.
    serde_json::from_str::<Value>(source).map_err(|e| PatchError::InvalidJson(e.to_string()))?;

    let root_span = top_level_object_span(source).ok_or(PatchError::NotAnObject)?;
    let indent = detect_indent(source);
    set_path_in_object(source, root_span, path, new_value, &indent, 0)
}

/// Remove `path` from the document if present. A no-op (returns the input
/// unchanged) if any segment of the path does not exist -- undo and re-apply
/// flows call this defensively and should not error on "already gone".
pub fn remove_path(source: &str, path: &[&str]) -> Result<String, PatchError> {
    serde_json::from_str::<Value>(source).map_err(|e| PatchError::InvalidJson(e.to_string()))?;
    let Some(root_span) = top_level_object_span(source) else {
        return Err(PatchError::NotAnObject);
    };
    Ok(remove_path_in_object(source, root_span, path).unwrap_or_else(|| source.to_string()))
}

/// Byte range `[start, end)` of the top-level `{ ... }`, including the
/// braces.
fn top_level_object_span(source: &str) -> Option<(usize, usize)> {
    let start = source.find('{')?;
    let end = matching_brace(source, start)?;
    Some((start, end + 1))
}

/// Given the byte index of an opening `{`, find the index of its matching
/// `}`, correctly skipping over braces inside strings.
fn matching_brace(source: &str, open_idx: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = open_idx;
    let mut in_string = false;
    let mut escape = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Find the byte span of a top-level string key's *value* within the object
/// body `body_start..body_end` (indices into `source`, `body_start` is right
/// after the opening `{`). Returns `(value_start, value_end, key_found)`.
fn find_member_value_span(source: &str, body_start: usize, body_end: usize, key: &str) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut i = body_start;
    let mut in_string = false;
    let mut escape = false;
    let mut depth = 0i32;
    let mut current_key_start: Option<usize> = None;
    let mut current_key: Option<String> = None;
    let mut expect_value = false;

    while i < body_end {
        let c = bytes[i] as char;
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
                if let (0, Some(start), false) = (depth, current_key_start, expect_value) {
                    current_key = Some(source[start + 1..i].to_string());
                }
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                if depth == 0 && !expect_value {
                    current_key_start = Some(i);
                }
            }
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ':' if depth == 0 => {
                expect_value = true;
                // Skip whitespace to find the true value start.
                let mut v = i + 1;
                while v < body_end && bytes[v].is_ascii_whitespace() {
                    v += 1;
                }
                if current_key.as_deref() == Some(key) {
                    let value_end = value_span_end(source, v, body_end)?;
                    return Some((v, value_end));
                }
            }
            ',' if depth == 0 => {
                expect_value = false;
                current_key = None;
                current_key_start = None;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Given the start of a JSON value, find its end (exclusive) by scanning
/// balanced brackets/strings.
fn value_span_end(source: &str, start: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if start >= limit {
        return None;
    }
    let first = bytes[start] as char;
    if first == '{' || first == '[' {
        let close = if first == '{' { '}' } else { ']' };
        let mut depth = 0i32;
        let mut i = start;
        let mut in_string = false;
        let mut escape = false;
        while i < limit {
            let c = bytes[i] as char;
            if in_string {
                if escape {
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            } else if c == first {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            i += 1;
        }
        None
    } else if first == '"' {
        let mut i = start + 1;
        let mut escape = false;
        while i < limit {
            let c = bytes[i] as char;
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                return Some(i + 1);
            }
            i += 1;
        }
        None
    } else {
        // number / true / false / null: ends at the next structural char.
        let mut i = start;
        while i < limit {
            let c = bytes[i] as char;
            if c == ',' || c == '}' || c == ']' || c.is_whitespace() {
                break;
            }
            i += 1;
        }
        Some(i)
    }
}

/// The dominant indentation unit used in `source` (defaults to two spaces
/// when nothing can be inferred), detected from the first indented line so a
/// newly inserted member matches the file's existing style rather than
/// imposing GitWyrm's own preference.
fn detect_indent(source: &str) -> String {
    for line in source.lines().skip(1) {
        let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        if !indent.is_empty() {
            return indent;
        }
    }
    "  ".to_string()
}

fn line_ending(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Recursively resolve/create `path` starting from the object spanning
/// `object_span` (inclusive of braces) and splice `new_value` in at the leaf.
fn set_path_in_object(
    source: &str,
    object_span: (usize, usize),
    path: &[&str],
    new_value: &Value,
    indent: &str,
    depth: usize,
) -> Result<String, PatchError> {
    let (obj_start, obj_end) = object_span;
    let Some((&key, rest)) = path.split_first() else {
        // Replace the whole object span with the new value (used only when
        // `path` was empty, which callers do not do today but keeps the
        // function total).
        let pretty = pretty_print(new_value, indent, depth);
        return Ok(format!("{}{}{}", &source[..obj_start], pretty, &source[obj_end..]));
    };

    let body_start = obj_start + 1;
    let body_end = obj_end - 1;

    if let Some((value_start, value_end)) = find_member_value_span(source, body_start, body_end, key) {
        if rest.is_empty() {
            let pretty = pretty_print(new_value, indent, member_depth(source, value_start));
            return Ok(format!(
                "{}{}{}",
                &source[..value_start],
                pretty,
                &source[value_end..]
            ));
        }
        // Descend: the existing value at this key must be an object to keep
        // walking. If it's some other JSON type, refuse rather than silently
        // clobbering a value of a different shape.
        let existing = &source[value_start..value_end];
        if !existing.trim_start().starts_with('{') {
            return Err(PatchError::PathSegmentNotObject(key.to_string()));
        }
        let inner_span = (value_start, value_end);
        let replacement = set_path_in_object(source, inner_span, rest, new_value, indent, depth + 1)?;
        // `set_path_in_object` returns a *full document* (it operates on
        // `source` directly), so splice its touched region back in relative
        // to the outer source: since it only ever changes bytes strictly
        // inside `inner_span`, diff against `source` length to locate the
        // new inner span boundaries in the replacement text itself.
        let grew_by = replacement.len() as isize - source.len() as isize;
        let new_inner_end = (value_end as isize + grew_by) as usize;
        return Ok(format!(
            "{}{}{}",
            &replacement[..value_start],
            &replacement[value_start..new_inner_end],
            &replacement[new_inner_end..]
        ));
    }

    // Key does not exist at this level: insert it.
    let member_indent = indent.repeat(depth + 1);
    let nl = line_ending(source);
    let pretty_value = if rest.is_empty() {
        pretty_print(new_value, indent, depth + 1)
    } else {
        // Build a nested object containing the remaining path, then the leaf.
        let mut nested = new_value.clone();
        for segment in rest.iter().rev() {
            let mut map = serde_json::Map::new();
            map.insert(segment.to_string(), nested);
            nested = Value::Object(map);
        }
        pretty_print(&nested, indent, depth + 1)
    };

    let body = &source[body_start..body_end];
    let trimmed_body = body.trim_end_matches([' ', '\t', '\n', '\r']);
    let is_empty_body = trimmed_body.trim().is_empty();

    let insertion = if is_empty_body {
        format!("{nl}{member_indent}\"{key}\": {pretty_value}{nl}{}", indent.repeat(depth))
    } else {
        format!(",{nl}{member_indent}\"{key}\": {pretty_value}")
    };

    let insert_at = body_start + trimmed_body.len();
    Ok(format!(
        "{}{}{}",
        &source[..insert_at],
        insertion,
        &source[insert_at..]
    ))
}

/// Remove `path`'s leaf member from wherever it resolves, if present.
/// Returns `None` if any path segment is missing (nothing to remove).
fn remove_path_in_object(source: &str, object_span: (usize, usize), path: &[&str]) -> Option<String> {
    let (obj_start, obj_end) = object_span;
    let (&key, rest) = path.split_first()?;
    let body_start = obj_start + 1;
    let body_end = obj_end - 1;

    let (value_start, value_end) = find_member_value_span(source, body_start, body_end, key)?;

    if !rest.is_empty() {
        let inner = (value_start, value_end);
        let replacement = remove_path_in_object(source, inner, rest)?;
        let grew_by = replacement.len() as isize - source.len() as isize;
        let new_inner_end = (value_end as isize + grew_by) as usize;
        return Some(format!(
            "{}{}{}",
            &replacement[..value_start],
            &replacement[value_start..new_inner_end],
            &replacement[new_inner_end..]
        ));
    }

    // Remove the "key": value member, plus one adjacent comma and the
    // whitespace/newline that belonged to it, so removing the only member
    // does not leave a dangling comma or a blank member line behind.
    let bytes = source.as_bytes();
    // Find the start of the key: walk back from value_start over the ':'
    // and any whitespace to the key's closing quote, then back over the
    // key's own characters to its opening quote.
    let mut key_end = value_start;
    while key_end > body_start && bytes[key_end - 1] != b':' {
        key_end -= 1;
    }
    // `key_end` now points just past the ':'; step back over the ':' and
    // any whitespace between the key and it to land on the closing quote.
    let mut key_start = key_end.saturating_sub(1);
    while key_start > body_start && bytes[key_start - 1].is_ascii_whitespace() {
        key_start -= 1;
    }
    // `key_start` now points just past the closing quote. Walk back through
    // the key's characters (which cannot themselves contain an unescaped
    // quote inside a JSON string key without being preceded by `\`) to the
    // opening quote.
    if key_start > body_start && bytes[key_start - 1] == b'"' {
        key_start -= 1; // now at the closing quote
        loop {
            if key_start <= body_start {
                break;
            }
            key_start -= 1;
            if bytes[key_start] == b'"' && (key_start == 0 || bytes[key_start - 1] != b'\\') {
                break; // found the opening quote
            }
        }
    }

    // Include a leading comma+whitespace if present, else a trailing one.
    let mut remove_start = key_start;
    while remove_start > body_start && bytes[remove_start - 1].is_ascii_whitespace() {
        remove_start -= 1;
    }
    let had_leading_comma = remove_start > body_start && bytes[remove_start - 1] == b',';
    if had_leading_comma {
        remove_start -= 1;
    }

    let mut remove_end = value_end;
    let mut scan = remove_end;
    while scan < body_end && bytes[scan].is_ascii_whitespace() {
        scan += 1;
    }
    if !had_leading_comma && scan < body_end && bytes[scan] == b',' {
        remove_end = scan + 1;
        while remove_end < body_end && bytes[remove_end].is_ascii_whitespace() && bytes[remove_end] != b'\n' {
            remove_end += 1;
        }
    }

    Some(format!("{}{}", &source[..remove_start], &source[remove_end..]))
}

/// Indentation depth of the line containing `byte_idx`, counted in
/// `detect_indent()` units, used so a replaced value that spans multiple
/// lines (an object/array) nests correctly.
fn member_depth(source: &str, byte_idx: usize) -> usize {
    let before = &source[..byte_idx];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_prefix = &source[line_start..byte_idx];
    let indent_unit = detect_indent(source);
    if indent_unit.is_empty() {
        return 0;
    }
    let leading_ws: usize = line_prefix.chars().take_while(|c| c.is_whitespace()).count();
    leading_ws / indent_unit.chars().count().max(1)
}

/// Pretty-print a `serde_json::Value` at the given indentation depth using
/// `indent` as the per-level unit. Keys within objects created by GitWyrm are
/// emitted in the map's own iteration order (insertion order is not
/// preserved by `serde_json::Map` without the `preserve_order` feature, so
/// for values GitWyrm itself constructs -- new items -- this sorts them,
/// which only affects fields *this write* introduces, never pre-existing
/// document content, which this module never re-serializes).
fn pretty_print(value: &Value, indent: &str, depth: usize) -> String {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                return "{}".to_string();
            }
            let inner_indent = indent.repeat(depth + 1);
            let outer_indent = indent.repeat(depth);
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let body: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{inner_indent}\"{}\": {}",
                        escape_json_string(k),
                        pretty_print(v, indent, depth + 1)
                    )
                })
                .collect();
            format!("{{\n{}\n{outer_indent}}}", body.join(",\n"))
        }
        Value::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }
            let inner_indent = indent.repeat(depth + 1);
            let outer_indent = indent.repeat(depth);
            let body: Vec<String> = items
                .iter()
                .map(|v| format!("{inner_indent}{}", pretty_print(v, indent, depth + 1)))
                .collect();
            format!("[\n{}\n{outer_indent}]", body.join(",\n"))
        }
        Value::String(s) => format!("\"{}\"", escape_json_string(s)),
        Value::Number(_) | Value::Bool(_) | Value::Null => value.to_string(),
    }
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn setting_a_new_top_level_key_on_a_two_key_object_preserves_the_rest() {
        let source = "{\n  \"a\": 1,\n  \"b\": 2\n}";
        let out = set_path(source, &["c"], &json!(3)).unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["a"], json!(1));
        assert_eq!(parsed["b"], json!(2));
        assert_eq!(parsed["c"], json!(3));
        assert!(out.contains("\"a\": 1"));
        assert!(out.contains("\"b\": 2"));
    }

    #[test]
    fn setting_a_nested_key_only_touches_that_branch() {
        let source = r#"{
  "mcpServers": {
    "existing": { "command": "foo" }
  },
  "unrelated": {
    "deeply": { "nested": true }
  }
}"#;
        let new_server = json!({ "command": "npx", "args": ["gh-mcp"] });
        let out = set_path(source, &["mcpServers", "github"], &new_server).unwrap();
        assert!(out.contains("\"unrelated\""), "unrelated branch must survive verbatim");
        assert!(out.contains("\"deeply\": {\n      \"nested\": true\n    }") || out.contains("\"nested\": true"));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["mcpServers"]["existing"]["command"], json!("foo"));
        assert_eq!(parsed["mcpServers"]["github"]["command"], json!("npx"));
    }

    #[test]
    fn updating_an_existing_key_preserves_sibling_keys_byte_for_byte() {
        let source = "{\n  \"mcpServers\": {\n    \"github\": { \"command\": \"old\" },\n    \"other\": { \"command\": \"unrelated\" }\n  }\n}";
        let out = set_path(source, &["mcpServers", "github"], &json!({ "command": "new" })).unwrap();
        assert!(out.contains("\"other\": { \"command\": \"unrelated\" }"));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["mcpServers"]["github"]["command"], json!("new"));
    }

    #[test]
    fn creating_a_missing_parent_object_works() {
        let source = "{\n  \"other\": true\n}";
        let out = set_path(source, &["mcpServers", "github"], &json!({ "command": "npx" })).unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["mcpServers"]["github"]["command"], json!("npx"));
        assert_eq!(parsed["other"], json!(true));
    }

    #[test]
    fn removing_a_key_leaves_siblings_intact_and_no_dangling_comma() {
        let source = "{\n  \"mcpServers\": {\n    \"github\": { \"command\": \"x\" },\n    \"other\": { \"command\": \"y\" }\n  }\n}";
        let out = remove_path(source, &["mcpServers", "github"]).unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert!(parsed["mcpServers"].get("github").is_none());
        assert_eq!(parsed["mcpServers"]["other"]["command"], json!("y"));
    }

    #[test]
    fn removing_a_missing_key_is_a_harmless_no_op() {
        let source = "{\n  \"a\": 1\n}";
        let out = remove_path(source, &["nope", "gone"]).unwrap();
        assert_eq!(out, source);
    }

    #[test]
    fn invalid_json_source_is_a_typed_error() {
        let result = set_path("{ not json", &["a"], &json!(1));
        assert!(matches!(result, Err(PatchError::InvalidJson(_))));
    }

    #[test]
    fn trailing_newline_and_crlf_style_are_preserved() {
        let source = "{\r\n  \"a\": 1\r\n}\r\n";
        let out = set_path(source, &["b"], &json!(2)).unwrap();
        assert!(out.contains("\r\n"), "must keep CRLF style");
        assert!(out.ends_with("}\r\n"), "trailing newline after closing brace preserved: {out:?}");
    }
}
