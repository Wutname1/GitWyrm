//! Claude Code adapter: reads `~/.claude/projects/<sanitized-path>/<session-id>.jsonl`,
//! read-only.
//!
//! Verified against a real installation's on-disk layout (2026-08). Each
//! project gets one directory (its path with path separators/dots turned
//! into `-`); each session is one JSON-Lines file named `<uuid>.jsonl`
//! directly inside it. The project's real path is not reverse-engineered
//! from that sanitized directory name -- every record in the file itself
//! carries a `cwd` field, which is what [`ClaudeCodeAdapter`] reads instead.
//!
//! Record shapes actually observed, one JSON object per line:
//! - `{"type":"user","message":{"role":"user","content":...},"timestamp":...,"cwd":...,"sessionId":...,"version":...}`
//! - `{"type":"assistant","message":{"role":"assistant","content":[...]},"timestamp":...}`
//!   where `content` is a list of blocks (`{"type":"text","text":...}`,
//!   `{"type":"tool_use",...}`, `{"type":"tool_result",...}`, etc.) -- only
//!   `text` blocks become plain conversational content; everything else is
//!   preserved as a raw-import record (task 2.1), not discarded.
//! - `{"type":"queue-operation",...}`, `{"type":"summary",...}`, and other
//!   session-management lines -- also preserved as raw-import records.
//!
//! `content` on a user message can itself be a bare string (typed input) or
//! an array of blocks (paste/attachment turns) -- both are handled.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{
    open_read_only, read_foreign_file_to_string, AdapterError, AgentClientAdapter, AdapterId,
    ContinuationCapability, DetectedClient, ExternalMessage, ExternalRole,
    ExternalSessionDetail, ExternalSessionSummary,
};

const SUPPORTED_RANGE: &str = ">=1.0.0, <3.0.0";

pub struct ClaudeCodeAdapter {
    home_override: Option<PathBuf>,
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self { home_override: None }
    }
}

impl ClaudeCodeAdapter {
    pub fn at(home: PathBuf) -> Self {
        Self {
            home_override: Some(home),
        }
    }

    fn claude_home(&self) -> Option<PathBuf> {
        if let Some(h) = &self.home_override {
            return Some(h.clone());
        }
        home_dir().map(|h| h.join(".claude"))
    }

    fn projects_dir(client: &DetectedClient) -> PathBuf {
        client.home_dir.join("projects")
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
    let Some(major) = version.split('.').next().and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    (1..3).contains(&major)
}

#[derive(Debug, Deserialize)]
struct RawLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    timestamp: Option<String>,
    cwd: Option<String>,
    version: Option<String>,
    message: Option<serde_json::Value>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    uuid: Option<String>,
}

impl AgentClientAdapter for ClaudeCodeAdapter {
    fn id(&self) -> AdapterId {
        "claude-code"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn supported_version_range(&self) -> &'static str {
        SUPPORTED_RANGE
    }

    fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
        let Some(home) = self.claude_home() else {
            return Ok(None);
        };
        if !home.is_dir() {
            return Ok(None);
        }
        let version = read_representative_version(&home);
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
        let projects_dir = Self::projects_dir(client);
        if !projects_dir.is_dir() {
            return Err(AdapterError::MissingPath {
                path: projects_dir.display().to_string(),
            });
        }

        let mut summaries = Vec::new();
        let Ok(project_entries) = std::fs::read_dir(&projects_dir) else {
            return Err(AdapterError::MissingPath {
                path: projects_dir.display().to_string(),
            });
        };
        for project_entry in project_entries.flatten() {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }
            let Ok(session_files) = std::fs::read_dir(&project_path) else {
                continue;
            };
            for file_entry in session_files.flatten() {
                let path = file_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let summary = summarize_session(&path, session_id);
                summaries.push(summary);
            }
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    fn read_session(
        &self,
        client: &DetectedClient,
        external_session_id: &str,
    ) -> Result<ExternalSessionDetail, AdapterError> {
        let projects_dir = Self::projects_dir(client);
        let path = find_session_file(&projects_dir, external_session_id).ok_or_else(|| {
            AdapterError::SessionNotFound {
                external_session_id: external_session_id.into(),
            }
        })?;

        let contents = read_foreign_file_to_string(&path)?;
        let mut messages = Vec::new();
        let mut cwd = None;
        let mut version = None;
        let mut last_timestamp = None;
        let mut seen_any_line = false;
        let mut parsed_any_line = false;

        for (i, line) in contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            seen_any_line = true;
            let parsed: Result<RawLine, _> = serde_json::from_str(line);
            let Ok(parsed) = parsed else {
                messages.push(raw_message(external_session_id, i, &last_timestamp));
                continue;
            };
            parsed_any_line = true;
            last_timestamp = parsed.timestamp.clone().or(last_timestamp.clone());
            cwd = parsed.cwd.clone().or(cwd);
            version = parsed.version.clone().or(version);

            let id = parsed
                .uuid
                .clone()
                .unwrap_or_else(|| format!("{external_session_id}#{i}"));

            match parsed.line_type.as_deref() {
                Some("user") | Some("assistant") => {
                    let role = if parsed.line_type.as_deref() == Some("user") {
                        ExternalRole::User
                    } else {
                        ExternalRole::Assistant
                    };
                    match extract_text(parsed.message.as_ref()) {
                        Some(text) => messages.push(ExternalMessage {
                            external_message_id: id,
                            role,
                            timestamp: parsed
                                .timestamp
                                .unwrap_or_else(|| "1970-01-01T00:00:00Z".into()),
                            plain_content: text,
                            model: None,
                            raw_unrecognized: false,
                        }),
                        // A user/assistant line whose content is entirely
                        // non-text blocks (a bare tool_use/tool_result turn)
                        // is preserved, not dropped.
                        None => messages.push(ExternalMessage {
                            external_message_id: id,
                            role: ExternalRole::System,
                            timestamp: parsed
                                .timestamp
                                .unwrap_or_else(|| "1970-01-01T00:00:00Z".into()),
                            plain_content: String::new(),
                            model: None,
                            raw_unrecognized: true,
                        }),
                    }
                }
                _ => messages.push(ExternalMessage {
                    external_message_id: id,
                    role: ExternalRole::System,
                    timestamp: parsed.timestamp.unwrap_or_else(|| "1970-01-01T00:00:00Z".into()),
                    plain_content: String::new(),
                    model: None,
                    raw_unrecognized: true,
                }),
            }
            let _ = parsed.session_id;
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
                title: "Claude Code session".into(),
                updated_at,
                project_path: cwd,
                message_count: messages.len() as u32,
                model: version,
            },
            messages,
        })
    }

    fn continuation_capability(
        &self,
        _client: &DetectedClient,
        _external_session_id: &str,
    ) -> ContinuationCapability {
        // `claude --resume <session-id>` is a documented CLI entry point, but
        // launching it (like Codex's `resume`) is not wired up in this
        // change -- only reading history is. Honest until it is: open-only.
        ContinuationCapability::OpenOnly
    }
}

fn raw_message(session_id: &str, index: usize, last_timestamp: &Option<String>) -> ExternalMessage {
    ExternalMessage {
        external_message_id: format!("{session_id}#{index}"),
        role: ExternalRole::System,
        timestamp: last_timestamp.clone().unwrap_or_else(|| "1970-01-01T00:00:00Z".into()),
        plain_content: String::new(),
        model: None,
        raw_unrecognized: true,
    }
}

/// `message.content` is either a bare string or a list of typed blocks; only
/// `{"type":"text","text":...}` blocks contribute plain conversational text.
fn extract_text(message: Option<&serde_json::Value>) -> Option<String> {
    let content = message?.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    let blocks = content.as_array()?;
    let texts: Vec<&str> = blocks
        .iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect();
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn summarize_session(path: &Path, session_id: &str) -> ExternalSessionSummary {
    let mut cwd = None;
    let mut updated_at = None;
    let mut message_count = 0u32;
    let mut title = None;

    if let Ok(file) = open_read_only(path) {
        use std::io::{BufRead, BufReader};
        for line in BufReader::new(file).lines().flatten() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<RawLine>(&line) {
                cwd = parsed.cwd.clone().or(cwd);
                updated_at = parsed.timestamp.clone().or(updated_at.clone());
                if matches!(parsed.line_type.as_deref(), Some("user") | Some("assistant")) {
                    message_count += 1;
                    if title.is_none() && parsed.line_type.as_deref() == Some("user") {
                        title = extract_text(parsed.message.as_ref())
                            .map(|t| t.chars().take(80).collect::<String>());
                    }
                }
            }
        }
    }

    ExternalSessionSummary {
        external_session_id: session_id.to_string(),
        title: title.unwrap_or_else(|| "Claude Code session".into()),
        updated_at: updated_at.unwrap_or_else(|| "1970-01-01T00:00:00Z".into()),
        project_path: cwd,
        message_count,
        model: None,
    }
}

fn read_representative_version(home: &Path) -> Option<String> {
    let projects_dir = home.join("projects");
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    let entries = std::fs::read_dir(&projects_dir).ok()?;
    for project_entry in entries.flatten() {
        let Ok(session_files) = std::fs::read_dir(project_entry.path()) else {
            continue;
        };
        for file_entry in session_files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Ok(meta) = std::fs::metadata(&path) {
                if let Ok(modified) = meta.modified() {
                    if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                        newest = Some((modified, path));
                    }
                }
            }
        }
    }
    let (_, path) = newest?;
    let file = open_read_only(&path).ok()?;
    use std::io::{BufRead, BufReader};
    for line in BufReader::new(file).lines().take(20).flatten() {
        if let Ok(parsed) = serde_json::from_str::<RawLine>(&line) {
            if parsed.version.is_some() {
                return parsed.version;
            }
        }
    }
    None
}

fn find_session_file(projects_dir: &Path, external_session_id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(projects_dir).ok()?;
    for project_entry in entries.flatten() {
        let candidate = project_entry
            .path()
            .join(format!("{external_session_id}.jsonl"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::adapters::fixtures;

    #[test]
    fn detects_a_supported_claude_home() {
        let dir = fixtures::claude_code::supported_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let detected = adapter.detect().unwrap().expect("should detect");
        assert!(detected.supported);
    }

    #[test]
    fn detects_but_flags_an_unsupported_version() {
        let dir = fixtures::claude_code::unsupported_version_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let detected = adapter.detect().unwrap().expect("should still detect");
        assert!(!detected.supported);
    }

    #[test]
    fn missing_home_is_not_detected() {
        let dir = tempfile::TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::at(dir.path().join("nope"));
        assert!(adapter.detect().unwrap().is_none());
    }

    #[test]
    fn missing_projects_dir_is_a_typed_error() {
        let dir = fixtures::claude_code::missing_projects_dir_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        assert!(matches!(
            adapter.list_sessions(&client),
            Err(AdapterError::MissingPath { .. })
        ));
    }

    #[test]
    fn lists_and_reads_a_session_with_real_content() {
        let dir = fixtures::claude_code::supported_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        assert!(!sessions.is_empty());
        let detail = adapter
            .read_session(&client, &sessions[0].external_session_id)
            .unwrap();
        assert!(detail.messages.iter().any(|m| m.role == ExternalRole::User));
        assert!(detail
            .messages
            .iter()
            .any(|m| m.role == ExternalRole::Assistant));
        assert!(detail.summary.project_path.is_some());
    }

    #[test]
    fn a_corrupt_session_file_is_typed_not_a_panic() {
        let dir = fixtures::claude_code::corrupt_session_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let result = adapter.read_session(&client, "corrupt-session");
        assert!(matches!(
            result,
            Err(AdapterError::CorruptSession { .. }) | Err(AdapterError::SessionNotFound { .. })
        ));
    }

    #[test]
    fn nonexistent_session_id_is_session_not_found() {
        let dir = fixtures::claude_code::supported_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let result = adapter.read_session(&client, "totally-made-up-id");
        assert!(matches!(result, Err(AdapterError::SessionNotFound { .. })));
    }

    #[test]
    fn one_thousand_sessions_list_without_error() {
        let dir = fixtures::claude_code::many_sessions_fixture(1000);
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        assert_eq!(sessions.len(), 1000);
    }

    #[test]
    fn unrecognized_line_types_are_preserved_not_dropped() {
        let dir = fixtures::claude_code::supported_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        let detail = adapter
            .read_session(&client, &sessions[0].external_session_id)
            .unwrap();
        assert!(detail.messages.iter().any(|m| m.raw_unrecognized));
    }
}

#[cfg(test)]
mod audit_probe {
    use super::*;
    use std::io::Write;

    #[test]
    fn probe_mixed_encoding_list_vs_read() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("projects").join("C--x");
        std::fs::create_dir_all(&p).unwrap();
        let f = p.join("s1.jsonl");
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(br#"{"type":"user","message":{"role":"user","content":"hello"},"timestamp":"2026-01-01T00:00:01Z","cwd":"C:/code/real","version":"2.1.0","uuid":"u1"}"#);
        out.push(b'\n');
        // a latin-1 encoded byte in an otherwise fine line
        out.extend_from_slice(br#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"caf"#);
        out.push(0xE9);
        out.extend_from_slice(br#""}]},"timestamp":"2026-01-01T00:00:02Z","uuid":"a1"}"#);
        out.push(b'\n');
        out.extend_from_slice(br#"{"type":"user","message":{"role":"user","content":"bye"},"timestamp":"2026-01-01T00:00:03Z","uuid":"u2"}"#);
        out.push(b'\n');
        std::fs::File::create(&f).unwrap().write_all(&out).unwrap();

        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let list = adapter.list_sessions(&client).unwrap();
        println!("LIST -> {} sessions: {:?}", list.len(), list.iter().map(|s|(&s.external_session_id,&s.title,&s.message_count)).collect::<Vec<_>>());
        let r = adapter.read_session(&client, "s1");
        println!("READ -> {:?}", r.map(|d| d.messages.len()));
    }

    #[test]
    fn probe_cwd_last_wins_and_updated_at_ordering() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("projects").join("C--x");
        std::fs::create_dir_all(&p).unwrap();
        // Realistic: a resumed session where the user cd'd; and out-of-order timestamps
        let lines = [
            r#"{"type":"user","message":{"role":"user","content":"a"},"timestamp":"2026-01-05T00:00:00Z","cwd":"C:/code/project-A","version":"2.1.0","uuid":"u1"}"#,
            r#"{"type":"user","message":{"role":"user","content":"b"},"timestamp":"2026-01-01T00:00:00Z","cwd":"C:/code/project-B","uuid":"u2"}"#,
        ].join("\n");
        std::fs::write(p.join("s2.jsonl"), lines).unwrap();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let list = adapter.list_sessions(&client).unwrap();
        println!("SUMMARY cwd={:?} updated={:?}", list[0].project_path, list[0].updated_at);
        let d = adapter.read_session(&client, "s2").unwrap();
        println!("DETAIL cwd={:?} updated={:?}", d.summary.project_path, d.summary.updated_at);
    }

    #[test]
    fn probe_truncated_last_line_live_append() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("projects").join("C--x");
        std::fs::create_dir_all(&p).unwrap();
        let lines = format!("{}\n{}",
            r#"{"type":"user","message":{"role":"user","content":"hello"},"timestamp":"2026-01-01T00:00:01Z","cwd":"C:/code/real","version":"2.1.0","uuid":"u1"}"#,
            r#"{"type":"assistant","message":{"role":"assis"#);
        std::fs::write(p.join("s3.jsonl"), lines).unwrap();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let d = adapter.read_session(&client, "s3").unwrap();
        for m in &d.messages { println!("msg role={:?} raw={} ts={} id={}", m.role, m.raw_unrecognized, m.timestamp, m.external_message_id); }
    }
}
