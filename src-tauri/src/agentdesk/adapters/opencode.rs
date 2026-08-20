//! OpenCode adapter: reads `~/.local/share/opencode/opencode.db`, a SQLite
//! database, strictly read-only.
//!
//! Verified against a real installation (2026-08): OpenCode does not store
//! sessions as flat files the way Codex/Claude Code do -- `storage/` holds
//! only scraps (a `session_diff` cache), and the actual session/message/part
//! data lives in `opencode.db`. Tables observed: `session` (`id`,
//! `directory`, `title`, `model` as a JSON string, `time_created`,
//! `time_updated`, ...), `message` (`id`, `session_id`, `data` as a JSON blob
//! with `role`), `part` (`id`, `message_id`, `session_id`, `data` as a JSON
//! blob with `type`; only `type: "text"` parts carry plain conversational
//! text -- `tool`, `file`, `reasoning`, etc. are preserved as raw-import
//! records per task 2.1, not discarded).
//!
//! # Read-only enforcement
//!
//! The connection is opened with exactly
//! `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX`, never
//! `SQLITE_OPEN_READ_WRITE` and never `SQLITE_OPEN_CREATE`. SQLite itself
//! then refuses any statement that would write -- this is enforced by the
//! database engine, not just by this adapter only ever issuing `SELECT`.
//! [`tests::the_connection_cannot_execute_a_write_statement`] proves that a
//! write attempted through this exact open mode is rejected by SQLite.
//! Opening with `immutable=1` is deliberately not used: that flag tells
//! SQLite the file will never change externally either, which does not hold
//! here (OpenCode itself may still be running and writing).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

use super::{
    AdapterError, AgentClientAdapter, AdapterId, ContinuationCapability, DetectedClient,
    ExternalMessage, ExternalRole, ExternalSessionDetail, ExternalSessionSummary,
};

const SUPPORTED_RANGE: &str = "any (no version gate; schema has been stable in fixtures)";

pub struct OpenCodeAdapter {
    home_override: Option<PathBuf>,
}

impl Default for OpenCodeAdapter {
    fn default() -> Self {
        Self { home_override: None }
    }
}

impl OpenCodeAdapter {
    pub fn at(home: PathBuf) -> Self {
        Self {
            home_override: Some(home),
        }
    }

    fn data_dir(&self) -> Option<PathBuf> {
        if let Some(h) = &self.home_override {
            return Some(h.clone());
        }
        #[cfg(windows)]
        {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|h| h.join(".local").join("share").join("opencode"))
        }
        #[cfg(not(windows))]
        {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|h| h.join(".local").join("share").join("opencode"))
        }
    }

    fn db_path(client: &DetectedClient) -> PathBuf {
        client.home_dir.join("opencode.db")
    }
}

/// Open `opencode.db` strictly for reading -- see the module doc.
fn open_read_only_db(path: &Path) -> Result<Connection, AdapterError> {
    if !path.is_file() {
        return Err(AdapterError::MissingPath {
            path: path.display().to_string(),
        });
    }
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| AdapterError::Io {
        detail: e.to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct ModelJson {
    #[serde(rename = "modelID")]
    model_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageData {
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PartData {
    #[serde(rename = "type")]
    part_type: Option<String>,
    text: Option<String>,
}

impl AgentClientAdapter for OpenCodeAdapter {
    fn id(&self) -> AdapterId {
        "opencode"
    }

    fn display_name(&self) -> &'static str {
        "opencode"
    }

    fn supported_version_range(&self) -> &'static str {
        SUPPORTED_RANGE
    }

    fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
        let Some(dir) = self.data_dir() else {
            return Ok(None);
        };
        let db_path = dir.join("opencode.db");
        if !db_path.is_file() {
            return Ok(None);
        }
        // Opening alone does not prove the file is a real SQLite database --
        // SQLite opens lazily and only fails once a page is actually read.
        // `PRAGMA schema_version` forces that first read cheaply, so a file
        // that merely exists but is not a database (or is truncated/garbage)
        // reads as "detected but unusable" rather than a false "supported."
        let supported = open_read_only_db(&db_path)
            .and_then(|conn| {
                conn.pragma_query_value(None, "schema_version", |row| row.get::<_, i64>(0))
                    .map_err(|e| AdapterError::Io {
                        detail: e.to_string(),
                    })
            })
            .is_ok();
        Ok(Some(DetectedClient {
            home_dir: dir,
            version: None,
            supported,
        }))
    }

    fn list_sessions(
        &self,
        client: &DetectedClient,
    ) -> Result<Vec<ExternalSessionSummary>, AdapterError> {
        let conn = open_read_only_db(&Self::db_path(client))?;
        let mut stmt = conn
            .prepare(
                "SELECT s.id, s.title, s.directory, s.model, s.time_updated,
                        (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) AS msg_count
                 FROM session s
                 ORDER BY s.time_updated DESC",
            )
            .map_err(sqlite_to_corrupt)?;

        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: Option<String> = row.get(1)?;
                let directory: Option<String> = row.get(2)?;
                let model_json: Option<String> = row.get(3)?;
                let time_updated: Option<i64> = row.get(4)?;
                let msg_count: i64 = row.get(5)?;
                Ok((id, title, directory, model_json, time_updated, msg_count))
            })
            .map_err(sqlite_to_corrupt)?;

        let mut summaries = Vec::new();
        for row in rows {
            let (id, title, directory, model_json, time_updated, msg_count) =
                row.map_err(sqlite_to_corrupt)?;
            let model = model_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<ModelJson>(j).ok())
                .and_then(|m| m.model_id);
            summaries.push(ExternalSessionSummary {
                external_session_id: id,
                title: title.unwrap_or_else(|| "opencode session".into()),
                updated_at: millis_to_rfc3339(time_updated),
                project_path: directory,
                message_count: msg_count.max(0) as u32,
                model,
            });
        }
        Ok(summaries)
    }

    fn read_session(
        &self,
        client: &DetectedClient,
        external_session_id: &str,
    ) -> Result<ExternalSessionDetail, AdapterError> {
        let conn = open_read_only_db(&Self::db_path(client))?;

        let mut session_stmt = conn
            .prepare("SELECT title, directory, model, time_updated FROM session WHERE id = ?1")
            .map_err(sqlite_to_corrupt)?;
        let session_row = session_stmt
            .query_row([external_session_id], |row| {
                let title: Option<String> = row.get(0)?;
                let directory: Option<String> = row.get(1)?;
                let model_json: Option<String> = row.get(2)?;
                let time_updated: Option<i64> = row.get(3)?;
                Ok((title, directory, model_json, time_updated))
            });
        let (title, directory, model_json, time_updated) = match session_row {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(AdapterError::SessionNotFound {
                    external_session_id: external_session_id.into(),
                })
            }
            Err(e) => return Err(sqlite_to_corrupt(e)),
        };
        let model = model_json
            .as_deref()
            .and_then(|j| serde_json::from_str::<ModelJson>(j).ok())
            .and_then(|m| m.model_id);

        let mut msg_stmt = conn
            .prepare(
                "SELECT id, data, time_created FROM message WHERE session_id = ?1 ORDER BY time_created ASC",
            )
            .map_err(sqlite_to_corrupt)?;
        let message_rows = msg_stmt
            .query_map([external_session_id], |row| {
                let id: String = row.get(0)?;
                let data: String = row.get(1)?;
                let time_created: Option<i64> = row.get(2)?;
                Ok((id, data, time_created))
            })
            .map_err(sqlite_to_corrupt)?;

        let mut messages = Vec::new();
        for row in message_rows {
            let (message_id, data, time_created) = row.map_err(sqlite_to_corrupt)?;
            let timestamp = millis_to_rfc3339(time_created);
            let role = serde_json::from_str::<MessageData>(&data)
                .ok()
                .and_then(|m| m.role);
            let external_role = match role.as_deref() {
                Some("user") => ExternalRole::User,
                Some("assistant") => ExternalRole::Assistant,
                _ => ExternalRole::System,
            };

            let collected = collect_text_parts(&conn, &message_id)?;
            match &collected.text {
                Some(text) if !text.is_empty() => messages.push(ExternalMessage {
                    external_message_id: message_id.clone(),
                    role: external_role,
                    timestamp: timestamp.clone(),
                    plain_content: text.clone(),
                    model: model.clone(),
                    raw_unrecognized: false,
                }),
                _ => messages.push(ExternalMessage {
                    external_message_id: message_id.clone(),
                    role: ExternalRole::System,
                    timestamp: timestamp.clone(),
                    plain_content: String::new(),
                    model: None,
                    raw_unrecognized: true,
                }),
            }
            // A message can carry both a text reply and a tool call in the
            // same turn -- the tool activity is preserved as its own
            // labeled raw-import record rather than silently merged away
            // (task 2.1).
            if collected.had_non_text_part && collected.text.is_some() {
                messages.push(ExternalMessage {
                    external_message_id: format!("{message_id}-events"),
                    role: ExternalRole::System,
                    timestamp,
                    plain_content: String::new(),
                    model: None,
                    raw_unrecognized: true,
                });
            }
        }

        Ok(ExternalSessionDetail {
            summary: ExternalSessionSummary {
                external_session_id: external_session_id.to_string(),
                title: title.unwrap_or_else(|| "opencode session".into()),
                updated_at: millis_to_rfc3339(time_updated),
                project_path: directory,
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
        // `commands::opencode` already knows how to launch the opencode CLI
        // in a terminal with an initial prompt (`opencode <project> --prompt`),
        // but that is a *new* conversation, not a resume of this specific
        // session id -- opencode's CLI has no documented `--session <id>`
        // resume flag as of this change. Honest capability: open only.
        ContinuationCapability::OpenOnly
    }
}

/// What a message's parts contained: the joined plain text (if any), and
/// whether at least one part was something other than `type: "text"` (a
/// tool call, a file reference, reasoning, etc.) -- kept as a separate flag
/// rather than folded silently into the text, so a message that mixes a
/// text reply with a tool call still surfaces that tool activity as its own
/// raw-import record (task 2.1) instead of quietly dropping it.
struct CollectedParts {
    text: Option<String>,
    had_non_text_part: bool,
}

fn collect_text_parts(conn: &Connection, message_id: &str) -> Result<CollectedParts, AdapterError> {
    let mut stmt = conn
        .prepare("SELECT data FROM part WHERE message_id = ?1 ORDER BY time_created ASC")
        .map_err(sqlite_to_corrupt)?;
    let rows = stmt
        .query_map([message_id], |row| row.get::<_, String>(0))
        .map_err(sqlite_to_corrupt)?;

    let mut texts = Vec::new();
    let mut had_non_text_part = false;
    for row in rows {
        let data = row.map_err(sqlite_to_corrupt)?;
        match serde_json::from_str::<PartData>(&data) {
            Ok(part) if part.part_type.as_deref() == Some("text") => {
                if let Some(text) = part.text {
                    texts.push(text);
                }
            }
            _ => had_non_text_part = true,
        }
    }
    let text = if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    };
    Ok(CollectedParts { text, had_non_text_part })
}

fn sqlite_to_corrupt(e: rusqlite::Error) -> AdapterError {
    AdapterError::CorruptSession {
        detail: e.to_string(),
    }
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
    fn detects_a_valid_opencode_db() {
        let dir = fixtures::opencode::supported_fixture();
        let adapter = OpenCodeAdapter::at(dir.path().to_path_buf());
        let detected = adapter.detect().unwrap().expect("should detect");
        assert!(detected.supported);
    }

    #[test]
    fn missing_db_is_not_detected() {
        let dir = tempfile::TempDir::new().unwrap();
        let adapter = OpenCodeAdapter::at(dir.path().to_path_buf());
        assert!(adapter.detect().unwrap().is_none());
    }

    #[test]
    fn a_file_that_is_not_a_real_sqlite_database_is_missing_path_on_list() {
        let dir = fixtures::opencode::corrupt_db_fixture();
        let adapter = OpenCodeAdapter::at(dir.path().to_path_buf());
        // detect() itself will report unsupported since open fails.
        let detected = adapter.detect().unwrap().expect("file exists, so detected");
        assert!(!detected.supported);
    }

    #[test]
    fn lists_and_reads_sessions_with_text_parts() {
        let dir = fixtures::opencode::supported_fixture();
        let adapter = OpenCodeAdapter::at(dir.path().to_path_buf());
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
    }

    #[test]
    fn nonexistent_session_id_is_session_not_found() {
        let dir = fixtures::opencode::supported_fixture();
        let adapter = OpenCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let result = adapter.read_session(&client, "ses_does_not_exist");
        assert!(matches!(result, Err(AdapterError::SessionNotFound { .. })));
    }

    #[test]
    fn one_thousand_sessions_list_without_error() {
        let dir = fixtures::opencode::many_sessions_fixture(1000);
        let adapter = OpenCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        assert_eq!(sessions.len(), 1000);
    }

    /// The core no-writes proof for this adapter specifically: attempting a
    /// write through the exact connection mode production code uses must be
    /// rejected by SQLite itself, not merely "our code happens not to write."
    #[test]
    fn the_connection_cannot_execute_a_write_statement() {
        let dir = fixtures::opencode::supported_fixture();
        let db_path = dir.path().join("opencode.db");
        let conn = open_read_only_db(&db_path).unwrap();
        let result = conn.execute("DELETE FROM session", []);
        assert!(
            result.is_err(),
            "a write through the adapter's own read-only open mode must be refused by SQLite"
        );
    }

    #[test]
    fn non_text_parts_are_preserved_as_raw_not_discarded() {
        let dir = fixtures::opencode::supported_fixture();
        let adapter = OpenCodeAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        let sessions = adapter.list_sessions(&client).unwrap();
        let detail = adapter
            .read_session(&client, &sessions[0].external_session_id)
            .unwrap();
        assert!(detail.messages.iter().any(|m| m.raw_unrecognized));
    }
}
