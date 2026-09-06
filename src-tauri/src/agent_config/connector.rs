//! Translate one MCP connector between the shapes different clients read.
//!
//! Every client stores the same idea -- "run this command" or "call this
//! URL" -- in a different set of field names. Claude and Codex write a
//! command string beside an `args` array and an `env` map; OpenCode writes a
//! single `command` array, calls the same map `environment`, and refuses an
//! entry that does not declare `type` and `enabled`.
//!
//! Copying the fields across verbatim therefore produces a file the
//! destination parses and then ignores: the connector appears in its config
//! and never starts. That is worse than a failure, because nothing reports
//! it.
//!
//! So a copy goes through this module in two steps. [`Connector::parse`]
//! reads a source entry into one shape that has no client in it, and
//! [`Connector::to_dialect`] writes that shape back out the way the
//! destination expects. Fields this module does not model survive the round
//! trip untouched (see [`Connector::passthrough`]) so a client-specific
//! setting is never silently dropped -- but the caller is told which ones
//! they were, because a field that means something in one client may mean
//! nothing in the next.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::model::{ClientId, ExtraFields, JsonValue};

/// How one client spells a connector.
///
/// Derived from the destination's registry row rather than named at each
/// call site, so adding a client cannot forget to pick one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// A command string, an `args` array beside it, and `env` for the
    /// environment. Claude Code and VS Code Copilot both read this shape.
    CommandWithArgs,
    /// One `command` array with the program at its head, `environment` for
    /// the environment, and explicit `type`/`enabled` fields. OpenCode.
    CommandArray,
    /// `CommandWithArgs` written into TOML. Codex. Field names match
    /// `CommandWithArgs`; the difference is only which writer serialises it.
    Toml,
}

impl Dialect {
    /// The dialect `client` reads.
    ///
    /// Exhaustive on purpose: a new client id fails to compile here rather
    /// than defaulting to a shape nobody checked.
    pub fn of(client: ClientId) -> Self {
        match client {
            ClientId::OpenCode => Dialect::CommandArray,
            ClientId::Codex => Dialect::Toml,
            ClientId::ClaudeCode | ClientId::VsCodeCopilot => Dialect::CommandWithArgs,
        }
    }
}

/// How a connector is reached.
#[derive(Debug, Clone, PartialEq)]
pub enum Transport {
    /// Started as a local process.
    Local {
        /// The program to run, followed by its arguments. Always stored
        /// split, whichever way the source spelled it.
        command: Vec<String>,
    },
    /// Reached over HTTP.
    Remote {
        url: String,
        headers: BTreeMap<String, String>,
    },
    /// Neither shape was recognised. The entry is still copied, but the
    /// caller is warned rather than told a guess.
    Unknown,
}

/// One connector, in no client's dialect.
#[derive(Debug, Clone, PartialEq)]
pub struct Connector {
    pub transport: Transport,
    pub environment: BTreeMap<String, String>,
    /// True unless the source explicitly disabled it. A connector copied
    /// while switched off stays switched off.
    pub enabled: bool,
    /// Fields this module does not model, kept so a client-specific setting
    /// survives the copy instead of being dropped.
    pub passthrough: BTreeMap<String, Value>,
}

/// Field names this module owns. Anything else is passthrough.
const MODELLED: &[&str] = &[
    "command",
    "args",
    "env",
    "environment",
    "url",
    "headers",
    "type",
    "enabled",
];

fn string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(Value::Object(map)) = value {
        for (k, v) in map {
            // A number or bool in an env map is legal in several of these
            // files and means its printed form; only nested structures have
            // no sensible spelling, and those are left out rather than
            // stringified into something the client would not accept.
            let spelled = match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None,
            };
            if let Some(spelled) = spelled {
                out.insert(k.clone(), spelled);
            }
        }
    }
    out
}

impl Connector {
    /// Read a source entry into the shape with no client in it.
    ///
    /// Accepts every dialect rather than being told which one it is looking
    /// at, because the file on disk is the authority: a config hand-edited
    /// into another client's shape should still copy correctly.
    pub fn parse(extra: &ExtraFields) -> Self {
        let plain: Map<String, Value> =
            extra.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect();

        let transport = Self::parse_transport(&plain);

        // Both spellings are accepted on the way in; which one goes out is
        // the destination's business.
        let environment = if plain.contains_key("environment") {
            string_map(plain.get("environment"))
        } else {
            string_map(plain.get("env"))
        };

        let enabled = !matches!(plain.get("enabled"), Some(Value::Bool(false)));

        let passthrough = plain
            .iter()
            .filter(|(k, _)| !MODELLED.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Connector {
            transport,
            environment,
            enabled,
            passthrough,
        }
    }

    fn parse_transport(plain: &Map<String, Value>) -> Transport {
        // A URL wins over a command: an entry carrying both is a remote one
        // whose command field is left over from an edit, and calling the URL
        // is the recoverable mistake of the two.
        if let Some(Value::String(url)) = plain.get("url") {
            if !url.trim().is_empty() {
                return Transport::Remote {
                    url: url.trim().to_string(),
                    headers: string_map(plain.get("headers")),
                };
            }
        }

        match plain.get("command") {
            // OpenCode's shape: the whole invocation in one array.
            Some(Value::Array(parts)) => {
                let command: Vec<String> = parts
                    .iter()
                    .filter_map(|p| match p {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    })
                    .collect();
                if command.is_empty() {
                    Transport::Unknown
                } else {
                    Transport::Local { command }
                }
            }
            // Claude/Codex/Copilot's shape: program first, arguments beside
            // it. Joined here so the destination never has to know which one
            // the source used.
            Some(Value::String(program)) if !program.trim().is_empty() => {
                let mut command = vec![program.clone()];
                if let Some(Value::Array(args)) = plain.get("args") {
                    for a in args {
                        match a {
                            Value::String(s) => command.push(s.clone()),
                            Value::Number(n) => command.push(n.to_string()),
                            _ => {}
                        }
                    }
                }
                Transport::Local { command }
            }
            _ => Transport::Unknown,
        }
    }

    /// Write this connector out the way `dialect` expects.
    pub fn to_dialect(&self, dialect: Dialect) -> ExtraFields {
        let mut out: Map<String, Value> = Map::new();

        match (&self.transport, dialect) {
            (Transport::Local { command }, Dialect::CommandArray) => {
                out.insert("type".into(), Value::String("local".into()));
                out.insert(
                    "command".into(),
                    Value::Array(command.iter().cloned().map(Value::String).collect()),
                );
            }
            (Transport::Local { command }, _) => {
                // Split back apart: head is the program, tail the arguments.
                // `command` is never empty for a `Local`, so the head always
                // exists -- but a `split_first` keeps that provable rather
                // than indexed.
                if let Some((program, args)) = command.split_first() {
                    out.insert("command".into(), Value::String(program.clone()));
                    if !args.is_empty() {
                        out.insert(
                            "args".into(),
                            Value::Array(args.iter().cloned().map(Value::String).collect()),
                        );
                    }
                }
            }
            (Transport::Remote { url, headers }, Dialect::CommandArray) => {
                out.insert("type".into(), Value::String("remote".into()));
                out.insert("url".into(), Value::String(url.clone()));
                if !headers.is_empty() {
                    out.insert("headers".into(), header_object(headers));
                }
            }
            (Transport::Remote { url, headers }, _) => {
                out.insert("url".into(), Value::String(url.clone()));
                if !headers.is_empty() {
                    out.insert("headers".into(), header_object(headers));
                }
            }
            (Transport::Unknown, _) => {}
        }

        if !self.environment.is_empty() {
            let key = match dialect {
                Dialect::CommandArray => "environment",
                _ => "env",
            };
            out.insert(key.into(), header_object(&self.environment));
        }

        // OpenCode treats a missing `enabled` as true, so it is only worth
        // writing when it carries information. The other dialects have no
        // such field and would gain an unread one.
        match dialect {
            Dialect::CommandArray => {
                out.insert("enabled".into(), Value::Bool(self.enabled));
            }
            _ => {
                if !self.enabled {
                    out.insert("enabled".into(), Value::Bool(false));
                }
            }
        }

        for (k, v) in &self.passthrough {
            out.insert(k.clone(), v.clone());
        }

        out.into_iter().map(|(k, v)| (k, JsonValue(v))).collect()
    }

    /// The names of fields carried across without being understood.
    ///
    /// Reported to the person doing the copy: these are the fields most
    /// likely to mean something different, or nothing at all, in the
    /// destination.
    pub fn unmodelled_field_names(&self) -> Vec<String> {
        self.passthrough.keys().cloned().collect()
    }
}

fn header_object(map: &BTreeMap<String, String>) -> Value {
    Value::Object(
        map.iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect(),
    )
}

/// Translate `extra` from whatever it is into what `destination` reads.
///
/// The single entry point used by the write path, so no caller can copy an
/// entry without it going through the translation.
pub fn translate(extra: &ExtraFields, destination: ClientId) -> (ExtraFields, Vec<String>) {
    let connector = Connector::parse(extra);
    let translated = connector.to_dialect(Dialect::of(destination));
    (translated, connector.unmodelled_field_names())
}

/// Whether this entry's transport was recognised at all.
///
/// A copy still proceeds when it was not -- refusing would strand entries
/// whose shape is simply newer than this module -- but the person is told,
/// because an unrecognised entry is the one case where the copied file may
/// genuinely not work.
pub fn transport_understood(extra: &ExtraFields) -> bool {
    !matches!(Connector::parse(extra).transport, Transport::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extra(json: Value) -> ExtraFields {
        match json {
            Value::Object(map) => map.into_iter().map(|(k, v)| (k, JsonValue(v))).collect(),
            _ => panic!("test fixture must be an object"),
        }
    }

    fn plain(fields: &ExtraFields) -> Value {
        Value::Object(fields.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect())
    }

    /// The defect this module exists for: a Claude entry copied to OpenCode
    /// used to arrive with a command string and an `args` array, which
    /// OpenCode parses and ignores.
    #[test]
    fn a_claude_command_becomes_an_opencode_command_array() {
        let source = extra(serde_json::json!({
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-git"],
            "env": { "GIT_ROOT": "/repo" }
        }));

        let (out, _) = translate(&source, ClientId::OpenCode);

        assert_eq!(
            plain(&out),
            serde_json::json!({
                "type": "local",
                "command": ["npx", "-y", "@modelcontextprotocol/server-git"],
                "environment": { "GIT_ROOT": "/repo" },
                "enabled": true
            })
        );
    }

    /// And the reverse: OpenCode's array split back into the shape Claude
    /// reads, without `type`/`enabled` fields Claude has no use for.
    #[test]
    fn an_opencode_command_array_becomes_a_claude_command_and_args() {
        let source = extra(serde_json::json!({
            "type": "local",
            "command": ["uvx", "mcp-server-time", "--local-timezone=UTC"],
            "environment": { "TZ": "UTC" },
            "enabled": true
        }));

        let (out, _) = translate(&source, ClientId::ClaudeCode);

        assert_eq!(
            plain(&out),
            serde_json::json!({
                "command": "uvx",
                "args": ["mcp-server-time", "--local-timezone=UTC"],
                "env": { "TZ": "UTC" }
            })
        );
    }

    #[test]
    fn a_remote_connector_keeps_its_url_and_headers_in_both_directions() {
        let source = extra(serde_json::json!({
            "url": "https://example.test/mcp",
            "headers": { "X-Key": "abc" }
        }));

        let (to_opencode, _) = translate(&source, ClientId::OpenCode);
        assert_eq!(
            plain(&to_opencode),
            serde_json::json!({
                "type": "remote",
                "url": "https://example.test/mcp",
                "headers": { "X-Key": "abc" },
                "enabled": true
            })
        );

        let (back, _) = translate(&to_opencode, ClientId::ClaudeCode);
        assert_eq!(
            plain(&back),
            serde_json::json!({
                "url": "https://example.test/mcp",
                "headers": { "X-Key": "abc" }
            })
        );
    }

    /// A local entry must never arrive carrying `url`/`headers`: OpenCode
    /// deletes them for local entries, and leaving them in invites an entry
    /// that looks remote to a reader that is not OpenCode.
    #[test]
    fn a_local_entry_does_not_carry_remote_fields() {
        let source = extra(serde_json::json!({ "command": "npx", "args": ["x"] }));
        let (out, _) = translate(&source, ClientId::OpenCode);
        assert!(!out.contains_key("url"));
        assert!(!out.contains_key("headers"));
    }

    #[test]
    fn a_disabled_connector_stays_disabled_across_a_copy() {
        let source = extra(serde_json::json!({
            "type": "local",
            "command": ["npx", "x"],
            "enabled": false
        }));

        let (to_claude, _) = translate(&source, ClientId::ClaudeCode);
        assert_eq!(to_claude.get("enabled").map(|v| &v.0), Some(&Value::Bool(false)));

        let (back, _) = translate(&to_claude, ClientId::OpenCode);
        assert_eq!(back.get("enabled").map(|v| &v.0), Some(&Value::Bool(false)));
    }

    /// An enabled connector gains no `enabled: false`, and picks up
    /// OpenCode's explicit `true` only where that field is read.
    #[test]
    fn an_enabled_connector_gains_no_disabled_flag() {
        let source = extra(serde_json::json!({ "command": "npx" }));
        let (to_claude, _) = translate(&source, ClientId::ClaudeCode);
        assert!(!to_claude.contains_key("enabled"));
    }

    #[test]
    fn a_field_this_module_does_not_model_survives_and_is_reported() {
        let source = extra(serde_json::json!({
            "command": "npx",
            "clientSpecificFlag": true
        }));

        let (out, unmodelled) = translate(&source, ClientId::OpenCode);

        assert_eq!(out.get("clientSpecificFlag").map(|v| &v.0), Some(&Value::Bool(true)));
        assert_eq!(unmodelled, vec!["clientSpecificFlag".to_string()]);
    }

    /// Round-tripping must not accumulate fields: copying A->B->A has to
    /// land back on the original, or a connector degrades a little with
    /// every copy.
    #[test]
    fn a_round_trip_returns_the_original_shape() {
        let original = extra(serde_json::json!({
            "command": "npx",
            "args": ["-y", "server"],
            "env": { "K": "v" }
        }));

        let (out, _) = translate(&original, ClientId::OpenCode);
        let (back, _) = translate(&out, ClientId::ClaudeCode);

        assert_eq!(plain(&back), plain(&original));
    }

    #[test]
    fn an_entry_with_no_command_or_url_is_reported_as_not_understood() {
        let source = extra(serde_json::json!({ "somethingElse": 1 }));
        assert!(!transport_understood(&source));

        let understood = extra(serde_json::json!({ "command": "npx" }));
        assert!(transport_understood(&understood));
    }

    /// The transport is read from the file, not from which client wrote it,
    /// so a config hand-edited into another shape still copies correctly.
    #[test]
    fn a_command_array_found_in_a_claude_file_is_still_understood() {
        let source = extra(serde_json::json!({ "command": ["npx", "-y", "srv"] }));
        let (out, _) = translate(&source, ClientId::ClaudeCode);
        assert_eq!(
            plain(&out),
            serde_json::json!({ "command": "npx", "args": ["-y", "srv"] })
        );
    }

    /// Every client id must map to a dialect somebody checked, so a new
    /// client cannot inherit a shape by accident.
    #[test]
    fn every_client_has_a_dialect() {
        for client in ClientId::ALL {
            let _ = Dialect::of(client);
        }
    }

    /// Codex is TOML but spells its fields the same way Claude does; a copy
    /// between them must not reshape the entry.
    #[test]
    fn codex_and_claude_share_a_field_shape() {
        let source = extra(serde_json::json!({ "command": "npx", "args": ["a"] }));
        let (to_codex, _) = translate(&source, ClientId::Codex);
        let (to_claude, _) = translate(&source, ClientId::ClaudeCode);
        assert_eq!(plain(&to_codex), plain(&to_claude));
    }

    /// An env value written as a number is still an environment variable;
    /// dropping it would change how the connector runs.
    #[test]
    fn a_non_string_environment_value_is_kept_as_its_printed_form() {
        let source = extra(serde_json::json!({
            "command": "npx",
            "env": { "PORT": 8080, "DEBUG": true }
        }));
        let (out, _) = translate(&source, ClientId::OpenCode);
        assert_eq!(
            out.get("environment").map(|v| &v.0),
            Some(&serde_json::json!({ "PORT": "8080", "DEBUG": "true" }))
        );
    }
}
