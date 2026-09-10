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

        super::sort_newest_first(&mut summaries);
        Ok(summaries)
    }

    fn read_session(
        &self,
        client: &DetectedClient,
        external_session_id: &str,
    ) -> Result<ExternalSessionDetail, AdapterError> {
        let projects_dir = Self::projects_dir(client);
        let matches = find_session_files(&projects_dir, external_session_id);
        if matches.len() > 1 {
            return Err(AdapterError::AmbiguousSession {
                external_session_id: external_session_id.into(),
                matches: matches.len() as u32,
            });
        }
        let path = matches.into_iter().next().ok_or_else(|| {
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
            // First one wins. `cwd` is recorded per line, and a conversation
            // legitimately moves between directories -- into a submodule, a
            // worktree, a sibling repo -- so last-wins attributed the whole
            // conversation to wherever it happened to end. `reconcile`
            // matches this path exactly against the open repositories,
            // deliberately refusing to guess, so a confidently wrong value
            // here defeats that: it resolves to the wrong project rather
            // than honestly failing to resolve. Where the conversation
            // started is the one directory that describes it.
            cwd = cwd.or_else(|| parsed.cwd.clone());
            version = version.or_else(|| parsed.version.clone());

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
                // Same first-wins rule as `read_session`, for the same
                // reason: the listing row and the transcript must name the
                // same project, and it must be the one the work started in.
                cwd = cwd.or_else(|| parsed.cwd.clone());
                // Newest, not last. Records are not guaranteed to be in time
                // order -- session management appends its own lines, and a
                // resumed conversation interleaves -- so taking the last one
                // could show a "last updated" older than messages plainly
                // visible in the conversation, and sort it wrongly in a list
                // ordered newest first.
                updated_at = match (updated_at, parsed.timestamp.clone()) {
                    (Some(current), Some(seen)) if seen > current => Some(seen),
                    (Some(current), _) => Some(current),
                    (None, seen) => seen,
                };
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

/// Every project folder holding a transcript with this id.
///
/// More than one is possible: Claude Code keeps one folder per project and
/// the id is only as unique as whatever wrote it. Restoring a backup or
/// copying a project folder produces two.
///
/// Collecting rather than returning the first is the same choice
/// `vscode_copilot` makes, and for the same reason -- taking the first
/// returned a real transcript that was simply not the one asked for, which
/// nothing downstream could detect because the answer looked entirely
/// valid. Claude Code's ids are UUIDs so a collision is far less likely
/// than VS Code's, but "less likely" is not a reason for the two adapters
/// to behave differently when it happens.
fn find_session_files(projects_dir: &Path, external_session_id: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(projects_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|project_entry| {
            project_entry
                .path()
                .join(format!("{external_session_id}.jsonl"))
        })
        .filter(|candidate| candidate.is_file())
        .collect()
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

    /// A conversation is attributed to where it STARTED, not where it ended.
    ///
    /// `cwd` is recorded per line, so a session that moves into a submodule
    /// or a sibling repo used to be attributed to whichever directory its
    /// last record happened to name. `reconcile` matches that path exactly
    /// against the open projects and deliberately refuses to guess -- so a
    /// confidently wrong path does not fail to resolve, it resolves to the
    /// WRONG project, which is the outcome reconcile says it exists to avoid.
    #[test]
    fn a_conversation_that_moved_directories_keeps_the_one_it_started_in() {
        let dir = fixtures::claude_code::moved_directory_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().expect("fixture is supported");

        let listed = adapter.list_sessions(&client).unwrap();
        let row = listed.iter().find(|s| s.external_session_id == "moved-1").expect("listed");
        assert_eq!(row.project_path.as_deref(), Some("C:/code/started-here"));

        // The listing row and the transcript must name the same project.
        let detail = adapter.read_session(&client, "moved-1").unwrap();
        assert_eq!(detail.summary.project_path.as_deref(), Some("C:/code/started-here"));
    }

    /// "Last updated" is the newest timestamp in the file, not the last one
    /// written. Records are not guaranteed to be in time order, so taking
    /// the last could show a date older than messages plainly visible in the
    /// conversation, and sort it wrongly in a newest-first list.
    #[test]
    fn the_updated_time_is_the_newest_not_the_last_written() {
        let dir = fixtures::claude_code::moved_directory_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().expect("fixture is supported");

        let listed = adapter.list_sessions(&client).unwrap();
        let row = listed.iter().find(|s| s.external_session_id == "moved-1").expect("listed");
        assert_eq!(row.updated_at, "2026-01-05T00:00:01Z");
    }

    /// Two transcripts sharing an id must not resolve to one of them.
    ///
    /// Claude Code's ids are UUIDs, so this is far less likely than the same
    /// case in VS Code -- but "less likely" is not a reason for the two
    /// adapters to behave differently when it happens, and taking the first
    /// match returns a real transcript that is simply not the one asked for.
    #[test]
    fn two_transcripts_sharing_an_id_are_refused_rather_than_guessed() {
        let dir = fixtures::claude_code::duplicate_id_fixture();
        let adapter = ClaudeCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().expect("fixture is supported");

        match adapter.read_session(&client, "dup") {
            Err(AdapterError::AmbiguousSession { external_session_id, matches }) => {
                assert_eq!(external_session_id, "dup");
                assert_eq!(matches, 2);
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
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
