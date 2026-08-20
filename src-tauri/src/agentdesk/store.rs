//! Session persistence: atomic file layout under `<app-data>/agent-desk/v1`.
//!
//! ```text
//! agent-desk/
//!   v1/
//!     index.json
//!     sessions/
//!       <session-id>.json
//! ```
//!
//! Every write goes through a sibling temp file, an explicit flush, and an
//! atomic rename so a crash mid-write can never leave a half-written file in
//! place of a good one (design.md: "Atomic rename failure: leave the previous
//! file untouched and return a typed write result"). Reads never delete or
//! move a file they cannot parse -- a bad session is reported and skipped,
//! never silently discarded (spec: "One damaged session").

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tempfile::NamedTempFile;

use super::model::{
    migrate_header, migrate_session, AgentSession, AgentSessionHeader, SessionId,
    SessionLoadError,
};

/// `<app-data>/agent-desk/v1`, resolved through `settings::app_data_dir` so it
/// shares the same identifier resolution (and the same test seam: pass any
/// `PathBuf` in tests without a Tauri handle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStoreRoot(PathBuf);

impl SessionStoreRoot {
    /// Resolve the store root under the real app-data directory, creating
    /// every directory in the layout above if it does not exist yet.
    pub fn resolve(app: &tauri::AppHandle) -> Result<Self, StoreInitError> {
        let app_data = crate::settings::app_data_dir(app)
            .map_err(|e| StoreInitError::AppDataUnavailable { detail: e.to_string() })?;
        Self::at(app_data.join("agent-desk").join("v1"))
    }

    /// Build a store root at an arbitrary path, creating its directories.
    /// The seam integration tests and the 1,000-session test use directly,
    /// pointed at a `tempfile::TempDir`.
    pub fn at(root: PathBuf) -> Result<Self, StoreInitError> {
        fs::create_dir_all(&root).map_err(|e| StoreInitError::Io {
            path: root.clone(),
            detail: e.to_string(),
        })?;
        fs::create_dir_all(root.join(SESSIONS_DIR)).map_err(|e| StoreInitError::Io {
            path: root.join(SESSIONS_DIR),
            detail: e.to_string(),
        })?;
        Ok(Self(root))
    }

    fn root(&self) -> &Path {
        &self.0
    }

    /// The store root's own path, for sibling stores that live alongside
    /// `sessions/`/`index.json` under the same app-data directory without
    /// duplicating `SessionStoreRoot::resolve`'s app-data lookup. Used by
    /// `agentdesk::result`'s per-session result sidecar files (task 1.2):
    /// results are read/written far more often than the session transcript
    /// itself as review/keep/undo/commit actions run, so keeping them in
    /// their own small files avoids rewriting the whole transcript file (and
    /// racing `SessionLocks`-guarded transcript mutations) on every result
    /// update.
    pub fn root_path(&self) -> &Path {
        &self.0
    }

    fn sessions_dir(&self) -> PathBuf {
        self.0.join(SESSIONS_DIR)
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{session_id}.json"))
    }

    fn index_path(&self) -> PathBuf {
        self.0.join(INDEX_FILE)
    }
}

const SESSIONS_DIR: &str = "sessions";
const INDEX_FILE: &str = "index.json";

/// Why the store root itself could not be prepared. Distinct from
/// [`SessionLoadError`] (a single file) and [`WriteError`] (a single write):
/// this is "the directory layout could not even be created."
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreInitError {
    #[error("app data directory unavailable: {detail}")]
    AppDataUnavailable { detail: String },
    #[error("could not create {path}: {detail}", path = path.display())]
    Io { path: PathBuf, detail: String },
}

/// Why an atomic write did not land. In every case the previous file on disk
/// (if any) is untouched -- the temp file either never became visible or the
/// rename itself failed, and nothing here ever truncates the destination
/// in place.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("could not serialize before writing: {detail}")]
    Serialize { detail: String },
    #[error("could not create temp file in {dir}: {detail}", dir = dir.display())]
    CreateTemp { dir: PathBuf, detail: String },
    #[error("could not write temp file: {detail}")]
    WriteTemp { detail: String },
    #[error("could not flush temp file: {detail}")]
    Flush { detail: String },
    #[error("could not rename temp file into place at {path}: {detail}", path = path.display())]
    Rename { path: PathBuf, detail: String },
}

/// Write `value` to `path` through a sibling temp file, an explicit flush,
/// and an atomic rename. `path`'s parent directory must already exist.
///
/// The temp file is created in the same directory as `path` (not the OS temp
/// directory) so the final rename is same-filesystem and therefore atomic on
/// every platform GitWyrm ships on.
pub(crate) fn write_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), WriteError> {
    let json = serde_json::to_vec_pretty(value).map_err(|e| WriteError::Serialize {
        detail: e.to_string(),
    })?;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(dir).map_err(|e| WriteError::CreateTemp {
        dir: dir.to_path_buf(),
        detail: e.to_string(),
    })?;

    io::Write::write_all(&mut temp, &json).map_err(|e| WriteError::WriteTemp {
        detail: e.to_string(),
    })?;
    temp.as_file_mut().sync_all().map_err(|e| WriteError::Flush {
        detail: e.to_string(),
    })?;

    temp.persist(path)
        .map_err(|e| WriteError::Rename {
            path: path.to_path_buf(),
            detail: e.error.to_string(),
        })?;
    Ok(())
}

/// Write one session's file atomically.
pub fn write_session(root: &SessionStoreRoot, session: &AgentSession) -> Result<(), WriteError> {
    let path = root.session_path(&session.header.session_id);
    write_atomic(&path, session)
}

/// Read and migrate one session file by ID. Never deletes or moves the file,
/// even when it fails to parse.
pub fn read_session(
    root: &SessionStoreRoot,
    session_id: &str,
) -> Result<AgentSession, SessionLoadError> {
    let path = root.session_path(session_id);
    let raw = read_json_file(&path)?;
    migrate_session(raw)
}

/// Write `index.json` from headers, already in the order the caller wants
/// stored (callers should pass [`sort_headers`]'s output).
pub fn write_index(
    root: &SessionStoreRoot,
    headers: &[AgentSessionHeader],
) -> Result<(), WriteError> {
    write_atomic(&root.index_path(), &SessionIndexFileRef { headers })
}

/// The on-disk shape of `index.json`: a thin, versioned wrapper so a future
/// index format change has a seam without touching the header schema.
///
/// Borrowing variant used for writes, so callers do not have to clone their
/// header list just to serialize it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionIndexFileRef<'a> {
    headers: &'a [AgentSessionHeader],
}

/// Owning variant used for reads.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionIndexFile {
    headers: Vec<AgentSessionHeader>,
}

/// One file the index-rebuild scan could not use, with why -- retained so the
/// caller can surface a recoverable diagnostic without losing every other
/// session (spec: "One damaged session").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFileDiagnostic {
    pub path: PathBuf,
    pub reason: SessionLoadError,
}

/// The result of loading (or rebuilding) the index: usable headers plus
/// whatever could not be read, and whether a rebuild actually happened.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexLoadResult {
    pub headers: Vec<AgentSessionHeader>,
    pub diagnostics: Vec<SessionFileDiagnostic>,
    pub rebuilt: bool,
}

/// Load `index.json`. If it is missing or cannot be parsed as the current
/// index shape, rebuild it by scanning every file in `sessions/` and write
/// the rebuilt index back out so the next load is fast again.
///
/// This never trusts a stale index over the session files: even a
/// successfully-parsed index that references a session ID with no matching
/// file on disk drops that header rather than presenting it as usable
/// (rebuilding fixes that automatically since the scan is the source of
/// truth).
pub fn load_or_rebuild_index(root: &SessionStoreRoot) -> IndexLoadResult {
    match read_index_file(root) {
        Some(headers) if all_sessions_present(root, &headers) => IndexLoadResult {
            headers: sort_headers(headers),
            diagnostics: Vec::new(),
            rebuilt: false,
        },
        _ => {
            let (headers, diagnostics) = rebuild_index_from_sessions(root);
            // Best-effort: if this write fails, the caller still gets correct
            // in-memory headers this run: the failure is not fatal, and the
            // next load will simply rebuild again.
            let _ = write_index(root, &headers);
            IndexLoadResult {
                headers,
                diagnostics,
                rebuilt: true,
            }
        }
    }
}

/// Read `index.json` and return its headers only if the file parses cleanly
/// as the current index shape. Any failure (missing file, invalid JSON,
/// wrong shape) returns `None` rather than a partial result -- an
/// untrustworthy index is fully rebuilt, not patched.
fn read_index_file(root: &SessionStoreRoot) -> Option<Vec<AgentSessionHeader>> {
    let raw = fs::read(root.index_path()).ok()?;
    let file: SessionIndexFile = serde_json::from_slice(&raw).ok()?;
    Some(file.headers)
}

/// Every header in `headers` must have a matching session file on disk, or
/// the index is considered stale and gets rebuilt.
fn all_sessions_present(root: &SessionStoreRoot, headers: &[AgentSessionHeader]) -> bool {
    headers
        .iter()
        .all(|h| root.session_path(&h.session_id).is_file())
}

/// Scan `sessions/*.json`, reading just enough of each file (`header`) to
/// produce a header list. Files that fail to parse are recorded as
/// diagnostics and omitted -- never moved, never deleted.
///
/// `pub(crate)` so a command that just wrote a session can force a fresh scan
/// after its own mutation, rather than going through
/// [`load_or_rebuild_index`], which trusts a structurally-valid but stale
/// `index.json` as long as every header in it still has a matching file --
/// exactly the case right after a header field changes on an existing
/// session.
pub(crate) fn rebuild_index_from_sessions(
    root: &SessionStoreRoot,
) -> (Vec<AgentSessionHeader>, Vec<SessionFileDiagnostic>) {
    let mut headers = Vec::new();
    let mut diagnostics = Vec::new();

    let entries = match fs::read_dir(root.sessions_dir()) {
        Ok(entries) => entries,
        Err(_) => return (headers, diagnostics),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("json") {
            continue;
        }
        // Ignore the sibling temp files an interrupted write can leave
        // behind (`tempfile` names them with a random suffix, not `.json`,
        // but a defensive check costs nothing and covers any future naming
        // change).
        if path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with('.'))
        {
            continue;
        }

        match read_json_file(&path).and_then(extract_header_value).and_then(migrate_header) {
            Ok(header) => headers.push(header),
            Err(reason) => diagnostics.push(SessionFileDiagnostic { path, reason }),
        }
    }

    (sort_headers(headers), diagnostics)
}

/// Pull just the `header` object out of a full session JSON value, so the
/// index rebuild does not have to deserialize every message/execution in
/// every session file just to render a sidebar row.
fn extract_header_value(mut raw: serde_json::Value) -> Result<serde_json::Value, SessionLoadError> {
    raw.get_mut("header")
        .map(serde_json::Value::take)
        .ok_or_else(|| SessionLoadError::Malformed {
            detail: "missing top-level \"header\" object".into(),
        })
}

/// Read and parse a JSON file, translating I/O and parse failures into
/// [`SessionLoadError`] without ever panicking on a bad file.
///
/// A missing file (`io::ErrorKind::NotFound`) is distinguished from every
/// other I/O failure at the point the error is created, here -- this is the
/// only place that ever sees the raw `std::io::Error` and its `.kind()`.
/// Everything downstream (commands mapping this to a typed outcome) can then
/// tell "the session is really gone" apart from "the file could not be read
/// right now" without re-deriving that distinction itself.
pub(crate) fn read_json_file(path: &Path) -> Result<serde_json::Value, SessionLoadError> {
    let raw = fs::read_to_string(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            SessionLoadError::NotFound
        } else {
            SessionLoadError::Io {
                detail: e.to_string(),
            }
        }
    })?;
    serde_json::from_str(&raw).map_err(|e| SessionLoadError::Malformed {
        detail: e.to_string(),
    })
}

/// Sort headers by `updated_at` descending, breaking ties on `session_id` so
/// the ordering is stable and reproducible across runs (matters for a
/// cursor-paged list: two headers with an identical timestamp must not swap
/// places between pages).
pub fn sort_headers(mut headers: Vec<AgentSessionHeader>) -> Vec<AgentSessionHeader> {
    headers.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    headers
}

/// Filters for a paged session list. Every field is optional/empty-means-off
/// so an unfiltered call is just `SessionListFilter::default()`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionListFilter {
    pub repo_id: Option<String>,
    pub project_path: Option<String>,
    pub states: Vec<super::model::SessionState>,
    pub source_kinds: Vec<&'static str>,
    /// `Some(true)`: only sessions with at least one changed file.
    /// `Some(false)`: only sessions with none. `None`: no filter.
    pub has_changed_files: Option<bool>,
    /// `None` matches both archived and active sessions. `Some(false)` (the
    /// common case) hides archived sessions unless explicitly asked for.
    pub archived: Option<bool>,
    /// Case-insensitive substring match against the title.
    pub title_contains: Option<String>,
}

impl SessionListFilter {
    fn matches(&self, header: &AgentSessionHeader) -> bool {
        if let Some(repo_id) = &self.repo_id {
            if &header.repo_id != repo_id {
                return false;
            }
        }
        if let Some(project_path) = &self.project_path {
            if &header.repo_path != project_path {
                return false;
            }
        }
        if !self.states.is_empty() && !self.states.contains(&header.state) {
            return false;
        }
        if !self.source_kinds.is_empty()
            && !self.source_kinds.contains(&header.source.kind_label())
        {
            return false;
        }
        if let Some(want_changed) = self.has_changed_files {
            let has_changed = header.changed_file_count > 0;
            if has_changed != want_changed {
                return false;
            }
        }
        if let Some(archived) = self.archived {
            if header.archived != archived {
                return false;
            }
        }
        if let Some(needle) = &self.title_contains {
            if !needle.is_empty()
                && !header
                    .title
                    .to_lowercase()
                    .contains(&needle.to_lowercase())
            {
                return false;
            }
        }
        true
    }
}

/// One page of the session list.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionListPage {
    pub headers: Vec<AgentSessionHeader>,
    /// Pass this back as the next call's cursor to continue. `None` once the
    /// last page has been returned.
    pub next_cursor: Option<String>,
    pub diagnostics: Vec<SessionFileDiagnostic>,
}

/// List sessions, filtered and paged, with no reconciliation applied -- every
/// header is shown exactly as read. Callers that display sessions to a user
/// should prefer [`list_sessions_reconciled`] (see its own doc comment for
/// why: a `states` filter needs to see the post-reconciliation state to be
/// honest about what it matches). This plain version exists for callers that
/// have no `RunSessionLinks` to consult -- store-layer tests, and any future
/// non-Tauri caller of this module -- and is kept deliberately non-lossy:
/// passing `|_| true` ("every session is live") to `list_sessions_reconciled`
/// would be indistinguishable from this in its effect (nothing gets
/// reconciled either way), but reads as an assertion about liveness this
/// function has no basis for; `|_| false` is wrong in the other direction,
/// since [`reconcile_header`](super::session_recovery::reconcile_header) does
/// not additionally check `active_execution_id`, so it would reconcile every
/// `Preparing`/`Working`/`NeedsInput` header this function is asked to list,
/// including ones a caller populated for reasons that have nothing to do with
/// process liveness (exactly what several of this module's own fixture-based
/// tests do). A dedicated no-op closure name states that choice plainly
/// rather than leaving `|_| true` looking like an arbitrary pick between two
/// closures with the same observable effect.
pub fn list_sessions(
    root: &SessionStoreRoot,
    filter: &SessionListFilter,
    cursor: Option<&str>,
    limit: usize,
) -> SessionListPage {
    list_sessions_reconciled(root, filter, cursor, limit, |_| true)
}

/// [`list_sessions`], but every loaded header is first offered to
/// `execution_is_live` (given the header, answer "is this header's
/// `active_execution_id` actually backed by a live process right now") and
/// reconciled in place via [`super::session_recovery::reconcile_header`] --
/// this is what stops a session's stale `Preparing`/`Working`/`NeedsInput`
/// header from ever reaching the *filter*: without reconciling before
/// `filter.matches`, a `states` filter for `Interrupted` would miss a session
/// still reading `Working` on disk, and vice versa.
///
/// This module has no dependency on `RunSessionLinks` or any Tauri type by
/// design (see this module's own doc comment and `agentdesk::session_recovery`'s),
/// so the caller supplies the liveness answer as a plain closure rather than
/// this function reaching for that registry itself.
///
/// Deliberately does **not** write anything back to disk -- this only fixes
/// what the returned page displays. The durable fix (and the one that
/// actually unblocks starting a new execution) happens lazily, the first
/// time a session is actually opened, in `commands::agent_desk::get_session_at`.
/// Persisting a fix for every stale header on every list call, for
/// potentially hundreds of sessions, would mean a `SessionLocks` acquisition
/// and a full read-modify-write per stale session on every sidebar refresh --
/// exactly the per-call cost this function is built to avoid; a session the
/// user never re-opens simply keeps showing correctly in the list without
/// ever needing a write.
pub fn list_sessions_reconciled(
    root: &SessionStoreRoot,
    filter: &SessionListFilter,
    cursor: Option<&str>,
    limit: usize,
    mut execution_is_live: impl FnMut(&AgentSessionHeader) -> bool,
) -> SessionListPage {
    let loaded = load_or_rebuild_index(root);
    let reconciled: Vec<AgentSessionHeader> = loaded
        .headers
        .into_iter()
        .map(|mut header| {
            let live = execution_is_live(&header);
            let _ = super::session_recovery::reconcile_header(&mut header, live);
            header
        })
        .collect();
    let matched: Vec<AgentSessionHeader> = reconciled.into_iter().filter(|h| filter.matches(h)).collect();

    let start = cursor
        .and_then(|c| c.parse::<usize>().ok())
        .filter(|&offset| offset <= matched.len())
        .unwrap_or(0);

    let limit = limit.max(1);
    let end = matched.len().min(start + limit);
    let page: Vec<AgentSessionHeader> = matched[start..end].to_vec();
    let next_cursor = if end < matched.len() {
        Some(end.to_string())
    } else {
        None
    };

    SessionListPage {
        headers: page,
        next_cursor,
        diagnostics: loaded.diagnostics,
    }
}

/// A duplicate session ID found on disk: two different files would both
/// resolve to the same `session_id` inside their `header`, which can only
/// happen if a file was hand-edited or copied -- [`write_session`] itself
/// always writes to the path derived from that same ID, so it cannot
/// produce this on its own. Surfaced by [`find_duplicate_session_ids`] so a
/// caller can decide how to react without the scan silently picking one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateSessionId {
    pub session_id: SessionId,
    pub paths: Vec<PathBuf>,
}

/// Scan `sessions/*.json` for header `session_id` values that do not match
/// the file's own name, grouping by the ID actually found inside the file.
/// An index rebuild already de-duplicates by filename (one header per file
/// path), so this exists to catch content-level duplication a rebuild alone
/// would not notice.
pub fn find_duplicate_session_ids(root: &SessionStoreRoot) -> Vec<DuplicateSessionId> {
    use std::collections::HashMap;

    let mut by_id: HashMap<SessionId, Vec<PathBuf>> = HashMap::new();
    let Ok(entries) = fs::read_dir(root.sessions_dir()) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("json") {
            continue;
        }
        if let Ok(raw) = read_json_file(&path) {
            if let Ok(header) = extract_header_value(raw).and_then(migrate_header) {
                by_id.entry(header.session_id).or_default().push(path);
            }
        }
    }

    by_id
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(session_id, mut paths)| {
            paths.sort();
            DuplicateSessionId { session_id, paths }
        })
        .collect()
}

/// Touch a temp file in `dir` and leave it behind without ever renaming it
/// into place, simulating a write that was interrupted between temp-file
/// creation and rename. Test-only: production code has no reason to leave a
/// temp file behind on purpose.
#[cfg(test)]
fn leave_interrupted_temp_file(dir: &Path, contents: &str) -> PathBuf {
    let mut temp = NamedTempFile::new_in(dir).expect("create temp file for test fixture");
    io::Write::write_all(&mut temp, contents.as_bytes()).expect("write temp fixture contents");
    temp.as_file_mut().sync_all().expect("flush temp fixture");
    // `keep()` detaches the guard so the temp file survives past this
    // function instead of being deleted when `temp` drops.
    let (_file, path) = temp.keep().expect("keep interrupted temp file for test");
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::{
        AgentSession, AgentSessionHeader, SessionIntent, SessionSource, SessionState,
        CURRENT_SCHEMA_VERSION,
    };
    use tempfile::TempDir;

    fn temp_root() -> (TempDir, SessionStoreRoot) {
        let dir = TempDir::new().expect("create temp dir");
        let root = SessionStoreRoot::at(dir.path().join("agent-desk").join("v1"))
            .expect("init store root");
        (dir, root)
    }

    fn header(session_id: &str, updated_at: &str) -> AgentSessionHeader {
        AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: session_id.into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/proj".into(),
            repo_name: "proj".into(),
            title: format!("Session {session_id}"),
            source: SessionSource::Manual {
                repo_id: "repo-1".into(),
            },
            intent: SessionIntent::Ask,
            state: SessionState::Ready,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: updated_at.into(),
            unread: false,
            changed_file_count: 0,
            active_execution_id: None,
            archived: false,
        }
    }

    fn session(session_id: &str, updated_at: &str) -> AgentSession {
        AgentSession::new(header(session_id, updated_at))
    }

    // -- 2.2 / 2.3: atomic write + read round trip --

    #[test]
    fn a_written_session_reads_back_identical() {
        let (_dir, root) = temp_root();
        let s = session("sess-1", "2026-01-01T00:00:00Z");
        write_session(&root, &s).expect("write session");
        let back = read_session(&root, "sess-1").expect("read session");
        assert_eq!(back, s);
    }

    #[test]
    fn writing_a_session_leaves_no_temp_file_behind() {
        let (_dir, root) = temp_root();
        write_session(&root, &session("sess-1", "2026-01-01T00:00:00Z")).unwrap();
        let entries: Vec<_> = fs::read_dir(root.sessions_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1, "only the final session file should remain");
        assert_eq!(entries[0].path().extension().unwrap(), "json");
    }

    #[test]
    fn a_second_write_replaces_the_first_atomically() {
        let (_dir, root) = temp_root();
        let mut s = session("sess-1", "2026-01-01T00:00:00Z");
        write_session(&root, &s).unwrap();
        s.header.title = "Renamed".into();
        s.header.updated_at = "2026-01-02T00:00:00Z".into();
        write_session(&root, &s).unwrap();

        let back = read_session(&root, "sess-1").unwrap();
        assert_eq!(back.header.title, "Renamed");
    }

    #[test]
    fn writing_and_reading_the_index_round_trips() {
        let (_dir, root) = temp_root();
        let s = session("sess-1", "2026-01-01T00:00:00Z");
        write_session(&root, &s).unwrap();
        write_index(&root, &[s.header.clone()]).unwrap();

        let loaded = load_or_rebuild_index(&root);
        assert!(!loaded.rebuilt);
        assert_eq!(loaded.headers, vec![s.header]);
        assert!(loaded.diagnostics.is_empty());
    }

    // -- 2.4: rebuild when index missing or invalid --

    #[test]
    fn a_missing_index_is_rebuilt_from_session_files() {
        let (_dir, root) = temp_root();
        write_session(&root, &session("sess-1", "2026-01-01T00:00:00Z")).unwrap();
        write_session(&root, &session("sess-2", "2026-01-02T00:00:00Z")).unwrap();

        assert!(!root.index_path().exists());
        let loaded = load_or_rebuild_index(&root);
        assert!(loaded.rebuilt);
        assert_eq!(loaded.headers.len(), 2);
        assert!(loaded.diagnostics.is_empty());
        // The rebuild also persists, so the next load does not rebuild again.
        assert!(root.index_path().exists());
        let second = load_or_rebuild_index(&root);
        assert!(!second.rebuilt);
    }

    #[test]
    fn an_invalid_index_file_is_rebuilt_rather_than_trusted() {
        let (_dir, root) = temp_root();
        write_session(&root, &session("sess-1", "2026-01-01T00:00:00Z")).unwrap();
        fs::write(root.index_path(), b"not json at all").unwrap();

        let loaded = load_or_rebuild_index(&root);
        assert!(loaded.rebuilt);
        assert_eq!(loaded.headers.len(), 1);
    }

    #[test]
    fn a_stale_index_referencing_a_missing_session_file_is_rebuilt() {
        let (_dir, root) = temp_root();
        let s = session("sess-1", "2026-01-01T00:00:00Z");
        write_session(&root, &s).unwrap();
        // Index claims a session that was never (or no longer is) on disk.
        let ghost = header("sess-ghost", "2026-01-01T00:00:00Z");
        write_index(&root, &[s.header.clone(), ghost]).unwrap();

        let loaded = load_or_rebuild_index(&root);
        assert!(loaded.rebuilt);
        assert_eq!(loaded.headers, vec![s.header]);
    }

    // -- 2.5: quarantine logically, never move/delete the bad file --

    #[test]
    fn one_corrupt_session_file_is_skipped_but_others_remain_and_the_file_is_untouched() {
        let (_dir, root) = temp_root();
        write_session(&root, &session("sess-good", "2026-01-01T00:00:00Z")).unwrap();

        let bad_path = root.sessions_dir().join("sess-bad.json");
        fs::write(&bad_path, b"{ this is not valid json").unwrap();

        let loaded = load_or_rebuild_index(&root);
        assert_eq!(loaded.headers.len(), 1);
        assert_eq!(loaded.headers[0].session_id, "sess-good");
        assert_eq!(loaded.diagnostics.len(), 1);
        assert_eq!(loaded.diagnostics[0].path, bad_path);
        assert!(
            matches!(loaded.diagnostics[0].reason, SessionLoadError::Malformed { .. }),
            "unexpected reason: {:?}",
            loaded.diagnostics[0].reason
        );

        // The bad file must still exist, byte-for-byte, at its original path.
        let contents = fs::read_to_string(&bad_path).unwrap();
        assert_eq!(contents, "{ this is not valid json");
    }

    #[test]
    fn a_session_with_a_future_schema_version_is_reported_and_skipped() {
        let (_dir, root) = temp_root();
        let mut s = session("sess-future", "2026-01-01T00:00:00Z");
        s.header.schema_version = CURRENT_SCHEMA_VERSION + 1;
        let path = root.session_path("sess-future");
        // Bypass write_session's normal path since it would write a header
        // whose version does not match what migrate would expect from a
        // *real* future build, but the shape is what matters for this test.
        let json = serde_json::to_vec_pretty(&s).unwrap();
        fs::write(&path, json).unwrap();

        let result = read_session(&root, "sess-future");
        assert!(matches!(
            result,
            Err(SessionLoadError::UnsupportedSchemaVersion { .. })
        ));

        let loaded = load_or_rebuild_index(&root);
        assert_eq!(loaded.headers.len(), 0);
        assert_eq!(loaded.diagnostics.len(), 1);
        assert!(matches!(
            loaded.diagnostics[0].reason,
            SessionLoadError::UnsupportedSchemaVersion { .. }
        ));
    }

    // -- 2.6: sort order --

    #[test]
    fn headers_sort_by_updated_at_descending_with_session_id_tie_break() {
        let headers = vec![
            header("b", "2026-01-01T00:00:00Z"),
            header("a", "2026-01-02T00:00:00Z"),
            header("z", "2026-01-02T00:00:00Z"),
            header("a", "2026-01-03T00:00:00Z"),
        ];
        let sorted = sort_headers(headers);
        let ids: Vec<&str> = sorted.iter().map(|h| h.session_id.as_str()).collect();
        // 01-03 first, then the 01-02 tie broken by session_id ascending,
        // then 01-01 last.
        assert_eq!(ids, vec!["a", "a", "z", "b"]);
    }

    // -- 2.7: filters --

    #[test]
    fn list_sessions_filters_by_repo_id() {
        let (_dir, root) = temp_root();
        let mut s2 = session("sess-2", "2026-01-02T00:00:00Z");
        s2.header.repo_id = "repo-2".into();
        write_session(&root, &session("sess-1", "2026-01-01T00:00:00Z")).unwrap();
        write_session(&root, &s2).unwrap();

        let filter = SessionListFilter {
            repo_id: Some("repo-2".into()),
            ..Default::default()
        };
        let page = list_sessions(&root, &filter, None, 10);
        assert_eq!(page.headers.len(), 1);
        assert_eq!(page.headers[0].session_id, "sess-2");
    }

    #[test]
    fn list_sessions_filters_by_project_path_state_source_kind_and_changed_files() {
        let (_dir, root) = temp_root();

        let mut fix_session = session("sess-fix", "2026-01-01T00:00:00Z");
        fix_session.header.repo_path = "C:/code/other".into();
        fix_session.header.state = SessionState::Working;
        fix_session.header.changed_file_count = 3;

        let mut issue_session = session("sess-issue", "2026-01-02T00:00:00Z");
        issue_session.header.source = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 1,
            url: "https://example.test/issues/1".into(),
            snapshot: crate::agentdesk::model::SourceSnapshot {
                title: "t".into(),
                summary: "s".into(),
                captured_at: "2026-01-01T00:00:00Z".into(),
                live_unavailable: false,
            },
        };

        write_session(&root, &fix_session).unwrap();
        write_session(&root, &issue_session).unwrap();

        let by_path = list_sessions(
            &root,
            &SessionListFilter {
                project_path: Some("C:/code/other".into()),
                ..Default::default()
            },
            None,
            10,
        );
        assert_eq!(by_path.headers.len(), 1);
        assert_eq!(by_path.headers[0].session_id, "sess-fix");

        let by_state = list_sessions(
            &root,
            &SessionListFilter {
                states: vec![SessionState::Working],
                ..Default::default()
            },
            None,
            10,
        );
        assert_eq!(by_state.headers.len(), 1);
        assert_eq!(by_state.headers[0].session_id, "sess-fix");

        let by_source = list_sessions(
            &root,
            &SessionListFilter {
                source_kinds: vec!["issue"],
                ..Default::default()
            },
            None,
            10,
        );
        assert_eq!(by_source.headers.len(), 1);
        assert_eq!(by_source.headers[0].session_id, "sess-issue");

        let by_changed = list_sessions(
            &root,
            &SessionListFilter {
                has_changed_files: Some(true),
                ..Default::default()
            },
            None,
            10,
        );
        assert_eq!(by_changed.headers.len(), 1);
        assert_eq!(by_changed.headers[0].session_id, "sess-fix");
    }

    #[test]
    fn list_sessions_filters_archived_and_title_text() {
        let (_dir, root) = temp_root();
        let mut archived = session("sess-archived", "2026-01-01T00:00:00Z");
        archived.header.archived = true;
        archived.header.title = "Old thing".into();
        let mut active = session("sess-active", "2026-01-02T00:00:00Z");
        active.header.title = "Fix the Widget Loader".into();

        write_session(&root, &archived).unwrap();
        write_session(&root, &active).unwrap();

        let default_hides_archived = list_sessions(
            &root,
            &SessionListFilter {
                archived: Some(false),
                ..Default::default()
            },
            None,
            10,
        );
        assert_eq!(default_hides_archived.headers.len(), 1);
        assert_eq!(default_hides_archived.headers[0].session_id, "sess-active");

        let only_archived = list_sessions(
            &root,
            &SessionListFilter {
                archived: Some(true),
                ..Default::default()
            },
            None,
            10,
        );
        assert_eq!(only_archived.headers.len(), 1);
        assert_eq!(only_archived.headers[0].session_id, "sess-archived");

        let by_title = list_sessions(
            &root,
            &SessionListFilter {
                title_contains: Some("widget".into()),
                ..Default::default()
            },
            None,
            10,
        );
        assert_eq!(by_title.headers.len(), 1);
        assert_eq!(by_title.headers[0].session_id, "sess-active");
    }

    // -- 2.8: 1,000 sessions, corrupt file, interrupted temp file, duplicate ID --

    #[test]
    fn one_thousand_sessions_page_correctly_in_sorted_order() {
        let (_dir, root) = temp_root();
        for i in 0..1000u32 {
            let id = format!("sess-{i:04}");
            // Spread updated_at so ordering is meaningfully exercised, not a
            // thousand-way tie broken only by ID.
            let updated_at = format!("2026-01-01T{:02}:{:02}:00Z", i / 60 % 24, i % 60);
            write_session(&root, &session(&id, &updated_at)).unwrap();
        }

        let loaded = load_or_rebuild_index(&root);
        assert!(loaded.rebuilt);
        assert_eq!(loaded.headers.len(), 1000);
        assert!(loaded.diagnostics.is_empty());

        // Walk every page with a small limit and confirm: every session is
        // seen exactly once, and the sequence is non-increasing by
        // updated_at (descending sort held across page boundaries).
        let mut seen = std::collections::HashSet::new();
        let mut cursor: Option<String> = None;
        let mut last_updated_at: Option<String> = None;
        loop {
            let page = list_sessions(&root, &SessionListFilter::default(), cursor.as_deref(), 37);
            for h in &page.headers {
                assert!(seen.insert(h.session_id.clone()), "duplicate in paging: {}", h.session_id);
                if let Some(prev) = &last_updated_at {
                    assert!(prev >= &h.updated_at, "page order broke sort");
                }
                last_updated_at = Some(h.updated_at.clone());
            }
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn one_corrupt_file_among_a_thousand_is_isolated() {
        let (_dir, root) = temp_root();
        for i in 0..999u32 {
            let id = format!("sess-{i:04}");
            write_session(&root, &session(&id, "2026-01-01T00:00:00Z")).unwrap();
        }
        let bad_path = root.sessions_dir().join("sess-corrupt.json");
        fs::write(&bad_path, b"{{{{not json").unwrap();

        let loaded = load_or_rebuild_index(&root);
        assert_eq!(loaded.headers.len(), 999);
        assert_eq!(loaded.diagnostics.len(), 1);
        assert_eq!(loaded.diagnostics[0].path, bad_path);
        assert!(bad_path.exists(), "corrupt file must not be deleted");
    }

    #[test]
    fn an_interrupted_temp_file_is_ignored_by_the_index_rebuild() {
        let (_dir, root) = temp_root();
        write_session(&root, &session("sess-1", "2026-01-01T00:00:00Z")).unwrap();
        let interrupted = leave_interrupted_temp_file(&root.sessions_dir(), "{ incomplete");

        let loaded = load_or_rebuild_index(&root);
        assert_eq!(loaded.headers.len(), 1);
        assert!(
            loaded.diagnostics.is_empty(),
            "a non-.json temp artifact must not even be reported, since it is not a session file"
        );
        // It also must not have been touched.
        assert!(interrupted.exists());
        assert_eq!(fs::read_to_string(&interrupted).unwrap(), "{ incomplete");
    }

    #[test]
    fn interrupting_a_session_write_leaves_the_previous_file_untouched() {
        let (_dir, root) = temp_root();
        let original = session("sess-1", "2026-01-01T00:00:00Z");
        write_session(&root, &original).unwrap();

        // Simulate a write that got a temp file down but crashed before the
        // rename: leave a stray temp file in the sessions dir without ever
        // calling write_session again.
        let _stray = leave_interrupted_temp_file(&root.sessions_dir(), "{ half written");

        let back = read_session(&root, "sess-1").expect("original session still reads fine");
        assert_eq!(back, original);
    }

    #[test]
    fn duplicate_session_ids_across_two_files_are_detected() {
        let (_dir, root) = temp_root();
        let s = session("sess-dup", "2026-01-01T00:00:00Z");
        write_session(&root, &s).unwrap();

        // Hand-craft a second file with a different filename but the same
        // session_id inside its header -- write_session itself cannot
        // produce this, which is exactly why it needs a separate check.
        let copy_path = root.sessions_dir().join("sess-dup-copy.json");
        let json = serde_json::to_vec_pretty(&s).unwrap();
        fs::write(&copy_path, json).unwrap();

        let duplicates = find_duplicate_session_ids(&root);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].session_id, "sess-dup");
        assert_eq!(duplicates[0].paths.len(), 2);
    }

    #[test]
    fn no_duplicates_are_reported_when_every_file_has_a_distinct_id() {
        let (_dir, root) = temp_root();
        write_session(&root, &session("sess-1", "2026-01-01T00:00:00Z")).unwrap();
        write_session(&root, &session("sess-2", "2026-01-02T00:00:00Z")).unwrap();
        assert!(find_duplicate_session_ids(&root).is_empty());
    }

    // -- StoreInitError seam --

    #[test]
    fn store_root_creates_its_directory_layout() {
        let (dir, _root) = temp_root();
        assert!(dir.path().join("agent-desk").join("v1").is_dir());
        assert!(dir
            .path()
            .join("agent-desk")
            .join("v1")
            .join(SESSIONS_DIR)
            .is_dir());
    }
}
