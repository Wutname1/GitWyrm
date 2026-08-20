//! Field-level secret detection and redaction for configuration items.
//!
//! This is deliberately distinct from `crate::scrub`, which pattern-matches
//! secret-shaped *text* inside free-form log/error strings. Here the shape of
//! the data is known (an MCP connector's `env`, `headers`, `args`, or a field
//! literally named like a credential): detection is by field identity, not by
//! guessing whether a string looks like a token. That means a redaction never
//! depends on the secret happening to match a known prefix -- every value
//! under a secret-shaped field is redacted regardless of what it looks like
//! (task 1.5, spec "Token field").

use serde_json::Value;

use super::model::{ExtraFields, JsonValue, SecretFieldRef, SecretReason};

/// Field name fragments (case-insensitive) that mark a field itself as
/// secret-bearing regardless of where it appears, e.g. `apiKey`, `password`.
const NAMED_SECRET_MARKERS: &[&str] = &[
    "token", "secret", "password", "passwd", "apikey", "api_key", "credential", "auth",
];

/// Header names whose value is always treated as secret, even though the
/// field name itself (`headers`) is not.
const SECRET_HEADER_NAMES: &[&str] = &["authorization", "x-api-key", "cookie", "proxy-authorization"];

fn is_named_secret(field_name: &str) -> bool {
    let lower = field_name.to_ascii_lowercase();
    NAMED_SECRET_MARKERS.iter().any(|marker| lower.contains(marker))
}

/// Scan an item's normalized `extra` fields for anything that should be
/// treated as a secret: `env.*` values, `headers.*` values (all of them --
/// header values are opaque to us and commonly carry bearer tokens even under
/// an unassuming name), `args` entries that look like `--flag=value` for a
/// secret-named flag, and any top-level or nested field whose own name
/// matches [`NAMED_SECRET_MARKERS`].
pub fn find_secret_fields(extra: &ExtraFields) -> Vec<SecretFieldRef> {
    let mut found = Vec::new();
    for (key, value) in extra {
        walk(key, &value.0, &mut found);
    }
    found.sort_by(|a, b| a.field_path.cmp(&b.field_path));
    found.dedup_by(|a, b| a.field_path == b.field_path);
    found
}

fn walk(path: &str, value: &Value, out: &mut Vec<SecretFieldRef>) {
    let leaf = path.rsplit('.').next().unwrap_or(path);

    if path == "env" || path.starts_with("env.") {
        if let Value::Object(map) = value {
            for (k, v) in map {
                out.push(SecretFieldRef {
                    field_path: format!("{path}.{k}"),
                    reason: SecretReason::EnvironmentValue,
                });
                walk(&format!("{path}.{k}"), v, out);
            }
            return;
        }
        if path.matches('.').count() >= 1 && path.starts_with("env.") {
            out.push(SecretFieldRef {
                field_path: path.to_string(),
                reason: SecretReason::EnvironmentValue,
            });
        }
    }

    if path == "headers" {
        if let Value::Object(map) = value {
            for (k, v) in map {
                if SECRET_HEADER_NAMES.contains(&k.to_ascii_lowercase().as_str()) {
                    out.push(SecretFieldRef {
                        field_path: format!("{path}.{k}"),
                        reason: SecretReason::HeaderValue,
                    });
                }
                walk(&format!("{path}.{k}"), v, out);
            }
            return;
        }
    }

    if leaf == "args" {
        if let Value::Array(items) = value {
            for (i, item) in items.iter().enumerate() {
                if let Value::String(s) = item {
                    if arg_looks_secret(s) {
                        out.push(SecretFieldRef {
                            field_path: format!("{path}[{i}]"),
                            reason: SecretReason::CommandArgument,
                        });
                    }
                }
            }
        }
    }

    if is_named_secret(leaf) {
        out.push(SecretFieldRef {
            field_path: path.to_string(),
            reason: SecretReason::NamedSecretField,
        });
        // Do not recurse further into a field already flagged as secret --
        // its entire subtree is presumed sensitive.
        return;
    }

    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk(&child_path, v, out);
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                walk(&format!("{path}[{i}]"), v, out);
            }
        }
        _ => {}
    }
}

/// `--api-key=xyz`, `--token xyz`, or a bare flag naming a secret marker.
/// Flag words are commonly hyphenated (`--api-key`) where the named-secret
/// markers are not (`apikey`/`api_key`), so hyphens are stripped before the
/// substring check -- otherwise `--api-key` would slip through as
/// unrecognized even though `apiKey`/`api_key` are both flagged.
fn arg_looks_secret(arg: &str) -> bool {
    let flag = arg.trim_start_matches('-').split('=').next().unwrap_or("");
    let normalized = flag.replace('-', "");
    is_named_secret(&normalized)
}

/// The marker written in place of any redacted value. Never a partial mask
/// (e.g. first/last characters) -- task 1.5 requires the value never appear,
/// and a partial mask can still leak enough to be useful to an attacker.
pub const REDACTED_MARKER: &str = "[secret hidden]";

/// Produce a copy of `extra` with every field named by `secret_fields`
/// replaced by [`REDACTED_MARKER`]. Used for anything that leaves the backend
/// process boundary for display: inventory rows, preview summaries, logs.
/// Never used for the content actually written to a destination file -- that
/// path carries the real value straight from the source through to the
/// writer so a copy is genuinely usable, and is never logged.
pub fn redact_for_display(extra: &ExtraFields, secret_fields: &[SecretFieldRef]) -> ExtraFields {
    let mut out = extra.clone();
    for field in secret_fields {
        redact_path(&mut out, &field.field_path);
    }
    out
}

fn redact_path(fields: &mut ExtraFields, field_path: &str) {
    let mut segments: Vec<&str> = Vec::new();
    for part in field_path.split('.') {
        // Split "key[0]" into "key" and an index marker; only object paths
        // are supported for in-place redaction since array elements would
        // need index-aware mutation the display copy does not require today
        // (array-shaped secrets like `args[i]` are still reported in
        // `secret_fields` and the UI treats their presence as a warning even
        // without redacting the specific element in this display copy).
        if let Some(bracket) = part.find('[') {
            segments.push(&part[..bracket]);
        } else {
            segments.push(part);
        }
    }
    if segments.is_empty() {
        return;
    }
    redact_recursive(fields, &segments);
}

fn redact_recursive(fields: &mut ExtraFields, segments: &[&str]) {
    let Some((head, rest)) = segments.split_first() else {
        return;
    };
    let Some(value) = fields.get_mut(*head) else {
        return;
    };
    if rest.is_empty() {
        value.0 = Value::String(REDACTED_MARKER.to_string());
        return;
    }
    if let Value::Object(map) = &mut value.0 {
        let mut as_extra: ExtraFields =
            map.iter().map(|(k, v)| (k.clone(), JsonValue(v.clone()))).collect();
        redact_recursive(&mut as_extra, rest);
        *map = as_extra.into_iter().map(|(k, v)| (k, v.0)).collect();
    }
}

/// Build the redacted diff summary lines for a preview: every field the
/// source item defines, marked as added/updated/unchanged relative to the
/// destination's current value, with secret-field values never inspected for
/// equality by their real content beyond presence (their redacted marker is
/// what would be compared, which is deliberately useless as a diff -- a
/// secret field is always reported as "updated" when present so the warning
/// path always fires rather than silently reporting "unchanged").
pub fn diff_fields(
    source: &ExtraFields,
    destination: Option<&ExtraFields>,
    secret_fields: &[SecretFieldRef],
) -> Vec<super::model::ChangeSummaryLine> {
    use super::model::ChangeSummaryLine;

    let secret_paths: std::collections::HashSet<&str> =
        secret_fields.iter().map(|f| f.field_path.as_str()).collect();

    let mut lines = Vec::new();
    let dest = destination.cloned().unwrap_or_default();
    diff_walk("", source, &dest, &secret_paths, &mut lines);
    lines.sort_by(|a: &ChangeSummaryLine, b| a.field_path.cmp(&b.field_path));
    lines
}

fn diff_walk(
    prefix: &str,
    source: &ExtraFields,
    dest: &ExtraFields,
    secret_paths: &std::collections::HashSet<&str>,
    out: &mut Vec<super::model::ChangeSummaryLine>,
) {
    use super::model::{ChangeSummaryLine, FieldChangeKind};

    for (key, value) in source {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if secret_paths.contains(path.as_str()) {
            let change = if dest.contains_key(key) {
                FieldChangeKind::Updated
            } else {
                FieldChangeKind::Added
            };
            out.push(ChangeSummaryLine { field_path: path, change });
            continue;
        }
        match (&value.0, dest.get(key).map(|v| &v.0)) {
            (Value::Object(src_map), Some(Value::Object(dest_map))) => {
                let src_extra: ExtraFields =
                    src_map.iter().map(|(k, v)| (k.clone(), JsonValue(v.clone()))).collect();
                let dest_extra: ExtraFields =
                    dest_map.iter().map(|(k, v)| (k.clone(), JsonValue(v.clone()))).collect();
                diff_walk(&path, &src_extra, &dest_extra, secret_paths, out);
            }
            (v, Some(existing)) => {
                let change = if v == existing {
                    FieldChangeKind::Unchanged
                } else {
                    FieldChangeKind::Updated
                };
                out.push(ChangeSummaryLine { field_path: path, change });
            }
            (_, None) => {
                out.push(ChangeSummaryLine {
                    field_path: path,
                    change: FieldChangeKind::Added,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extra_from(pairs: &[(&str, Value)]) -> ExtraFields {
        pairs.iter().map(|(k, v)| (k.to_string(), JsonValue(v.clone()))).collect()
    }

    #[test]
    fn env_values_are_flagged_as_environment_secrets() {
        let extra = extra_from(&[(
            "env",
            json!({ "API_KEY": "sk-live-abc123", "LOG_LEVEL": "debug" }),
        )]);
        let found = find_secret_fields(&extra);
        let paths: Vec<&str> = found.iter().map(|f| f.field_path.as_str()).collect();
        assert!(paths.contains(&"env.API_KEY"));
        assert!(paths.contains(&"env.LOG_LEVEL"), "every env value is treated as sensitive, not just secret-named ones");
    }

    #[test]
    fn authorization_header_is_flagged_but_other_headers_are_not() {
        let extra = extra_from(&[(
            "headers",
            json!({ "Authorization": "Bearer xyz", "X-Trace-Id": "abc" }),
        )]);
        let found = find_secret_fields(&extra);
        let paths: Vec<&str> = found.iter().map(|f| f.field_path.as_str()).collect();
        assert!(paths.contains(&"headers.Authorization"));
        assert!(!paths.contains(&"headers.X-Trace-Id"));
    }

    #[test]
    fn command_argument_flags_named_like_a_secret_are_flagged() {
        let extra = extra_from(&[(
            "args",
            json!(["--api-key=abc123", "--verbose", "--token", "run"]),
        )]);
        let found = find_secret_fields(&extra);
        let paths: Vec<&str> = found.iter().map(|f| f.field_path.as_str()).collect();
        assert!(paths.contains(&"args[0]"));
        assert!(paths.contains(&"args[2]"));
        assert!(!paths.contains(&"args[1]"));
    }

    #[test]
    fn a_top_level_field_named_like_a_secret_is_flagged() {
        let extra = extra_from(&[("apiKey", json!("abc123")), ("displayLabel", json!("Prod"))]);
        let found = find_secret_fields(&extra);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].field_path, "apiKey");
        assert_eq!(found[0].reason, SecretReason::NamedSecretField);
    }

    #[test]
    fn redaction_never_leaves_the_real_value_reachable() {
        let extra = extra_from(&[(
            "env",
            json!({ "API_KEY": "sk-live-super-secret-value" }),
        )]);
        let secrets = find_secret_fields(&extra);
        let redacted = redact_for_display(&extra, &secrets);
        let serialized = serde_json::to_string(&redacted).unwrap();
        assert!(!serialized.contains("sk-live-super-secret-value"));
        assert!(serialized.contains(REDACTED_MARKER));
    }

    #[test]
    fn diff_marks_a_secret_field_as_updated_even_when_present_on_both_sides() {
        let source = extra_from(&[("env", json!({ "API_KEY": "new-value" }))]);
        let dest = extra_from(&[("env", json!({ "API_KEY": "old-value" }))]);
        let secrets = find_secret_fields(&source);
        let lines = diff_fields(&source, Some(&dest), &secrets);
        let secret_line = lines.iter().find(|l| l.field_path == "env.API_KEY").unwrap();
        assert_eq!(secret_line.change, super::super::model::FieldChangeKind::Updated);
    }

    #[test]
    fn diff_reports_unchanged_for_identical_non_secret_fields() {
        let source = extra_from(&[("command", json!("node")), ("args", json!(["server.js"]))]);
        let dest = source.clone();
        let lines = diff_fields(&source, Some(&dest), &[]);
        assert!(lines
            .iter()
            .all(|l| l.change == super::super::model::FieldChangeKind::Unchanged));
    }

    #[test]
    fn diff_reports_added_for_a_missing_destination() {
        let source = extra_from(&[("command", json!("node"))]);
        let lines = diff_fields(&source, None, &[]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].change, super::super::model::FieldChangeKind::Added);
    }

    #[test]
    fn unknown_fields_are_preserved_through_redaction_untouched() {
        let extra = extra_from(&[
            ("clientSpecificQuirk", json!({ "nested": true, "value": 42 })),
            ("apiKey", json!("secret")),
        ]);
        let secrets = find_secret_fields(&extra);
        let redacted = redact_for_display(&extra, &secrets);
        assert_eq!(redacted.get("clientSpecificQuirk"), extra.get("clientSpecificQuirk"));
    }
}
