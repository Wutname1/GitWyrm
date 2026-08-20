//! Synthetic `~/.codex`-shaped trees for adapter tests.

use std::path::Path;

use tempfile::TempDir;

fn session_meta_line(session_id: &str, cwd: &str, cli_version: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-01-01T00:00:00.000Z","type":"session_meta","payload":{{"session_id":"{session_id}","cwd":"{cwd}","cli_version":"{cli_version}","model":"gpt-5-test"}}}}"#
    )
}

fn message_line(role: &str, text: &str, ts: &str) -> String {
    format!(
        r#"{{"timestamp":"{ts}","type":"response_item","payload":{{"type":"message","role":"{role}","content":[{{"type":"input_text","text":"{text}"}}]}}}}"#
    )
}

fn unrecognized_line(ts: &str) -> String {
    format!(r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"task_started"}}}}"#)
}

fn write_rollout(dir: &Path, session_id: &str, cwd: &str, cli_version: &str, lines: &[String]) {
    let path = dir.join("sessions/2026/01/01").join(format!(
        "rollout-2026-01-01T00-00-00-{session_id}.jsonl"
    ));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut all = vec![session_meta_line(session_id, cwd, cli_version)];
    all.extend_from_slice(lines);
    std::fs::write(&path, all.join("\n")).unwrap();
}

/// A well-formed `.codex` home with a current, supported CLI version and one
/// real session with a user + assistant turn.
pub fn supported_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    write_rollout(
        dir.path(),
        "019f2536-28c1-74f1-ace8-36f84e7b56da",
        "C:/code/fixture-project",
        "0.142.5",
        &[
            message_line("user", "hello codex", "2026-01-01T00:00:01Z"),
            message_line("assistant", "hello back", "2026-01-01T00:00:02Z"),
            unrecognized_line("2026-01-01T00:00:03Z"),
        ],
    );
    std::fs::write(
        dir.path().join("session_index.jsonl"),
        r#"{"id":"019f2536-28c1-74f1-ace8-36f84e7b56da","thread_name":"Fixture session","updated_at":"2026-01-01T00:00:02Z"}"#,
    )
    .unwrap();
    dir
}

/// Same shape, but the recorded `cli_version` is outside the supported
/// range, so `detect` should still find it but mark it unsupported.
pub fn unsupported_version_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    write_rollout(
        dir.path(),
        "019f0000-0000-0000-0000-000000000001",
        "C:/code/fixture-project",
        "0.42.0",
        &[message_line("user", "old codex", "2026-01-01T00:00:01Z")],
    );
    dir
}

/// The home directory exists (so `detect` succeeds) but `sessions/` itself
/// is missing -- a plausible fresh-install or permission-stripped state.
pub fn missing_sessions_dir_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path()).unwrap();
    dir
}

/// A rollout file that is present but not valid JSONL at all.
pub fn corrupt_session_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let path = dir
        .path()
        .join("sessions/2026/01/01")
        .join("rollout-2026-01-01T00-00-00-corrupt-0000-0000-0000-000000000000.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ this is not valid json at all").unwrap();
    dir
}

/// 1,000 distinct sessions, matching Gate 6's "1,000 sessions" requirement.
pub fn many_sessions_fixture(count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..count {
        let id = format!("{i:08x}-0000-0000-0000-000000000000");
        write_rollout(
            dir.path(),
            &id,
            "C:/code/fixture-project",
            "0.142.5",
            &[message_line("user", "hi", "2026-01-01T00:00:01Z")],
        );
    }
    dir
}
