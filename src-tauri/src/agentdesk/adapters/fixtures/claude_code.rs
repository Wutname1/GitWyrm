//! Synthetic `~/.claude`-shaped trees for adapter tests.

use std::path::Path;

use tempfile::TempDir;

fn user_line(session_id: &str, cwd: &str, version: &str, text: &str, ts: &str) -> String {
    format!(
        r#"{{"type":"user","message":{{"role":"user","content":"{text}"}},"timestamp":"{ts}","cwd":"{cwd}","sessionId":"{session_id}","version":"{version}","uuid":"u-{ts}"}}"#
    )
}

fn assistant_line(session_id: &str, text: &str, ts: &str) -> String {
    format!(
        r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{text}"}}]}},"timestamp":"{ts}","sessionId":"{session_id}","uuid":"a-{ts}"}}"#
    )
}

fn queue_op_line(session_id: &str, ts: &str) -> String {
    format!(r#"{{"type":"queue-operation","operation":"enqueue","timestamp":"{ts}","sessionId":"{session_id}"}}"#)
}

fn write_session(root: &Path, project_dir_name: &str, session_id: &str, lines: &[String]) {
    let path = root
        .join("projects")
        .join(project_dir_name)
        .join(format!("{session_id}.jsonl"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, lines.join("\n")).unwrap();
}

pub fn supported_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let session_id = "029d0239-5af9-4ded-a09b-065696d454ca";
    write_session(
        dir.path(),
        "C--code-fixture-project",
        session_id,
        &[
            queue_op_line(session_id, "2026-01-01T00:00:00Z"),
            user_line(
                session_id,
                "C:/code/fixture-project",
                "2.1.215",
                "hello claude",
                "2026-01-01T00:00:01Z",
            ),
            assistant_line(session_id, "hello back", "2026-01-01T00:00:02Z"),
        ],
    );
    dir
}

/// A conversation that started in one project and moved to another, with a
/// later record carrying an older timestamp.
///
/// Both are ordinary: `cwd` is written per line and a session legitimately
/// moves between directories, and session management appends its own lines
/// so records are not guaranteed to be in time order.
pub fn moved_directory_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let session_id = "moved-1";
    write_session(
        dir.path(),
        "C--code-started-here",
        session_id,
        &[
            user_line(
                session_id,
                "C:/code/started-here",
                "2.1.215",
                "where it began",
                "2026-01-05T00:00:00Z",
            ),
            assistant_line(session_id, "working", "2026-01-05T00:00:01Z"),
            // A `cd` into a sibling repo, then a line appended out of order.
            user_line(
                session_id,
                "C:/code/ended-up-here",
                "2.1.215",
                "and where it ended",
                "2026-01-02T00:00:00Z",
            ),
        ],
    );
    dir
}

pub fn unsupported_version_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let session_id = "00000000-0000-0000-0000-000000000099";
    write_session(
        dir.path(),
        "C--code-fixture-project",
        session_id,
        &[user_line(
            session_id,
            "C:/code/fixture-project",
            "0.9.0",
            "old client",
            "2026-01-01T00:00:01Z",
        )],
    );
    dir
}

pub fn missing_projects_dir_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path()).unwrap();
    dir
}

pub fn corrupt_session_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let path = dir
        .path()
        .join("projects")
        .join("C--code-fixture-project")
        .join("corrupt-session.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not json{{{").unwrap();
    dir
}

pub fn many_sessions_fixture(count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..count {
        let id = format!("{i:08x}-0000-0000-0000-000000000000");
        write_session(
            dir.path(),
            "C--code-fixture-project",
            &id,
            &[user_line(
                &id,
                "C:/code/fixture-project",
                "2.1.215",
                "hi",
                "2026-01-01T00:00:01Z",
            )],
        );
    }
    dir
}
