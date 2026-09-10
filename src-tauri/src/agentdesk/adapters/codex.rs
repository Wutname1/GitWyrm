//! Codex adapter: reads `~/.codex/sessions/**/rollout-*.jsonl` and the
//! `session_index.jsonl` sidecar, read-only.
//!
//! Verified against a real installation's on-disk layout (2026-08):
//!
//! ```text
//! ~/.codex/
//!   session_index.jsonl        # {"id","thread_name","updated_at"} per line, title+time only
//!   sessions/<yyyy>/<mm>/<dd>/rollout-<timestamp>-<uuid>.jsonl
//! ```
//!
//! Each rollout file is JSON Lines. The record shapes actually observed:
//! - `{"type":"session_meta","payload":{"session_id","cwd","cli_version",...}}` -- once,
//!   near the top; `cwd` is the project path and `cli_version` is what
//!   `supported_version_range` is checked against.
//! - `{"type":"response_item","payload":{"type":"message","role":"user"|"assistant"|"developer","content":[{"type":"input_text"|"output_text","text":...}]}}`
//!   -- the actual turns. `developer` role carries injected instructions
//!   (permissions boilerplate, AGENTS.md) rather than conversation content;
//!   mapped to [`ExternalRole::System`] and still imported per task 2.1
//!   rather than dropped, since a future Codex build could put real content
//!   there.
//! - Everything else (`event_msg`, `turn_context`, and any tag this build has
//!   never seen) is preserved as a `raw_unrecognized` record rather than
//!   discarded, per task 2.1.
//!
//! `session_index.jsonl` is a cheap listing sidecar (title + timestamp only,
//! no message bodies), so [`CodexAdapter::list_sessions`] uses it and falls
//! back to scanning `sessions/` when it is missing or unreadable -- the
//! sidecar being absent is not itself a corrupt-session condition.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{
    open_read_only, read_foreign_file_to_string, AdapterError, AgentClientAdapter, AdapterId,
    ContinuationCapability, DetectedClient, ExternalMessage, ExternalRole,
    ExternalSessionDetail, ExternalSessionSummary,
};

/// Versions this parsing was validated against. Codex's CLI version moves
/// fast; anything parses (the shape has been stable), but only versions in
/// this range are marked `supported` for the detected-clients UI.
const SUPPORTED_RANGE: &str = ">=0.100.0, <1.0.0";

pub struct CodexAdapter {
    /// Overridable for tests/fixtures; production always uses `home_dir()`.
    home_override: Option<PathBuf>,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self { home_override: None }
    }
}

impl CodexAdapter {
    pub fn at(home: PathBuf) -> Self {
        Self {
            home_override: Some(home),
        }
    }

    fn codex_home(&self) -> Option<PathBuf> {
        if let Some(h) = &self.home_override {
            return Some(h.clone());
        }
        home_dir().map(|h| h.join(".codex"))
    }

    fn sessions_dir(client: &DetectedClient) -> PathBuf {
        client.home_dir.join("sessions")
    }

    fn index_path(client: &DetectedClient) -> PathBuf {
        client.home_dir.join("session_index.jsonl")
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn is_supported_version(version: &str) -> bool {
    // ">=0.100.0, <1.0.0" -- a minimal semver-major/minor gate rather than a
    // full range parser, matching how little this needs to express.
    let Some((major, minor)) = parse_major_minor(version) else {
        return false;
    };
    major == 0 && minor >= 100
}

fn parse_major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[derive(Debug, Deserialize)]
struct IndexLine {
    id: String,
    thread_name: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RolloutLine {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(rename = "type")]
    line_type: Option<String>,
    payload: Option<serde_json::Value>,
}

impl AgentClientAdapter for CodexAdapter {
    fn id(&self) -> AdapterId {
        "codex"
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn supported_version_range(&self) -> &'static str {
        SUPPORTED_RANGE
    }

    fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
        let Some(home) = self.codex_home() else {
            return Ok(None);
        };
        if !home.is_dir() {
            return Ok(None);
        }
        let version = read_cli_version(&home);
        let supported = version.as_deref().map(is_supported_version).unwrap_or(false);
        Ok(Some(DetectedClient {
            home_dir: home,
            version,
            supported,
        }))
    }

    fn list_sessions(
        &self,
        client: &DetectedClient,
    ) -> Result<Vec<ExternalSessionSummary>, AdapterError> {
        let index_path = Self::index_path(client);
        let mut by_id: std::collections::HashMap<String, (Option<String>, Option<String>)> =
            std::collections::HashMap::new();
        if index_path.is_file() {
            if let Ok(contents) = read_foreign_file_to_string(&index_path) {
                for line in contents.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Ok(entry) = serde_json::from_str::<IndexLine>(line) {
                        by_id.insert(entry.id, (entry.thread_name, entry.updated_at));
                    }
                    // A malformed line in the sidecar is skipped, not fatal --
                    // the file-scan fallback below still finds every session
                    // that has a rollout file, even ones missing from (or
                    // corrupted in) the index.
                }
            }
        }

        let sessions_dir = Self::sessions_dir(client);
        if !sessions_dir.is_dir() {
            return Err(AdapterError::MissingPath {
                path: sessions_dir.display().to_string(),
            });
        }

        let mut summaries = Vec::new();
        for path in walk_jsonl_files(&sessions_dir) {
            let Some(external_session_id) = session_id_from_filename(&path) else {
                continue;
            };
            let (title, updated_at) = by_id
                .get(&external_session_id)
                .cloned()
                .unwrap_or((None, None));

            // Cheap summary: read only the `session_meta` line for cwd/model,
            // and count remaining lines as a message-count upper bound,
            // without fully parsing every payload (that happens in
            // `read_session`).
            let meta = read_session_meta(&path);
            let updated_at = updated_at
                .or_else(|| meta.as_ref().and_then(|m| m.file_mtime.clone()))
                .unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
            let title = title.unwrap_or_else(|| "Codex session".into());
            let message_count = count_lines(&path);

            summaries.push(ExternalSessionSummary {
                external_session_id,
                title,
                updated_at,
                project_path: meta.as_ref().and_then(|m| m.cwd.clone()),
                message_count,
                model: meta.and_then(|m| m.model),
            });
        }

        super::sort_newest_first(&mut summaries);
        Ok(summaries)
    }

    fn read_session(
        &self,
        client: &DetectedClient,
        external_session_id: &str,
    ) -> Result<ExternalSessionDetail, AdapterError> {
        let sessions_dir = Self::sessions_dir(client);
        let path = find_rollout_file(&sessions_dir, external_session_id).ok_or_else(|| {
            AdapterError::SessionNotFound {
                external_session_id: external_session_id.into(),
            }
        })?;

        let contents = read_foreign_file_to_string(&path)?;
        let mut messages = Vec::new();
        let mut cwd = None;
        let mut model = None;
        let mut last_timestamp = None;
        let mut seen_any_line = false;
        let mut parsed_any_line = false;

        for (i, line) in contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            seen_any_line = true;
            let parsed: Result<RolloutLine, _> = serde_json::from_str(line);
            let Ok(parsed) = parsed else {
                // One bad line does not sink the whole session -- it becomes
                // a labeled raw-import record (task 2.1), still imported.
                messages.push(ExternalMessage {
                    external_message_id: format!("{external_session_id}#{i}"),
                    role: ExternalRole::System,
                    timestamp: last_timestamp.clone().unwrap_or_else(|| "1970-01-01T00:00:00Z".into()),
                    plain_content: String::new(),
                    model: None,
                    raw_unrecognized: true,
                });
                continue;
            };
            parsed_any_line = true;
            last_timestamp = parsed.timestamp.clone().or(last_timestamp);

            if parsed.line_type.as_deref() == Some("session_meta") {
                if let Some(payload) = &parsed.payload {
                    cwd = payload
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .or(cwd);
                    model = payload
                        .get("model")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .or(model);
                }
                continue;
            }

            let Some((role, text)) = extract_response_message(&parsed) else {
                messages.push(ExternalMessage {
                    external_message_id: format!("{external_session_id}#{i}"),
                    role: ExternalRole::System,
                    timestamp: parsed.timestamp.unwrap_or_else(|| "1970-01-01T00:00:00Z".into()),
                    plain_content: String::new(),
                    model: None,
                    raw_unrecognized: true,
                });
                continue;
            };

            messages.push(ExternalMessage {
                external_message_id: format!("{external_session_id}#{i}"),
                role,
                timestamp: parsed.timestamp.unwrap_or_else(|| "1970-01-01T00:00:00Z".into()),
                plain_content: text,
                model: model.clone(),
                raw_unrecognized: false,
            });
        }

        if !seen_any_line || !parsed_any_line {
            return Err(AdapterError::CorruptSession {
                detail: format!("{} contained no parseable lines", path.display()),
            });
        }

        let updated_at = last_timestamp.unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
        Ok(ExternalSessionDetail {
            summary: ExternalSessionSummary {
                external_session_id: external_session_id.to_string(),
                title: "Codex session".into(),
                updated_at,
                project_path: cwd,
                message_count: messages.len() as u32,
                model,
            },
            messages,
        })
    }

    fn continuation_capability(
        &self,
        _client: &DetectedClient,
        _external_session_id: &str,
    ) -> ContinuationCapability {
        // Codex supports `codex resume <id>` from its documented CLI, which
        // GitWyrm can launch the same way `commands::opencode` launches
        // opencode's own resume today -- but wiring that launch is separate
        // from reading history, and is not implemented in this change.
        // Honest until that launch exists: open-only, not resume.
        ContinuationCapability::OpenOnly
    }
}

struct SessionMeta {
    cwd: Option<String>,
    model: Option<String>,
    file_mtime: Option<String>,
}

fn read_session_meta(path: &Path) -> Option<SessionMeta> {
    let file = open_read_only(path).ok()?;
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(file);
    let mtime = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| {
            let dt: time::OffsetDateTime = t.into();
            dt.format(&time::format_description::well_known::Rfc3339).ok()
        });

    // Only the first several lines are ever scanned for session_meta -- it is
    // documented to appear near the top of the file.
    for line in reader.lines().take(20).flatten() {
        if let Ok(parsed) = serde_json::from_str::<RolloutLine>(&line) {
            if parsed.line_type.as_deref() == Some("session_meta") {
                if let Some(payload) = parsed.payload {
                    return Some(SessionMeta {
                        cwd: payload.get("cwd").and_then(|v| v.as_str()).map(str::to_string),
                        model: payload.get("model").and_then(|v| v.as_str()).map(str::to_string),
                        file_mtime: mtime,
                    });
                }
            }
        }
    }
    Some(SessionMeta {
        cwd: None,
        model: None,
        file_mtime: mtime,
    })
}

fn extract_response_message(line: &RolloutLine) -> Option<(ExternalRole, String)> {
    if line.line_type.as_deref() != Some("response_item") {
        return None;
    }
    let payload = line.payload.as_ref()?;
    if payload.get("type").and_then(|v| v.as_str()) != Some("message") {
        return None;
    }
    let role_str = payload.get("role").and_then(|v| v.as_str())?;
    let role = match role_str {
        "user" => ExternalRole::User,
        "assistant" => ExternalRole::Assistant,
        // "developer" carries injected instructions/permissions text, not a
        // conversational turn from either party.
        _ => ExternalRole::System,
    };
    let content = payload.get("content")?.as_array()?;
    let text: String = content
        .iter()
        .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    Some((role, text))
}

fn read_cli_version(home: &Path) -> Option<String> {
    // The version is recorded per-session in `session_meta.cli_version`, not
    // in a single global file. The most recent rollout file is used as a
    // representative sample -- good enough for the detected-clients display,
    // and never treated as authoritative for parsing decisions beyond that.
    let sessions_dir = home.join("sessions");
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for path in walk_jsonl_files(&sessions_dir) {
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                    newest = Some((modified, path));
                }
            }
        }
    }
    let (_, path) = newest?;
    let file = open_read_only(&path).ok()?;
    use std::io::{BufRead, BufReader};
    for line in BufReader::new(file).lines().take(5).flatten() {
        if let Ok(parsed) = serde_json::from_str::<RolloutLine>(&line) {
            if parsed.line_type.as_deref() == Some("session_meta") {
                return parsed
                    .payload
                    .as_ref()
                    .and_then(|p| p.get("cli_version"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
        }
    }
    None
}

fn walk_jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_jsonl_files_inner(dir, &mut out, 0);
    out
}

/// Depth-bounded so an adversarial/corrupt directory structure (deeply
/// nested symlink loop, etc.) cannot make a scan run forever. Codex's own
/// layout is `sessions/<yyyy>/<mm>/<dd>/*.jsonl`, four levels deep.
fn walk_jsonl_files_inner(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_jsonl_files_inner(&path, out, depth + 1);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

fn session_id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    // "rollout-2026-07-02T18-42-19-<uuid>" -- the UUID is the trailing
    // 5 hyphen-separated groups (8-4-4-4-12 hex).
    let parts: Vec<&str> = stem.rsplitn(6, '-').collect();
    if parts.len() < 5 {
        return None;
    }
    let uuid = format!(
        "{}-{}-{}-{}-{}",
        parts[4], parts[3], parts[2], parts[1], parts[0]
    );
    Some(uuid)
}

fn find_rollout_file(sessions_dir: &Path, external_session_id: &str) -> Option<PathBuf> {
    walk_jsonl_files(sessions_dir)
        .into_iter()
        .find(|p| session_id_from_filename(p).as_deref() == Some(external_session_id))
}

fn count_lines(path: &Path) -> u32 {
    let Ok(file) = open_read_only(path) else {
        return 0;
    };
    use std::io::{BufRead, BufReader};
    BufReader::new(file).lines().flatten().filter(|l| !l.trim().is_empty()).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::adapters::fixtures;

    #[test]
    fn detects_a_supported_codex_home() {
        let dir = fixtures::codex::supported_fixture();
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let detected = adapter.detect().unwrap().expect("should detect");
        assert!(detected.supported, "version should be in supported range");
    }

    #[test]
    fn detects_but_flags_an_unsupported_version() {
        let dir = fixtures::codex::unsupported_version_fixture();
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let detected = adapter.detect().unwrap().expect("should still detect");
        assert!(!detected.supported);
    }

    #[test]
    fn missing_codex_home_is_not_detected() {
        let dir = tempfile::TempDir::new().unwrap();
        let adapter = CodexAdapter::at(dir.path().join("does-not-exist"));
        assert!(adapter.detect().unwrap().is_none());
    }

    #[test]
    fn missing_sessions_dir_is_a_typed_missing_path_error() {
        let dir = fixtures::codex::missing_sessions_dir_fixture();
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().expect("home exists");
        let result = adapter.list_sessions(&client);
        assert!(matches!(result, Err(AdapterError::MissingPath { .. })));
    }

    #[test]
    fn lists_sessions_from_a_supported_fixture() {
        let dir = fixtures::codex::supported_fixture();
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        assert!(!sessions.is_empty());
        assert!(sessions[0].project_path.is_some());
    }

    #[test]
    fn reads_a_session_with_user_and_assistant_messages() {
        let dir = fixtures::codex::supported_fixture();
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        let detail = adapter
            .read_session(&client, &sessions[0].external_session_id)
            .unwrap();
        assert!(detail.messages.iter().any(|m| m.role == ExternalRole::User));
        assert!(detail
            .messages
            .iter()
            .any(|m| m.role == ExternalRole::Assistant));
    }

    #[test]
    fn a_corrupt_session_file_is_a_typed_error_not_a_panic() {
        let dir = fixtures::codex::corrupt_session_fixture();
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let result = adapter.read_session(&client, "corrupt-0000-0000-0000-000000000000");
        assert!(matches!(
            result,
            Err(AdapterError::CorruptSession { .. }) | Err(AdapterError::SessionNotFound { .. })
        ));
    }

    #[test]
    fn reading_a_nonexistent_session_id_is_session_not_found() {
        let dir = fixtures::codex::supported_fixture();
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let result = adapter.read_session(&client, "00000000-0000-0000-0000-000000000000");
        assert!(matches!(result, Err(AdapterError::SessionNotFound { .. })));
    }

    #[test]
    fn one_thousand_sessions_list_without_error() {
        let dir = fixtures::codex::many_sessions_fixture(1000);
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        assert_eq!(sessions.len(), 1000);
    }

    #[test]
    fn continuation_is_open_only_not_resume() {
        let dir = fixtures::codex::supported_fixture();
        let adapter = CodexAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let cap = adapter.continuation_capability(&client, "any");
        assert_eq!(cap, ContinuationCapability::OpenOnly);
    }
}
