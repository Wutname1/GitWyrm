//! Synthetic `Code/User`-shaped trees for adapter tests.

use tempfile::TempDir;

fn write_session_file(dir: &TempDir, hash: &str, session_id: &str, contents: &str, with_workspace: bool) {
    let hash_dir = dir.path().join("workspaceStorage").join(hash);
    let sessions_dir = hash_dir.join("chatSessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::write(sessions_dir.join(format!("{session_id}.json")), contents).unwrap();
    if with_workspace {
        std::fs::write(
            hash_dir.join("workspace.json"),
            r#"{"folder":"file:///c%3A/code/fixture-project"}"#,
        )
        .unwrap();
    }
}

fn session_json(session_id: &str, custom_title: &str, last_message_date: i64) -> String {
    format!(
        r#"{{
  "version": 3,
  "requesterUsername": "fixture-user",
  "responderUsername": "GitHub Copilot",
  "initialLocation": "panel",
  "sessionId": "{session_id}",
  "creationDate": {last_message_date},
  "lastMessageDate": {last_message_date},
  "customTitle": "{custom_title}",
  "requests": [
    {{
      "requestId": "request_{session_id}",
      "message": {{ "text": "hello copilot" }},
      "timestamp": {last_message_date},
      "modelId": "fixture-model",
      "response": [
        {{ "kind": "mcpServersStarting", "didStartServerIds": [] }},
        {{ "kind": "thinking", "value": "internal reasoning, not conversational" }},
        {{ "value": "hello back", "supportThemeIcons": true, "supportHtml": false }}
      ]
    }}
  ]
}}"#
    )
}

pub fn supported_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    write_session_file(
        &dir,
        "hash0001",
        "session-fixture-0001",
        &session_json("session-fixture-0001", "Fixture chat", 1_700_000_000_000),
        true,
    );
    dir
}

/// `workspaceStorage` exists but no `chatSessions` directory anywhere under
/// it -- VS Code installed, Copilot Chat never used.
pub fn no_sessions_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("workspaceStorage").join("hash0001")).unwrap();
    dir
}

/// The same session id under two workspace folders.
///
/// VS Code keeps one folder per workspace and does not promise ids are
/// unique across them; a restored backup, a synced profile or a cloned
/// machine reproduces one.
pub fn duplicate_id_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    write_session_file(
        &dir,
        "hash0001",
        "dup",
        &session_json("dup", "In the first project", 1_700_000_000_000),
        true,
    );
    write_session_file(
        &dir,
        "hash0002",
        "dup",
        &session_json("dup", "In the second project", 1_700_000_009_000),
        true,
    );
    dir
}

pub fn corrupt_session_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    write_session_file(&dir, "hash0001", "corrupt", "{ not valid json", false);
    dir
}

pub fn many_sessions_fixture(count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..count {
        let id = format!("session-fixture-{i:06}");
        write_session_file(
            &dir,
            &format!("hash{i:06}"),
            &id,
            &session_json(&id, "Session", 1_700_000_000_000 + i as i64),
            true,
        );
    }
    dir
}
