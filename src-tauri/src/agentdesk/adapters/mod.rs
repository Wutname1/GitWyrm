//! External chat client adapters: read-only discovery and import of sessions
//! from Codex, Claude Code, OpenCode, VS Code Copilot, and OpenChamber.
//!
//! # The read-only guarantee
//!
//! An adapter's entire contract is: **never write to, lock, move, or truncate
//! a foreign client's files.** Every adapter method in [`AgentClientAdapter`]
//! takes `&self` and returns owned data -- there is no method here that
//! accepts anything to write back into the client's directory. Adapters open
//! foreign files with [`open_read_only`], which asks the OS for a read handle
//! only (`OpenOptions::new().read(true).write(false)`); nothing under
//! `adapters/` ever constructs an `OpenOptions` with `.write(true)`,
//! `.append(true)`, `.create(true)`, or `.truncate(true)` pointed at a
//! foreign path, and [`tests::no_adapter_source_opens_foreign_paths_for_writing`]
//! greps every adapter source file to keep that true mechanically, not just
//! by convention. Any GitWyrm-side bookkeeping (cursors, dedupe state) is
//! kept entirely under GitWyrm's own app-data directory, never inside a
//! detected client's directory -- see `super::store` for where that lives.
//!
//! # Failure isolation
//!
//! One adapter timing out, panicking, or hitting a corrupt file must never
//! stop native Agent Desk sessions or any other adapter from working. The
//! registry in this module (see [`AdapterRegistry`]) runs each adapter's
//! `detect` with its own timeout and catches panics with
//! [`std::panic::catch_unwind`], turning either failure mode into a
//! [`DetectionOutcome::Failed`] entry rather than propagating.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

pub mod claude_code;
pub mod codex;
pub mod opencode;
pub mod openchamber;
pub mod vscode_copilot;

#[cfg(test)]
pub mod fixtures;

/// Stable identifier for one adapter. Persisted inside [`super::model::ImportProvenance`]
/// and used as the capability-flag key, so it must never change once shipped.
pub type AdapterId = &'static str;

/// One external session as an adapter's `list` reports it -- enough to render
/// a picker row before any message content is read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSessionSummary {
    pub external_session_id: String,
    pub title: String,
    /// RFC 3339 UTC timestamp of the session's own last-modified time, taken
    /// from the file/record itself, never from import time.
    pub updated_at: String,
    /// The absolute path the external client itself recorded as the working
    /// directory/project for this session, if the client records one at all.
    /// Reconciled against known repos by `super::reconcile`, never guessed at
    /// here.
    pub project_path: Option<String>,
    pub message_count: u32,
    pub model: Option<String>,
}

/// One message as read from an external client, before it is mapped into a
/// [`super::model::SessionMessage`]. Conservative: only what nearly every
/// client's transcript format actually carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMessage {
    pub external_message_id: String,
    pub role: ExternalRole,
    /// RFC 3339 UTC timestamp.
    pub timestamp: String,
    pub plain_content: String,
    pub model: Option<String>,
    /// `true` when this record's shape was not recognized as a normal
    /// user/assistant turn (a tool event, a client-internal marker, an event
    /// kind added by a newer client version) -- see task 2.1: "preserve
    /// unknown events as labeled raw-import records rather than discarding
    /// them." The record is still imported, with `plain_content` holding
    /// whatever text could be salvaged and `role` best-efforted to `System`.
    pub raw_unrecognized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExternalRole {
    User,
    Assistant,
    System,
}

/// A full external session: its summary plus every message read, in
/// client-original order. `read_session` returns this in one shot rather than
/// a paged stream -- import fixtures top out at 1,000 *sessions*, not 1,000
/// messages in one session, so this stays simple.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSessionDetail {
    pub summary: ExternalSessionSummary,
    pub messages: Vec<ExternalMessage>,
}

/// Whether, and how, "Continue externally" can launch this client on this
/// session. See design.md: "never claims the external client accepted
/// context when it merely opened."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ContinuationCapability {
    /// The client has a documented resume mechanism for a specific session,
    /// and GitWyrm can launch it. The UI may say "Continue session."
    ResumeSession,
    /// The client can be opened (e.g. at the project directory) but there is
    /// no supported way to hand it a specific prior session. The UI must say
    /// "Open client," never "Continue session" (spec scenario "Open only").
    OpenOnly,
    /// No supported launch mechanism exists at all for this client on this
    /// platform/version.
    Unsupported,
}

/// Why an adapter could not do what was asked. Every variant here is a real,
/// distinguishable next action -- never a bare string the UI has to sniff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AdapterError {
    #[error("client not detected on this machine")]
    ClientNotDetected,
    #[error("client version {found} is not in the supported range {supported_range}")]
    #[serde(rename_all = "camelCase")]
    UnsupportedVersion {
        found: String,
        supported_range: String,
    },
    #[error("expected path is missing: {path}")]
    MissingPath { path: String },
    #[error("session data could not be parsed: {detail}")]
    CorruptSession { detail: String },
    #[error("session {external_session_id} was not found")]
    #[serde(rename_all = "camelCase")]
    SessionNotFound { external_session_id: String },
    /// More than one saved conversation carries this id, so which one was
    /// meant cannot be worked out.
    ///
    /// Ids are only promised unique by the client that wrote them, and some
    /// do not manage it: VS Code keeps one folder per workspace, and a
    /// restored backup, a synced settings profile or a cloned machine can
    /// reproduce the same id under two of them. Picking the first match
    /// returned a real conversation that was simply not the one asked for,
    /// which nothing downstream could detect.
    ///
    /// Refusing is not a fix -- those conversations become unimportable
    /// rather than wrongly importable -- but it is honest, and it is the
    /// half that can be done without changing the id, which is a key already
    /// written to disk for everything imported so far.
    #[error("more than one saved conversation has the id {external_session_id}")]
    #[serde(rename_all = "camelCase")]
    AmbiguousSession {
        external_session_id: String,
        matches: u32,
    },
    #[error("reading timed out after {millis}ms")]
    TimedOut { millis: u32 },
    #[error("could not read: {detail}")]
    Io { detail: String },
}

/// What detecting one adapter found. Distinct from [`AdapterError`]: this is
/// the *registry's* view across every adapter (task 1.2), so a panic and a
/// timeout are outcomes here rather than propagated errors -- one bad adapter
/// must never take the scan down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DetectionOutcome {
    Detected {
        version: Option<String>,
        supported: bool,
    },
    NotDetected,
    /// `detect` returned an error, timed out, or panicked. `detail` is
    /// logged, never a live error object -- see task 5.2 (redact from normal
    /// logs) for why message content specifically must never land here.
    Failed { detail: String },
}

/// Everything an adapter must implement. Every method is `&self` and returns
/// owned data -- there is intentionally no method that takes anything to
/// write into the external client's directory. See the module doc for how
/// that is enforced and tested.
pub trait AgentClientAdapter: Send + Sync {
    /// Stable ID, e.g. `"codex"`. Used as the capability-flag key and
    /// embedded in every imported message's `ImportProvenance::adapter_id`.
    fn id(&self) -> AdapterId;

    /// Display name for UI copy, e.g. `"Codex"`.
    fn display_name(&self) -> &'static str;

    /// The inclusive version range this adapter's parsing was validated
    /// against, for display in the detected-clients UI (task 4.1). A version
    /// outside this range still detects, but `supported` in
    /// [`DetectionOutcome::Detected`] is `false`.
    fn supported_version_range(&self) -> &'static str;

    /// Look for the client's data on this machine. Must be cheap and must
    /// never write anything. Returns `Ok(None)` when the client's home
    /// directory does not exist at all, matching [`AdapterError::ClientNotDetected`]
    /// one level up in the registry.
    fn detect(&self) -> Result<Option<DetectedClient>, AdapterError>;

    /// List sessions found for the detected client, newest-first, optionally
    /// only those touching `project_path` when the client records one. No
    /// message bodies here -- see [`Self::read_session`].
    fn list_sessions(
        &self,
        client: &DetectedClient,
    ) -> Result<Vec<ExternalSessionSummary>, AdapterError>;

    /// Read one session's full message list.
    fn read_session(
        &self,
        client: &DetectedClient,
        external_session_id: &str,
    ) -> Result<ExternalSessionDetail, AdapterError>;

    /// Whether/how "Continue externally" can work for one session.
    fn continuation_capability(
        &self,
        client: &DetectedClient,
        external_session_id: &str,
    ) -> ContinuationCapability;
}

/// What `detect` found: enough for `list_sessions`/`read_session` to locate
/// data without re-probing the filesystem for the client's home directory
/// every call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedClient {
    pub home_dir: std::path::PathBuf,
    pub version: Option<String>,
    pub supported: bool,
}

/// Longest string [`redact_for_log`] lets through unchanged. Adapter ids,
/// external session ids, error variant names and version strings all fit;
/// a message body, a title typed by a person, or a serialized record does
/// not, and those are exactly what must never reach the log (task 5.2).
pub const REDACT_LOG_MAX_CHARS: usize = 96;

/// Make a string safe to log from the import path. Imported chats belong to
/// the user and their paths reveal where they keep their work, so nothing
/// that could carry either is logged raw:
///
/// * any token that looks like an absolute path (Windows drive or UNC,
///   POSIX, or `~/`) is replaced with `<path>`, even inside an OS error
///   message such as "could not open C:\Users\me\.codex\x.jsonl";
/// * anything longer than [`REDACT_LOG_MAX_CHARS`] after that is replaced
///   entirely with a length marker, since long text in this path is almost
///   always message content.
///
/// Short identifiers pass through untouched, so log lines stay useful for
/// "which adapter, which outcome" without ever answering "what did they say".
pub fn redact_for_log(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut first = true;
    for token in input.split(' ') {
        if !first {
            out.push(' ');
        }
        first = false;
        out.push_str(&redact_token(token));
    }
    let char_count = out.chars().count();
    if char_count > REDACT_LOG_MAX_CHARS {
        return format!("<redacted {char_count} chars>");
    }
    out
}

fn redact_token(token: &str) -> String {
    // Punctuation an error message wraps a path in (quotes, parentheses, a
    // trailing period or colon) is kept so the sentence still reads, and
    // only the path inside it is replaced.
    let leading: &[char] = &['"', '\'', '(', '[', '<', '`'];
    let trailing: &[char] = &['"', '\'', ')', ']', '>', '`', ',', '.', ';', ':'];
    let start = token.len() - token.trim_start_matches(leading).len();
    let core_end = token.trim_end_matches(trailing).len().max(start);
    let (prefix, rest) = token.split_at(start);
    let (core, suffix) = rest.split_at(core_end - start);
    if looks_like_path(core) {
        format!("{prefix}<path>{suffix}")
    } else {
        token.to_string()
    }
}

fn looks_like_path(s: &str) -> bool {
    let bytes = s.as_bytes();
    // Windows drive path: C:\ or C:/
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    // UNC share: \\server\share
    let unc = s.starts_with("\\\\") && s.len() > 2;
    // POSIX absolute path: /home/x, /tmp/y. A bare "/" or "a/b" is not one.
    let posix = s.starts_with('/') && s.len() > 1;
    // Home-relative: ~/.config/x
    let home = s.starts_with("~/") && s.len() > 2;
    drive || unc || posix || home
}

/// Open a foreign path for reading only. This is the *only* sanctioned way
/// adapter code touches a file inside a detected client's directory --
/// [`std::fs::File::open`] itself already opens read-only, but this wrapper
/// exists as the single, greppable call site the no-writes proof anchors on
/// (see the module doc and [`tests::no_adapter_source_opens_foreign_paths_for_writing`]),
/// and to normalize the I/O error into [`AdapterError`] in one place.
pub fn open_read_only(path: &Path) -> Result<std::fs::File, AdapterError> {
    std::fs::File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AdapterError::MissingPath {
                path: path.display().to_string(),
            }
        } else {
            AdapterError::Io {
                detail: e.to_string(),
            }
        }
    })
}

/// Read a foreign file fully into a string, read-only, bounding how much is
/// ever pulled into memory for one file so a single enormous or adversarial
/// session file cannot exhaust memory during a scan.
const MAX_FOREIGN_FILE_BYTES: u64 = 64 * 1024 * 1024;

pub fn read_foreign_file_to_string(path: &Path) -> Result<String, AdapterError> {
    let mut file = open_read_only(path)?;
    let len = file
        .metadata()
        .map_err(|e| AdapterError::Io {
            detail: e.to_string(),
        })?
        .len();
    if len > MAX_FOREIGN_FILE_BYTES {
        return Err(AdapterError::CorruptSession {
            detail: format!(
                "{} is {len} bytes, over the {MAX_FOREIGN_FILE_BYTES}-byte read cap",
                path.display()
            ),
        });
    }
    // Read bytes, then convert lossily. Strict `read_to_string` fails the
    // WHOLE file for one invalid byte, and that is not how the rest of this
    // behaves: the listing path reads the same file line by line and skips a
    // bad one, so a conversation could list with a title and a message count
    // and then refuse to open, losing every other message in it -- including
    // the ones the list had just shown.
    //
    // A torn multi-byte character at the end of a file another program is
    // still writing produces exactly that, and so does a pasted fragment in
    // another encoding. Replacing the unreadable bytes keeps every message
    // that IS readable, which is the outcome the person wanted; refusing the
    // file keeps none of them.
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| AdapterError::Io {
        detail: e.to_string(),
    })?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// -- 1.2: registry, independent timeouts, failure isolation --

/// How long [`AdapterRegistry::detect_all`] waits for one adapter's `detect`
/// before giving up on it and moving to the next. Detection is meant to be a
/// handful of filesystem stats, so this is generous without letting one slow
/// or hung disk (a network drive, a stalled external client) block the whole
/// scan for long.
pub const DETECTION_TIMEOUT: Duration = Duration::from_secs(3);

/// One adapter's outcome from a registry-wide scan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdapterDetectionResult {
    pub adapter_id: String,
    pub display_name: String,
    pub outcome: DetectionOutcome,
}

/// Holds every registered adapter and runs detection across all of them with
/// per-adapter isolation: a timeout, and a caught panic, both turned into
/// [`DetectionOutcome::Failed`] rather than aborting the scan (task 1.2,
/// spec "Adapters fail independently").
pub struct AdapterRegistry {
    adapters: Vec<std::sync::Arc<dyn AgentClientAdapter>>,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::with_default_adapters()
    }
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// The registry used in production: every adapter this change ships,
    /// each still gated by its own capability flag one layer up in
    /// `commands::agent_import` (task 1.4) -- being registered here only
    /// means "eligible to detect," not "shown to the user."
    pub fn with_default_adapters() -> Self {
        let mut registry = Self::new();
        registry.register(std::sync::Arc::new(codex::CodexAdapter::default()));
        registry.register(std::sync::Arc::new(claude_code::ClaudeCodeAdapter::default()));
        registry.register(std::sync::Arc::new(opencode::OpenCodeAdapter::default()));
        registry.register(std::sync::Arc::new(vscode_copilot::VsCodeCopilotAdapter::default()));
        registry.register(std::sync::Arc::new(openchamber::OpenChamberAdapter::default()));
        registry
    }

    /// Registers an adapter. Takes an owned `Arc` rather than a `Box` (or a
    /// borrow) specifically so [`Self::detect_all`] can clone a `'static`
    /// handle onto a genuinely detached thread for the timeout below --
    /// `std::thread::scope` was tried first and rejected: a scoped thread's
    /// join happens when the *scope* exits, not when the caller decides to
    /// stop waiting, so a hung adapter still blocked the whole scan for its
    /// full hang duration regardless of the timeout check in the loop.
    pub fn register(&mut self, adapter: std::sync::Arc<dyn AgentClientAdapter>) {
        self.adapters.push(adapter);
    }

    pub fn get(&self, adapter_id: &str) -> Option<&dyn AgentClientAdapter> {
        self.adapters
            .iter()
            .find(|a| a.id() == adapter_id)
            .map(|a| a.as_ref())
    }

    pub fn ids(&self) -> Vec<AdapterId> {
        self.adapters.iter().map(|a| a.id()).collect()
    }

    /// Detect every registered adapter, isolating failures per-adapter. Runs
    /// each `detect` on its own detached OS thread with a timeout: `detect`
    /// is specified as "cheap," but a hung network mount or a pathological
    /// client install must not be allowed to block every other adapter's
    /// detection, let alone native session loads that share the same async
    /// runtime.
    pub fn detect_all(&self) -> Vec<AdapterDetectionResult> {
        self.adapters
            .iter()
            .map(|adapter| AdapterDetectionResult {
                adapter_id: adapter.id().to_string(),
                display_name: adapter.display_name().to_string(),
                outcome: detect_with_isolation(adapter.clone()),
            })
            .collect()
    }
}

/// Run one adapter's `detect` with a timeout and a caught panic, both mapped
/// to [`DetectionOutcome::Failed`]. `AgentClientAdapter: Send + Sync` plus
/// the `'static` owned `Arc` is what makes handing this off to a genuinely
/// detached (non-scoped) thread sound: a *scoped* thread's join happens when
/// the scope exits regardless of any timeout check inside it, which was
/// tried first and defeats the whole point of a timeout -- a hung adapter
/// still blocked the caller for its full hang duration. A detached thread
/// has no such join point; a channel with a bounded `recv_timeout` is what
/// actually lets the caller stop waiting while the hung thread is abandoned
/// to finish (or never finish) entirely on its own.
fn detect_with_isolation(adapter: std::sync::Arc<dyn AgentClientAdapter>) -> DetectionOutcome {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| adapter.detect()));
        // The receiver may already be gone (timed out and dropped) -- that
        // is expected, not an error, since nothing is waiting to see this
        // result anymore.
        let _ = tx.send(result);
    });

    let result = rx.recv_timeout(DETECTION_TIMEOUT).ok();

    match result {
        Some(Ok(Ok(Some(detected)))) => DetectionOutcome::Detected {
            version: detected.version,
            supported: detected.supported,
        },
        Some(Ok(Ok(None))) => DetectionOutcome::NotDetected,
        Some(Ok(Err(e))) => DetectionOutcome::Failed {
            detail: e.to_string(),
        },
        Some(Err(panic)) => DetectionOutcome::Failed {
            detail: panic_message(&panic),
        },
        None => DetectionOutcome::Failed {
            detail: format!("timed out after {}ms", DETECTION_TIMEOUT.as_millis()),
        },
    }
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        format!("panicked: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("panicked: {s}")
    } else {
        "panicked with a non-string payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// One unreadable byte must not discard a whole conversation.
    ///
    /// Strict reading failed the entire file, while the listing path -- which
    /// reads the same file line by line -- skipped the bad line and carried
    /// on. So a conversation listed with a title and a message count and then
    /// refused to open, losing every message in it including the ones the
    /// list had just shown. A file another program is still writing produces
    /// exactly this, by way of a half-written character at the end.
    #[test]
    fn one_unreadable_byte_does_not_discard_the_readable_messages() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");
        let mut bytes = b"first line
".to_vec();
        bytes.push(0xFF); // not valid UTF-8 in any position
        bytes.extend_from_slice(b"
last line
");
        std::fs::write(&path, &bytes).unwrap();

        let text = read_foreign_file_to_string(&path).expect("a bad byte is not a failed read");
        assert!(text.contains("first line"), "{text}");
        assert!(text.contains("last line"), "the rest of the file was lost: {text}");
    }

    /// An ordinary file is unchanged by reading it this way.
    #[test]
    fn a_valid_file_reads_back_exactly() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("plain.jsonl");
        let original = "one
two
three
";
        std::fs::write(&path, original).unwrap();
        assert_eq!(read_foreign_file_to_string(&path).unwrap(), original);
    }

    struct AlwaysDetects;
    impl AgentClientAdapter for AlwaysDetects {
        fn id(&self) -> AdapterId {
            "always-detects"
        }
        fn display_name(&self) -> &'static str {
            "Always Detects"
        }
        fn supported_version_range(&self) -> &'static str {
            "*"
        }
        fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
            Ok(Some(DetectedClient {
                home_dir: std::path::PathBuf::from("/fake"),
                version: Some("1.0.0".into()),
                supported: true,
            }))
        }
        fn list_sessions(
            &self,
            _client: &DetectedClient,
        ) -> Result<Vec<ExternalSessionSummary>, AdapterError> {
            Ok(Vec::new())
        }
        fn read_session(
            &self,
            _client: &DetectedClient,
            external_session_id: &str,
        ) -> Result<ExternalSessionDetail, AdapterError> {
            Err(AdapterError::SessionNotFound {
                external_session_id: external_session_id.into(),
            })
        }
        fn continuation_capability(
            &self,
            _client: &DetectedClient,
            _external_session_id: &str,
        ) -> ContinuationCapability {
            ContinuationCapability::Unsupported
        }
    }

    struct AlwaysPanics;
    impl AgentClientAdapter for AlwaysPanics {
        fn id(&self) -> AdapterId {
            "always-panics"
        }
        fn display_name(&self) -> &'static str {
            "Always Panics"
        }
        fn supported_version_range(&self) -> &'static str {
            "*"
        }
        fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
            panic!("boom");
        }
        fn list_sessions(
            &self,
            _client: &DetectedClient,
        ) -> Result<Vec<ExternalSessionSummary>, AdapterError> {
            Ok(Vec::new())
        }
        fn read_session(
            &self,
            _client: &DetectedClient,
            external_session_id: &str,
        ) -> Result<ExternalSessionDetail, AdapterError> {
            Err(AdapterError::SessionNotFound {
                external_session_id: external_session_id.into(),
            })
        }
        fn continuation_capability(
            &self,
            _client: &DetectedClient,
            _external_session_id: &str,
        ) -> ContinuationCapability {
            ContinuationCapability::Unsupported
        }
    }

    struct AlwaysHangs;
    impl AgentClientAdapter for AlwaysHangs {
        fn id(&self) -> AdapterId {
            "always-hangs"
        }
        fn display_name(&self) -> &'static str {
            "Always Hangs"
        }
        fn supported_version_range(&self) -> &'static str {
            "*"
        }
        fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
            std::thread::sleep(Duration::from_secs(60));
            Ok(None)
        }
        fn list_sessions(
            &self,
            _client: &DetectedClient,
        ) -> Result<Vec<ExternalSessionSummary>, AdapterError> {
            Ok(Vec::new())
        }
        fn read_session(
            &self,
            _client: &DetectedClient,
            external_session_id: &str,
        ) -> Result<ExternalSessionDetail, AdapterError> {
            Err(AdapterError::SessionNotFound {
                external_session_id: external_session_id.into(),
            })
        }
        fn continuation_capability(
            &self,
            _client: &DetectedClient,
            _external_session_id: &str,
        ) -> ContinuationCapability {
            ContinuationCapability::Unsupported
        }
    }

    struct AlwaysErrors;
    impl AgentClientAdapter for AlwaysErrors {
        fn id(&self) -> AdapterId {
            "always-errors"
        }
        fn display_name(&self) -> &'static str {
            "Always Errors"
        }
        fn supported_version_range(&self) -> &'static str {
            "*"
        }
        fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
            Err(AdapterError::Io {
                detail: "disk exploded".into(),
            })
        }
        fn list_sessions(
            &self,
            _client: &DetectedClient,
        ) -> Result<Vec<ExternalSessionSummary>, AdapterError> {
            Ok(Vec::new())
        }
        fn read_session(
            &self,
            _client: &DetectedClient,
            external_session_id: &str,
        ) -> Result<ExternalSessionDetail, AdapterError> {
            Err(AdapterError::SessionNotFound {
                external_session_id: external_session_id.into(),
            })
        }
        fn continuation_capability(
            &self,
            _client: &DetectedClient,
            _external_session_id: &str,
        ) -> ContinuationCapability {
            ContinuationCapability::Unsupported
        }
    }

    #[test]
    fn a_healthy_adapter_detects_normally() {
        let mut registry = AdapterRegistry::new();
        registry.register(std::sync::Arc::new(AlwaysDetects));
        let results = registry.detect_all();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            results[0].outcome,
            DetectionOutcome::Detected { supported: true, .. }
        ));
    }

    #[test]
    fn a_panicking_adapter_becomes_a_failed_outcome_not_a_crash() {
        let mut registry = AdapterRegistry::new();
        registry.register(std::sync::Arc::new(AlwaysPanics));
        let results = registry.detect_all();
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].outcome, DetectionOutcome::Failed { .. }));
    }

    #[test]
    fn an_erroring_adapter_becomes_a_failed_outcome() {
        let mut registry = AdapterRegistry::new();
        registry.register(std::sync::Arc::new(AlwaysErrors));
        let results = registry.detect_all();
        assert!(matches!(results[0].outcome, DetectionOutcome::Failed { .. }));
    }

    /// The regression test for task 1.2 / spec "Adapters fail independently":
    /// one adapter that panics or hangs must not stop the others from
    /// reporting a real outcome in the same scan.
    #[test]
    fn one_broken_adapter_does_not_block_the_others_in_the_same_scan() {
        let mut registry = AdapterRegistry::new();
        registry.register(std::sync::Arc::new(AlwaysPanics));
        registry.register(std::sync::Arc::new(AlwaysDetects));
        registry.register(std::sync::Arc::new(AlwaysErrors));
        let results = registry.detect_all();
        assert_eq!(results.len(), 3);
        assert!(matches!(results[0].outcome, DetectionOutcome::Failed { .. }));
        assert!(matches!(
            results[1].outcome,
            DetectionOutcome::Detected { .. }
        ));
        assert!(matches!(results[2].outcome, DetectionOutcome::Failed { .. }));
    }

    #[test]
    fn a_hanging_adapter_times_out_rather_than_blocking_the_scan_forever() {
        let mut registry = AdapterRegistry::new();
        registry.register(std::sync::Arc::new(AlwaysHangs));
        registry.register(std::sync::Arc::new(AlwaysDetects));
        let started = Instant::now();
        let results = registry.detect_all();
        // Should not have waited anywhere near the hang's 60s sleep. Loose
        // on purpose: this only needs to prove the scan did not block for
        // the full hang duration, not pin an exact timing under a parallel
        // test run's scheduling noise.
        assert!(started.elapsed() < Duration::from_secs(30));
        assert!(matches!(results[0].outcome, DetectionOutcome::Failed { .. }));
        assert!(matches!(
            results[1].outcome,
            DetectionOutcome::Detected { .. }
        ));
    }

    #[test]
    fn registry_get_finds_a_registered_adapter_by_id() {
        let mut registry = AdapterRegistry::new();
        registry.register(std::sync::Arc::new(AlwaysDetects));
        assert!(registry.get("always-detects").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn default_registry_has_all_five_shipped_adapters() {
        let registry = AdapterRegistry::with_default_adapters();
        let ids = registry.ids();
        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&"codex"));
        assert!(ids.contains(&"claude-code"));
        assert!(ids.contains(&"opencode"));
        assert!(ids.contains(&"vscode-copilot"));
        assert!(ids.contains(&"openchamber"));
    }

    // -- 5.1: the no-writes proof --
    //
    // A behavioral spy (opening a real file and asserting no write occurred)
    // only proves one adapter, on one run, against one fixture. This proves
    // the *invariant* across all adapter source: every file under
    // `adapters/` (this module's siblings) is grepped for the write-capable
    // `OpenOptions` builder methods. `open_read_only`/`read_foreign_file_to_string`
    // above are the sanctioned exception (they exist precisely so adapter
    // code never needs its own `OpenOptions` at all), and are excluded from
    // the scan since they are demonstrably read-only by inspection right
    // here.
    #[test]
    fn no_adapter_source_opens_foreign_paths_for_writing() {
        let adapters_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agentdesk/adapters");
        let write_markers = [
            ".write(true)",
            ".append(true)",
            ".create(true)",
            ".create_new(true)",
            ".truncate(true)",
            "fs::write(",
            "fs::remove_file(",
            "fs::remove_dir",
            "fs::rename(",
            "fs::copy(",
            "File::create(",
        ];

        let mut offenders = Vec::new();
        visit_rs_files(&adapters_dir, &mut |path| {
            // This file defines `open_read_only`/`read_foreign_file_to_string`
            // themselves and documents why they are the sanctioned read-only
            // seam; skip it so the doc comment text above (which mentions
            // `.write(true)` etc. by name) is not mistaken for a violation.
            if path.file_name().and_then(|n| n.to_str()) == Some("mod.rs")
                && path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str())
                    == Some("adapters")
            {
                return;
            }
            let Ok(contents) = std::fs::read_to_string(path) else {
                return;
            };
            for marker in write_markers {
                if contents.contains(marker) {
                    offenders.push(format!("{}: contains {marker:?}", path.display()));
                }
            }
        });

        assert!(
            offenders.is_empty(),
            "adapter source must never open a foreign path for writing, found:\n{}",
            offenders.join("\n")
        );
    }

    fn visit_rs_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Fixtures are test data, not adapter logic, and are allowed
                // to contain arbitrary strings including the markers above.
                if path.file_name().and_then(|n| n.to_str()) == Some("fixtures") {
                    continue;
                }
                visit_rs_files(&path, visit);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                visit(&path);
            }
        }
    }

    #[test]
    fn redact_replaces_a_windows_drive_path() {
        let out = redact_for_log(r"could not open C:\Users\me\.codex\sessions\a.jsonl for reading");
        assert_eq!(out, "could not open <path> for reading");
    }

    #[test]
    fn redact_replaces_a_posix_path_and_keeps_surrounding_punctuation() {
        let out = redact_for_log("expected path is missing: \"/home/me/.claude/projects/x.jsonl\".");
        assert_eq!(out, "expected path is missing: \"<path>\".");
        assert_eq!(redact_for_log("~/.config/opencode/db"), "<path>");
        assert_eq!(redact_for_log(r"\\server\share\file"), "<path>");
    }

    #[test]
    fn redact_replaces_a_long_message_body_with_a_length_marker() {
        let body = "Here is the whole conversation the user had about their private project, \
                    repeated so that it is comfortably longer than the cap allows for logging.";
        let out = redact_for_log(body);
        assert!(out.starts_with("<redacted "), "got {out}");
        assert!(out.ends_with(" chars>"));
        assert!(!out.contains("private project"));
    }

    #[test]
    fn redact_passes_a_short_adapter_id_and_outcome_through() {
        assert_eq!(redact_for_log("codex"), "codex");
        assert_eq!(
            redact_for_log("client not detected on this machine"),
            "client not detected on this machine"
        );
        // "and/or" and a lone slash are words, not paths.
        assert_eq!(redact_for_log("either and/or / neither"), "either and/or / neither");
    }
}
