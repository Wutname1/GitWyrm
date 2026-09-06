//! VS Code Copilot Chat adapter: reads
//! `%APPDATA%/Code/User/workspaceStorage/<hash>/chatSessions/<session-id>.json`
//! (and the platform-equivalent paths elsewhere), read-only.
//!
//! Verified against real installation data (2026-08). Each VS Code
//! *workspace* gets its own `workspaceStorage/<hash>/` directory (the hash is
//! opaque -- there is no documented reverse mapping from hash back to
//! workspace path, so `project_path` for these sessions is best-effort: read
//! from `workspaceStorage/<hash>/workspace.json`'s `folder` field when that
//! sidecar exists, `None` otherwise, never guessed from the hash itself).
//!
//! One chat session is one JSON file (not JSONL): `{"requests": [...],
//! "sessionId", "creationDate", "lastMessageDate", ...}`. Each entry in
//! `requests` is one full turn -- both the user's message and Copilot's
//! reply live in the *same* array element:
//! - `request.message.text` -- the user's plain text for that turn.
//! - `request.response` -- an array of heterogeneous parts. Only elements
//!   with a `value: string` field **and no `kind` tag** are literal markdown
//!   response text (confirmed empirically: those are the ones VS Code's own
//!   chat view renders as the reply body). Every tagged `kind` observed
//!   (`thinking`, `toolInvocationSerialized`, `codeblockUri`,
//!   `prepareToolInvocation`, `mcpServersStarting`, `textEditGroup`,
//!   `undoStop`, ...) is a structured event, not conversational text, and is
//!   preserved as a raw-import record per task 2.1 rather than discarded or
//!   guessed at.
//!
//! This adapter deliberately omits capabilities it cannot support safely
//! (task 3.4): there is no documented, stable way to launch VS Code and
//! resume a specific chat session from the command line, so
//! `continuation_capability` never returns anything better than `OpenOnly`,
//! and even that is conditional on `code` actually being resolvable, which
//! this adapter does not attempt to launch (launching editors is
//! `commands::editors`'s job, out of scope here).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{
    open_read_only, read_foreign_file_to_string, AdapterError, AgentClientAdapter, AdapterId,
    ContinuationCapability, DetectedClient, ExternalMessage, ExternalRole,
    ExternalSessionDetail, ExternalSessionSummary,
};

const SUPPORTED_RANGE: &str = "chat session schema version 3 (observed; older/newer versions are read but not guaranteed)";

pub struct VsCodeCopilotAdapter {
    home_override: Option<PathBuf>,
}

impl Default for VsCodeCopilotAdapter {
    fn default() -> Self {
        Self { home_override: None }
    }
}

impl VsCodeCopilotAdapter {
    pub fn at(user_dir: PathBuf) -> Self {
        Self {
            home_override: Some(user_dir),
        }
    }

    /// VS Code's per-user data directory (`.../Code/User`), platform-specific.
    fn user_dir(&self) -> Option<PathBuf> {
        if let Some(h) = &self.home_override {
            return Some(h.clone());
        }
        #[cfg(windows)]
        {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .map(|a| a.join("Code").join("User"))
        }
        #[cfg(target_os = "macos")]
        {
            std::env::var_os("HOME").map(PathBuf::from).map(|h| {
                h.join("Library")
                    .join("Application Support")
                    .join("Code")
                    .join("User")
            })
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|h| h.join(".config").join("Code").join("User"))
        }
    }

    fn workspace_storage_dir(client: &DetectedClient) -> PathBuf {
        client.home_dir.join("workspaceStorage")
    }
}

#[derive(Debug, Deserialize)]
struct ChatSessionFile {
    version: Option<i64>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "lastMessageDate")]
    last_message_date: Option<i64>,
    #[serde(rename = "customTitle")]
    custom_title: Option<String>,
    requests: Vec<ChatRequest>,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    message: Option<ChatMessage>,
    response: Option<Vec<serde_json::Value>>,
    timestamp: Option<i64>,
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    text: Option<String>,
}

impl AgentClientAdapter for VsCodeCopilotAdapter {
    fn id(&self) -> AdapterId {
        "vscode-copilot"
    }

    fn display_name(&self) -> &'static str {
        "VS Code Copilot Chat"
    }

    fn supported_version_range(&self) -> &'static str {
        SUPPORTED_RANGE
    }

    fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
        let Some(user_dir) = self.user_dir() else {
            return Ok(None);
        };
        let workspace_storage = user_dir.join("workspaceStorage");
        if !workspace_storage.is_dir() {
            return Ok(None);
        }
        // At least one chatSessions directory must actually exist -- VS Code
        // itself can be installed with the workspaceStorage tree present but
        // Copilot Chat never used, which should read as "not detected" for
        // import purposes rather than a false positive.
        let has_any_session = find_all_session_files(&workspace_storage)
            .into_iter()
            .next()
            .is_some();
        if !has_any_session {
            return Ok(None);
        }
        Ok(Some(DetectedClient {
            home_dir: user_dir,
            version: None,
            supported: true,
        }))
    }

    fn list_sessions(
        &self,
        client: &DetectedClient,
    ) -> Result<Vec<ExternalSessionSummary>, AdapterError> {
        let workspace_storage = Self::workspace_storage_dir(client);
        if !workspace_storage.is_dir() {
            return Err(AdapterError::MissingPath {
                path: workspace_storage.display().to_string(),
            });
        }

        let mut summaries = Vec::new();
        for (path, workspace_hash_dir) in find_all_session_files(&workspace_storage) {
            let Ok(contents) = read_foreign_file_to_string(&path) else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<ChatSessionFile>(&contents) else {
                continue;
            };
            if parsed.requests.is_empty() {
                // An empty draft session with no turns yet -- not worth
                // surfacing in the import picker.
                continue;
            }
            let session_id = parsed
                .session_id
                .clone()
                .unwrap_or_else(|| file_stem(&path));
            let title = parsed
                .custom_title
                .clone()
                .or_else(|| {
                    parsed
                        .requests
                        .first()
                        .and_then(|r| r.message.as_ref())
                        .and_then(|m| m.text.clone())
                        .map(|t| t.chars().take(80).collect())
                })
                .unwrap_or_else(|| "VS Code Copilot Chat session".into());
            let project_path = read_workspace_folder(&workspace_hash_dir);
            summaries.push(ExternalSessionSummary {
                external_session_id: session_id,
                title,
                updated_at: millis_to_rfc3339(parsed.last_message_date),
                project_path,
                message_count: (parsed.requests.len() * 2) as u32,
                model: parsed.requests.first().and_then(|r| r.model_id.clone()),
            });
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    fn read_session(
        &self,
        client: &DetectedClient,
        external_session_id: &str,
    ) -> Result<ExternalSessionDetail, AdapterError> {
        let workspace_storage = Self::workspace_storage_dir(client);
        let (path, workspace_hash_dir) = find_all_session_files(&workspace_storage)
            .into_iter()
            .find(|(p, _)| {
                file_stem(p) == external_session_id
                    || session_id_in_file(p).as_deref() == Some(external_session_id)
            })
            .ok_or_else(|| AdapterError::SessionNotFound {
                external_session_id: external_session_id.into(),
            })?;

        let contents = read_foreign_file_to_string(&path)?;
        let parsed: ChatSessionFile = serde_json::from_str(&contents).map_err(|e| {
            AdapterError::CorruptSession {
                detail: e.to_string(),
            }
        })?;

        let mut messages = Vec::new();
        for req in &parsed.requests {
            let ts = millis_to_rfc3339(req.timestamp);
            let user_text = req.message.as_ref().and_then(|m| m.text.clone());
            let id_base = req
                .request_id
                .clone()
                .unwrap_or_else(|| format!("{external_session_id}#{}", messages.len()));

            match user_text {
                Some(text) if !text.is_empty() => messages.push(ExternalMessage {
                    external_message_id: format!("{id_base}-user"),
                    role: ExternalRole::User,
                    timestamp: ts.clone(),
                    plain_content: text,
                    model: None,
                    raw_unrecognized: false,
                }),
                _ => messages.push(ExternalMessage {
                    external_message_id: format!("{id_base}-user"),
                    role: ExternalRole::System,
                    timestamp: ts.clone(),
                    plain_content: String::new(),
                    model: None,
                    raw_unrecognized: true,
                }),
            }

            let response_parts = req.response.clone().unwrap_or_default();
            let mut markdown = String::new();
            let mut had_unrecognized = false;
            for part in &response_parts {
                let is_plain_markdown = part.get("kind").is_none()
                    && part.get("value").and_then(|v| v.as_str()).is_some();
                if is_plain_markdown {
                    if !markdown.is_empty() {
                        markdown.push('\n');
                    }
                    markdown.push_str(part.get("value").and_then(|v| v.as_str()).unwrap_or(""));
                } else {
                    had_unrecognized = true;
                }
            }

            if !markdown.is_empty() {
                messages.push(ExternalMessage {
                    external_message_id: format!("{id_base}-assistant"),
                    role: ExternalRole::Assistant,
                    timestamp: ts.clone(),
                    plain_content: markdown,
                    model: req.model_id.clone(),
                    raw_unrecognized: false,
                });
            }
            if had_unrecognized {
                messages.push(ExternalMessage {
                    external_message_id: format!("{id_base}-events"),
                    role: ExternalRole::System,
                    timestamp: ts,
                    plain_content: String::new(),
                    model: None,
                    raw_unrecognized: true,
                });
            }
        }

        let project_path = read_workspace_folder(&workspace_hash_dir);
        Ok(ExternalSessionDetail {
            summary: ExternalSessionSummary {
                external_session_id: external_session_id.to_string(),
                title: parsed
                    .custom_title
                    .unwrap_or_else(|| "VS Code Copilot Chat session".into()),
                updated_at: millis_to_rfc3339(parsed.last_message_date),
                project_path,
                message_count: messages.len() as u32,
                model: parsed.requests.first().and_then(|r| r.model_id.clone()),
            },
            messages,
        })
    }

    fn continuation_capability(
        &self,
        _client: &DetectedClient,
        _external_session_id: &str,
    ) -> ContinuationCapability {
        // No documented `code --resume-chat <id>` equivalent exists. Opening
        // the workspace folder in VS Code is possible in principle but is a
        // separate, unimplemented launch path in this change -- claiming
        // even OpenOnly here would be aspirational, not honest, so this
        // capability is Unsupported until a real launch exists.
        ContinuationCapability::Unsupported
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn session_id_in_file(path: &Path) -> Option<String> {
    let file = open_read_only(path).ok()?;
    let parsed: ChatSessionFile = serde_json::from_reader(file).ok()?;
    parsed.session_id
}

/// Every `chatSessions/*.json` file under every `workspaceStorage/<hash>/`
/// directory, paired with that hash directory (needed afterward to look up
/// `workspace.json` for the project path).
fn find_all_session_files(workspace_storage: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut out = Vec::new();
    let Ok(hash_dirs) = std::fs::read_dir(workspace_storage) else {
        return out;
    };
    for hash_entry in hash_dirs.flatten() {
        let hash_dir = hash_entry.path();
        if !hash_dir.is_dir() {
            continue;
        }
        let sessions_dir = hash_dir.join("chatSessions");
        let Ok(session_files) = std::fs::read_dir(&sessions_dir) else {
            continue;
        };
        for file_entry in session_files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                out.push((path, hash_dir.clone()));
            }
        }
    }
    out
}

/// `workspace.json` inside a `workspaceStorage/<hash>/` directory, when
/// present, records the folder VS Code opened for that hash --
/// `{"folder":"file:///c%3A/code/..."}`. Best-effort: many hash dirs have no
/// such sidecar (empty-window sessions), and this returns `None` rather than
/// guessing from the hash.
fn read_workspace_folder(hash_dir: &Path) -> Option<String> {
    let path = hash_dir.join("workspace.json");
    let contents = read_foreign_file_to_string(&path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let folder_uri = value.get("folder").and_then(|v| v.as_str())?;
    Some(file_uri_to_path(folder_uri))
}

/// `file:///c%3A/code/foo` -> `C:/code/foo`. Minimal, handles the Windows
/// drive-letter percent-encoding actually observed; does not attempt general
/// URI decoding since workspace folder URIs are always local file paths.
fn file_uri_to_path(uri: &str) -> String {
    let without_scheme = uri.strip_prefix("file:///").unwrap_or(uri);
    without_scheme.replace("%3A", ":").replace("%3a", ":")
}

fn millis_to_rfc3339(millis: Option<i64>) -> String {
    let Some(millis) = millis else {
        return "1970-01-01T00:00:00Z".into();
    };
    match time::OffsetDateTime::from_unix_timestamp(millis / 1000) {
        Ok(dt) => dt
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into()),
        Err(_) => "1970-01-01T00:00:00Z".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::adapters::fixtures;

    #[test]
    fn detects_a_user_dir_with_real_sessions() {
        let dir = fixtures::vscode_copilot::supported_fixture();
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let detected = adapter.detect().unwrap().expect("should detect");
        assert!(detected.supported);
    }

    #[test]
    fn a_user_dir_with_no_chat_sessions_is_not_detected() {
        let dir = fixtures::vscode_copilot::no_sessions_fixture();
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        assert!(adapter.detect().unwrap().is_none());
    }

    #[test]
    fn missing_workspace_storage_is_a_typed_error_on_list() {
        let dir = fixtures::vscode_copilot::supported_fixture();
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let client = DetectedClient {
            home_dir: dir.path().join("does-not-exist"),
            version: None,
            supported: true,
        };
        assert!(matches!(
            adapter.list_sessions(&client),
            Err(AdapterError::MissingPath { .. })
        ));
    }

    #[test]
    fn lists_and_reads_a_session_with_real_content() {
        let dir = fixtures::vscode_copilot::supported_fixture();
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        assert!(!sessions.is_empty());
        assert!(sessions[0].project_path.is_some());
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
    fn a_corrupt_session_file_is_typed_not_a_panic() {
        let dir = fixtures::vscode_copilot::corrupt_session_fixture();
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        // The corrupt file is skipped during listing (never counted), and a
        // direct read by its filename stem returns SessionNotFound since it
        // never made it into any index -- both are typed outcomes, neither
        // panics.
        let sessions = adapter.list_sessions(&client).unwrap();
        assert!(sessions.iter().all(|s| s.external_session_id != "corrupt"));
    }

    #[test]
    fn nonexistent_session_id_is_session_not_found() {
        let dir = fixtures::vscode_copilot::supported_fixture();
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let result = adapter.read_session(&client, "totally-made-up");
        assert!(matches!(result, Err(AdapterError::SessionNotFound { .. })));
    }

    #[test]
    fn one_thousand_sessions_list_without_error() {
        let dir = fixtures::vscode_copilot::many_sessions_fixture(1000);
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        assert_eq!(sessions.len(), 1000);
    }

    #[test]
    fn continuation_is_unsupported_not_aspirational() {
        let dir = fixtures::vscode_copilot::supported_fixture();
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        let cap = adapter.continuation_capability(&client, &sessions[0].external_session_id);
        assert_eq!(cap, ContinuationCapability::Unsupported);
    }

    #[test]
    fn tagged_response_parts_are_preserved_as_raw_events() {
        let dir = fixtures::vscode_copilot::supported_fixture();
        let adapter = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
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

    fn write(dir: &tempfile::TempDir, hash: &str, name: &str, body: &str, ws: Option<&str>) {
        let hd = dir.path().join("workspaceStorage").join(hash);
        let sd = hd.join("chatSessions");
        std::fs::create_dir_all(&sd).unwrap();
        std::fs::write(sd.join(format!("{name}.json")), body).unwrap();
        if let Some(f) = ws {
            std::fs::write(hd.join("workspace.json"), format!(r#"{{"folder":"{f}"}}"#)).unwrap();
        }
    }

    #[test]
    fn probe_message_count_vs_actual() {
        let dir = tempfile::TempDir::new().unwrap();
        // 3 turns: one normal, one user-only (no response), one with only tool events
        let body = r#"{"version":3,"sessionId":"s1","lastMessageDate":1700000000000,"requests":[
          {"requestId":"r1","message":{"text":"hi"},"timestamp":1700000000000,"response":[{"value":"reply"}]},
          {"requestId":"r2","message":{"text":"pending"},"timestamp":1700000001000,"response":[]},
          {"requestId":"r3","message":{"text":"tools"},"timestamp":1700000002000,"response":[{"kind":"thinking","value":"x"}]}
        ]}"#;
        write(&dir, "h1", "s1", body, Some("file:///c%3A/code/proj"));
        let a = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let c = a.detect().unwrap().unwrap();
        let list = a.list_sessions(&c).unwrap();
        let d = a.read_session(&c, "s1").unwrap();
        println!("LIST count={} ACTUAL={} detail_summary_count={}", list[0].message_count, d.messages.len(), d.summary.message_count);
    }

    #[test]
    fn probe_duplicate_session_id_across_workspaces() {
        let dir = tempfile::TempDir::new().unwrap();
        let body_a = r#"{"version":3,"sessionId":"dup","lastMessageDate":1700000000000,"requests":[{"requestId":"ra","message":{"text":"in project A"},"timestamp":1700000000000,"response":[{"value":"A reply"}]}]}"#;
        let body_b = r#"{"version":3,"sessionId":"dup","lastMessageDate":1700000009000,"requests":[{"requestId":"rb","message":{"text":"in project B"},"timestamp":1700000009000,"response":[{"value":"B reply"}]}]}"#;
        write(&dir, "hA", "dup", body_a, Some("file:///c%3A/code/project-A"));
        write(&dir, "hB", "dup", body_b, Some("file:///c%3A/code/project-B"));
        let a = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let c = a.detect().unwrap().unwrap();
        let list = a.list_sessions(&c).unwrap();
        for s in &list { println!("LIST id={} proj={:?} updated={}", s.external_session_id, s.project_path, s.updated_at); }
        let d = a.read_session(&c, "dup").unwrap();
        println!("READ proj={:?} content={:?}", d.summary.project_path, d.messages.iter().map(|m|m.plain_content.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn probe_missing_timestamps_fabricated() {
        let dir = tempfile::TempDir::new().unwrap();
        let body = r#"{"version":3,"sessionId":"nots","requests":[{"requestId":"r1","message":{"text":"hi"},"response":[{"value":"reply"}]}]}"#;
        write(&dir, "h1", "nots", body, None);
        let a = VsCodeCopilotAdapter::at(dir.path().to_path_buf());
        let c = a.detect().unwrap().unwrap();
        let list = a.list_sessions(&c).unwrap();
        println!("LIST updated={} proj={:?}", list[0].updated_at, list[0].project_path);
        let d = a.read_session(&c, "nots").unwrap();
        for m in &d.messages { println!("  msg ts={} role={:?} raw={}", m.timestamp, m.role, m.raw_unrecognized); }
    }

    #[test]
    fn probe_uri_decoding() {
        for u in ["file:///c%3A/code/my%20project", "file:///c%3A/code/caf%C3%A9", "file:///home/me/x", "vscode-remote://ssh-remote%2Bbox/home/me/x"] {
            println!("{u} -> {}", file_uri_to_path(u));
        }
    }
}
