//! Synthetic `opencode.db`-shaped SQLite databases for adapter tests.

use rusqlite::Connection;
use tempfile::TempDir;

fn create_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE session (
            id TEXT PRIMARY KEY,
            project_id TEXT,
            directory TEXT,
            title TEXT,
            model TEXT,
            time_created INTEGER,
            time_updated INTEGER
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT,
            time_created INTEGER,
            time_updated INTEGER,
            data TEXT
        );
        CREATE TABLE part (
            id TEXT PRIMARY KEY,
            message_id TEXT,
            session_id TEXT,
            time_created INTEGER,
            time_updated INTEGER,
            data TEXT
        );",
    )
    .unwrap();
}

fn insert_session(conn: &Connection, id: &str, directory: &str, title: &str, updated: i64) {
    conn.execute(
        "INSERT INTO session (id, project_id, directory, title, model, time_created, time_updated) VALUES (?1, 'global', ?2, ?3, '{\"modelID\":\"fixture-model\"}', ?4, ?4)",
        rusqlite::params![id, directory, title, updated],
    )
    .unwrap();
}

fn insert_message(conn: &Connection, id: &str, session_id: &str, role: &str, created: i64) {
    let data = format!(r#"{{"role":"{role}","time":{{"created":{created}}}}}"#);
    conn.execute(
        "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?3, ?4)",
        rusqlite::params![id, session_id, created, data],
    )
    .unwrap();
}

fn insert_text_part(conn: &Connection, id: &str, message_id: &str, session_id: &str, text: &str, created: i64) {
    let data = format!(r#"{{"type":"text","text":"{text}"}}"#);
    conn.execute(
        "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
        rusqlite::params![id, message_id, session_id, created, data],
    )
    .unwrap();
}

fn insert_tool_part(conn: &Connection, id: &str, message_id: &str, session_id: &str, created: i64) {
    let data = r#"{"type":"tool","tool":"read_file","state":{"status":"completed"}}"#;
    conn.execute(
        "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
        rusqlite::params![id, message_id, session_id, created, data],
    )
    .unwrap();
}

pub fn supported_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let conn = Connection::open(dir.path().join("opencode.db")).unwrap();
    create_schema(&conn);

    let session_id = "ses_fixture0000000000000001";
    insert_session(&conn, session_id, "C:/code/fixture-project", "Fixture session", 1_700_000_000_000);
    insert_message(&conn, "msg_1", session_id, "user", 1_700_000_000_000);
    insert_text_part(&conn, "prt_1", "msg_1", session_id, "hello opencode", 1_700_000_000_000);
    insert_message(&conn, "msg_2", session_id, "assistant", 1_700_000_001_000);
    insert_text_part(&conn, "prt_2", "msg_2", session_id, "hello back", 1_700_000_001_000);
    insert_tool_part(&conn, "prt_3", "msg_2", session_id, 1_700_000_001_500);

    dir
}

/// A file that exists at the expected path but is not a valid SQLite
/// database at all.
pub fn corrupt_db_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("opencode.db"), b"not a sqlite database").unwrap();
    dir
}

pub fn many_sessions_fixture(count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    let conn = Connection::open(dir.path().join("opencode.db")).unwrap();
    create_schema(&conn);
    for i in 0..count {
        let id = format!("ses_fixture{i:016}");
        insert_session(&conn, &id, "C:/code/fixture-project", "Session", 1_700_000_000_000 + i as i64);
        insert_message(&conn, &format!("msg_{i}"), &id, "user", 1_700_000_000_000 + i as i64);
        insert_text_part(&conn, &format!("prt_{i}"), &format!("msg_{i}"), &id, "hi", 1_700_000_000_000 + i as i64);
    }
    dir
}
