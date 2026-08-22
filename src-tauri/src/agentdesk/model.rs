//! The durable Agent Desk session shape: what gets written to disk.
//!
//! A session is a backend-owned record, not frontend state. Every type here is
//! `Serialize + Deserialize + specta::Type` so it round-trips through JSON on
//! disk and through the generated bindings to the UI unchanged.
//!
//! Source provenance is an exhaustive tagged enum on purpose (see
//! [`SessionSource`]): a bag of optional fields would let a new source type be
//! added without anyone being forced to decide how it refreshes, displays, and
//! kicks off a run.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The current on-disk shape. Bump when a breaking change to any type in this
/// module ships, and extend [`migrate_header`]/[`migrate_session`] to carry
/// old files forward rather than discarding them.
pub const CURRENT_SCHEMA_VERSION: u16 = 1;

pub type SessionId = String;
pub type MessageId = String;
pub type SegmentId = String;
pub type ExecutionId = String;

/// The compact, list-friendly projection of a session.
///
/// This is what `index.json` stores: enough to render a sidebar row and
/// resolve filters without reading every session file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionHeader {
    pub schema_version: u16,
    pub session_id: SessionId,
    pub repo_id: String,
    pub repo_path: String,
    pub repo_name: String,
    pub title: String,
    pub source: SessionSource,
    pub intent: SessionIntent,
    pub state: SessionState,
    /// RFC 3339 UTC timestamp.
    pub created_at: String,
    /// RFC 3339 UTC timestamp.
    pub updated_at: String,
    pub unread: bool,
    pub changed_file_count: u32,
    pub active_execution_id: Option<ExecutionId>,
    pub archived: bool,
    /// RFC 3339 UTC timestamp of the moment this session's write authority
    /// was durably granted -- the user pressing Start on a Plan proposal
    /// (`agent_session_start_graph`), or "Use solo instead" abandoning the
    /// proposal in favor of an ordinary run. `None` for a session that has
    /// never been started this way: a fresh session of any intent (Fix
    /// included -- it needs no separate Start, its intent policy already
    /// grants write authority from creation) or a Plan session still sitting
    /// on an unactioned proposal.
    ///
    /// This is the fix for "Plan can write before Start": before this field
    /// existed, `start_execution_at` passed `started: true` into every
    /// `cli_run::run_task` call unconditionally, including the very first
    /// Plan-mode turn that PRODUCES the proposal -- so a proposal turn had
    /// the same write authority as a turn that ran after the user actually
    /// accepted the graph. `started_for_execution` (in
    /// `commands::agent_desk`) reads this field, not any in-memory or
    /// graph-shape signal, so the answer survives a restart and is not
    /// re-derivable from "does an executed helper exist yet" (a Plan session
    /// with a proposal that has zero ready-now helpers would otherwise look
    /// identical to one that never proposed at all).
    #[serde(default)]
    pub graph_started_at: Option<String>,
}

/// The full session file on disk: the header plus everything the transcript,
/// executions list, and context panel need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub header: AgentSessionHeader,
    pub segments: Vec<ConversationSegment>,
    pub messages: Vec<SessionMessage>,
    pub executions: Vec<ExecutionRecord>,
    pub attachments: Vec<ContextAttachment>,
}

impl AgentSession {
    /// A brand-new, empty session for `header`. Callers append the opening
    /// message and any executions afterward.
    pub fn new(header: AgentSessionHeader) -> Self {
        Self {
            header,
            segments: Vec::new(),
            messages: Vec::new(),
            executions: Vec::new(),
            attachments: Vec::new(),
        }
    }
}

/// What started a session, and what to show if the live thing it points to
/// disappears.
///
/// Every variant carries a cached snapshot captured at launch time. Refreshing
/// a session updates the live locator's data but must never overwrite the
/// snapshot -- that is the only honest record of what the user actually
/// clicked, and it is what the source banner falls back to when the live
/// source is gone (see spec `Deleted issue` scenario).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionSource {
    #[serde(rename_all = "camelCase")]
    Manual {
        repo_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Issue {
        host_id: String,
        owner: String,
        repo: String,
        number: u32,
        url: String,
        snapshot: SourceSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    PullRequest {
        host_id: String,
        owner: String,
        repo: String,
        number: u32,
        url: String,
        head: String,
        base: String,
        snapshot: SourceSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    OpenSpecChange {
        change_id: String,
        snapshot: SourceSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    OpenSpecTask {
        change_id: String,
        task_index: u32,
        task_text: String,
        snapshot: SourceSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    Commit {
        oid: String,
        snapshot: SourceSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    Diff {
        scope: String,
        paths: Vec<String>,
        snapshot: SourceSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    WorkingChanges {
        paths: Vec<String>,
        snapshot: SourceSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    CheckFailure {
        provider: String,
        check_id: String,
        url: Option<String>,
        snapshot: SourceSnapshot,
    },
}

impl SessionSource {
    /// A stable label for logging and tests. Never shown to the user as-is --
    /// UI copy is built per variant, not derived from this.
    pub fn kind_label(&self) -> &'static str {
        match self {
            SessionSource::Manual { .. } => "manual",
            SessionSource::Issue { .. } => "issue",
            SessionSource::PullRequest { .. } => "pullRequest",
            SessionSource::OpenSpecChange { .. } => "openSpecChange",
            SessionSource::OpenSpecTask { .. } => "openSpecTask",
            SessionSource::Commit { .. } => "commit",
            SessionSource::Diff { .. } => "diff",
            SessionSource::WorkingChanges { .. } => "workingChanges",
            SessionSource::CheckFailure { .. } => "checkFailure",
        }
    }

    /// A stable key identifying *what real-world thing* this source points
    /// at -- deliberately excludes the snapshot (title/body/captured time),
    /// which changes on every refresh and would otherwise make two sessions
    /// for the same issue look like different sources. Used by
    /// `agent_desk::find_active_session_for_source` (task 1.4) to detect
    /// "the user is starting the same source/intent a second time" without
    /// the store layer knowing anything about issues, PRs, or OpenSpec.
    ///
    /// Two sources with equal `identity_key()` and equal `SessionIntent` are
    /// the same real-world request -- re-clicking Fix on the same issue
    /// should focus the existing session, not fork a second one (design.md
    /// "Deduplication").
    pub fn identity_key(&self) -> String {
        match self {
            SessionSource::Manual { repo_id } => format!("manual:{repo_id}"),
            SessionSource::Issue {
                host_id,
                owner,
                repo,
                number,
                ..
            } => format!("issue:{host_id}:{owner}/{repo}#{number}"),
            SessionSource::PullRequest {
                host_id,
                owner,
                repo,
                number,
                ..
            } => format!("pullRequest:{host_id}:{owner}/{repo}#{number}"),
            SessionSource::OpenSpecChange { change_id, .. } => {
                format!("openSpecChange:{change_id}")
            }
            SessionSource::OpenSpecTask {
                change_id,
                task_index,
                ..
            } => format!("openSpecTask:{change_id}#{task_index}"),
            SessionSource::Commit { oid, .. } => format!("commit:{oid}"),
            SessionSource::Diff { scope, paths, .. } => {
                let mut sorted = paths.clone();
                sorted.sort();
                format!("diff:{scope}:{}", sorted.join(","))
            }
            SessionSource::WorkingChanges { paths, .. } => {
                let mut sorted = paths.clone();
                sorted.sort();
                format!("workingChanges:{}", sorted.join(","))
            }
            SessionSource::CheckFailure {
                provider, check_id, ..
            } => format!("checkFailure:{provider}:{check_id}"),
        }
    }
}

/// The cached title/body captured when a session was created from a source.
/// Never mutated by a refresh -- refreshes update the live-availability flag
/// on the surrounding variant's own fields, not this snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub title: String,
    pub summary: String,
    /// RFC 3339 UTC timestamp of when the snapshot was captured.
    pub captured_at: String,
    /// Set once a refresh finds the live source cannot be loaded. The snapshot
    /// itself is left untouched so the banner can keep showing it.
    pub live_unavailable: bool,
}

/// What the user was trying to do when the session started. Drives the
/// default mode/team/write/worktree policy in `src-tauri/src/agentdesk/policy.rs`
/// (a later task), not persisted behavior here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SessionIntent {
    Ask,
    Explain,
    Plan,
    Fix,
    Review,
    Summarize,
}

/// Where a session is. Distinct from `airun::RunState`: a session outlives any
/// single execution and has states -- `Draft`, `Ready`, `MissingSource` -- that
/// have no execution running at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    /// Created but no message sent yet.
    Draft,
    /// Kickoff accepted; provider/policy resolution has not finished.
    Preparing,
    /// Has a plan or transcript, no execution currently running.
    Ready,
    /// An execution is actively running.
    Working,
    /// Paused at a gate or awaiting a plan-mode start decision.
    NeedsInput,
    Finished,
    Failed,
    Stopped,
    /// The source this session depends on could not be loaded at all (never
    /// loaded successfully, as opposed to a snapshot's `live_unavailable`
    /// flag, which means it loaded once and later disappeared).
    MissingSource,
    /// This session (or one of its executions) was found on load claiming
    /// `Preparing`/`Working`/`NeedsInput` with no live process behind it --
    /// the process that owned the run is gone (crash, force-quit, power
    /// loss, or an app update mid-run), so the state on disk outlived it.
    /// Distinct from `Failed`/`Stopped`: neither of those is true here --
    /// nothing ever decided the run failed or was deliberately stopped, it
    /// just never got to say anything at all. Starting a new execution on
    /// this session (`agent_session_start_execution`) is exactly the
    /// existing recovery path once this state is set, since it is what
    /// unblocks `record_execution_if_not_running`'s "already running" guard.
    /// See `agentdesk::session_recovery`.
    Interrupted,
}

/// A grouping boundary within a session's transcript, e.g. across a
/// plan-then-execute split or a lead/helper divide. Kept minimal in this
/// change; the conversation shell package extends how segments render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSegment {
    pub segment_id: SegmentId,
    pub label: String,
    /// RFC 3339 UTC timestamp.
    pub started_at: String,
}

/// One entry in the transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub message_id: MessageId,
    pub segment_id: SegmentId,
    pub role: MessageRole,
    /// RFC 3339 UTC timestamp.
    pub timestamp: String,
    pub plain_content: String,
    pub rendered_content: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub kind: MessageKind,
    pub execution_id: Option<ExecutionId>,
    /// Position within that execution's ordered event stream. `None` for
    /// messages not produced by an execution (user messages, system notes).
    pub sequence: Option<u32>,
    pub import: Option<ImportProvenance>,
    pub targets: Vec<MessageTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// What kind of content a message carries. Separate from `role`: an assistant
/// role can produce a thought summary, a tool call, or a final result, and the
/// UI renders each differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MessageKind {
    User,
    Assistant,
    ThoughtSummary,
    Tool,
    Approval,
    System,
    Result,
}

/// A link from a message to something else in the app: a file, a diff, the
/// session's own source, a graph node, or an OpenSpec task. Exhaustive so a
/// message never carries a target the UI has no renderer for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MessageTarget {
    #[serde(rename_all = "camelCase")]
    File { path: String },
    #[serde(rename_all = "camelCase")]
    Diff { scope: String },
    Source,
    #[serde(rename_all = "camelCase")]
    GraphNode { execution_id: ExecutionId },
    #[serde(rename_all = "camelCase")]
    OpenSpecTask { change_id: String, task_index: u32 },
}

/// Where an imported message actually came from. Present only on messages
/// brought in from an external client adapter (a later change); native
/// messages leave this `None`. Its presence is what stops an imported message
/// from ever being presented as native output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportProvenance {
    pub adapter_id: String,
    pub external_session_id: String,
    pub external_message_id: String,
    /// RFC 3339 UTC timestamp of the import operation itself, distinct from
    /// the message's own `timestamp`.
    pub imported_at: String,
}

/// One run attached to a session. A session can accumulate more than one
/// execution over its lifetime (retries, follow-ups, helper runs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub execution_id: ExecutionId,
    pub session_id: SessionId,
    /// `None` for the lead; `Some(execution_id)` of the lead for a helper.
    pub parent_execution_id: Option<ExecutionId>,
    pub state: SessionState,
    /// RFC 3339 UTC timestamp.
    pub started_at: String,
    /// RFC 3339 UTC timestamp. `None` while still running.
    pub ended_at: Option<String>,
    /// Hash of the OpenSpec plan text this execution was actually given
    /// (`openspec_context::fingerprint`), or `None` for a session that did not
    /// start from OpenSpec. Lets a later reader tell whether the plan has moved
    /// since the agent read it, rather than guessing from timestamps (R5.3).
    #[serde(default)]
    pub context_fingerprint: Option<String>,
    /// Highest sequence number persisted for this execution so far. Lets a
    /// late/duplicate event be recognized without rescanning `messages`.
    pub last_sequence: u32,
    /// Short label for this node's job -- what the inspector shows as its
    /// title (mockup `.ag-inspector-title` / `.ag-node-title`). `None` for the
    /// lead, whose title the UI derives from the session itself.
    #[serde(default)]
    pub job_title: Option<String>,
    /// One or two sentences describing what this node is doing, shown in the
    /// inspector card (mockup `.ag-inspector-copy`).
    #[serde(default)]
    pub job_description: Option<String>,
    /// `researcher | builder | verifier`, shown in the node's meta line
    /// (mockup: "Luna · researcher · read-only"). `None` for the lead.
    #[serde(default)]
    pub helper_role: Option<String>,
    /// Repo-relative path globs this node may write to. Empty for read-only
    /// nodes and for the lead (whose allowance is the whole repository).
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Absolute path of the isolated worktree this helper runs in. `None` for
    /// the lead (which runs against the session's own source) and for
    /// read-only helpers that never provision one.
    #[serde(default)]
    pub worktree_path: Option<String>,
    /// Branch checked out in `worktree_path`.
    #[serde(default)]
    pub branch: Option<String>,
    /// The commit `worktree_path`'s branch was created from -- R3.5:
    /// "Persist worktree path, branch, base revision, provider, mode, and
    /// policy on execution," so a restart (or a much later review) can tell
    /// what this execution actually ran against without re-deriving it from
    /// a worktree folder that may since have been cleaned up.
    #[serde(default)]
    pub base_oid: Option<String>,
    /// Which provider transport ran this execution --
    /// `agentdesk::policy::ExecutionProvider`'s name (e.g. `"copilot"`), not
    /// the enum itself: that type is `Copy`/non-`Type` (never crosses the
    /// IPC boundary), and this module has no dependency on `policy` today.
    /// Stored as the plain string a provider override already travels as
    /// everywhere else in this package (`StartAgentSessionRequest.provider_override`,
    /// `ResultRecord`'s commit-message drafting).
    #[serde(default)]
    pub provider: Option<String>,
    /// `agentdesk::policy::ExecutionMode` as its serialized string (`"ask" |
    /// "plan" | "auto"`) -- what this execution actually ran under, which
    /// may differ from the intent's default when a caller passed an
    /// explicit mode (`ExecutionPolicy::resolve`'s `mode` parameter).
    #[serde(default)]
    pub mode: Option<String>,
    /// `agentdesk::policy::ExecutionTeam` as its serialized string (`"solo"
    /// | "lead"`) -- same reasoning as `mode`.
    #[serde(default)]
    pub team: Option<String>,
    /// Other execution IDs (within the same session) this node depends on --
    /// it will not be scheduled until all of them reach `Finished`.
    #[serde(default)]
    pub depends_on: Vec<ExecutionId>,
    /// How many files this node changed, for the inspector's files line.
    #[serde(default)]
    pub changed_file_count: u32,
    /// One-line summary of what the node produced, once finished.
    #[serde(default)]
    pub output_summary: Option<String>,
    /// Present only on a lead execution that has drafted a graph and is
    /// waiting for the user's Start/Revise/Use-solo decision (`state` is
    /// `NeedsInput` while this is set -- "Paused at a gate or awaiting a
    /// plan-mode start decision", `SessionState::NeedsInput`'s own doc
    /// comment). Cleared once Start is chosen.
    #[serde(default)]
    pub proposed_graph: Option<crate::agentdesk::graph::ProposedGraph>,
    /// Present only while this node's result integration hit a conflict
    /// (tasks.md 5.3). Cleared once a person resolves it.
    #[serde(default)]
    pub conflict: Option<crate::agentdesk::graph::IntegrationConflict>,
    /// R6.4: the turn/time budget this execution must actually stop at, once
    /// it is running -- copied from the proposed helper job's own
    /// `JobBudget` when the graph starts (`commit_started_graph_if_still_proposed`
    /// in `commands::agent_graph`), so `cli_run::run_task` can enforce it
    /// without threading `ProposedGraph` through the whole launch path.
    /// `None` for the lead (no proposed budget applies to it) and for any
    /// helper launched before this field existed.
    #[serde(default)]
    pub budget: Option<crate::agentdesk::graph::JobBudget>,
    /// Present only on the LEAD's own execution record: the absolute path of
    /// the dedicated worktree helper results are integrated into. Provisioned
    /// lazily (`commands::agent_graph::ensure_integration_worktree`) the first
    /// time any helper finishes, from the lead's own current HEAD in the
    /// session's real repository -- never `session.header.repo_path` itself,
    /// which is the user's own open checkout. `None` for a helper record, and
    /// for a lead that has not yet had a helper finish.
    #[serde(default)]
    pub integration_worktree_path: Option<String>,
}

impl ExecutionRecord {
    /// A bare-minimum record with every graph-only field left empty --
    /// existing call sites (single-execution sessions predating this change)
    /// use this so they do not have to spell out every new field by hand.
    pub fn minimal(
        execution_id: ExecutionId,
        session_id: SessionId,
        parent_execution_id: Option<ExecutionId>,
        state: SessionState,
        started_at: String,
        ended_at: Option<String>,
        last_sequence: u32,
    ) -> Self {
        Self {
            execution_id,
            session_id,
            parent_execution_id,
            state,
            started_at,
            ended_at,
            last_sequence,
            context_fingerprint: None,
            job_title: None,
            job_description: None,
            helper_role: None,
            allowed_paths: Vec::new(),
            worktree_path: None,
            branch: None,
            base_oid: None,
            provider: None,
            mode: None,
            team: None,
            depends_on: Vec::new(),
            changed_file_count: 0,
            output_summary: None,
            proposed_graph: None,
            conflict: None,
            budget: None,
            integration_worktree_path: None,
        }
    }
}

/// Something attached to a session's context beyond the messages themselves:
/// a pinned file, a pasted note, a linked OpenSpec task. Kept intentionally
/// small in this change -- the context panel package defines richer variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContextAttachment {
    pub attachment_id: String,
    pub label: String,
    pub target: MessageTarget,
    /// RFC 3339 UTC timestamp.
    pub added_at: String,
}

/// Why a session file on disk could not be turned into a usable
/// [`AgentSession`]. Carried by the store so a bad file can be logged and
/// skipped without losing every other session (spec: "One damaged session").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionLoadError {
    /// The file's `schemaVersion` is newer than this build understands, so
    /// migrating it forward would guess at fields that do not exist yet.
    UnsupportedSchemaVersion { found: u16, max_supported: u16 },
    /// The file is not valid JSON, or its shape does not match any schema
    /// version this build knows how to read.
    Malformed { detail: String },
    /// No file exists at the path this session ID resolves to -- confirmed by
    /// `std::io::ErrorKind::NotFound`, not inferred from "some I/O error
    /// happened." A session that was never written, or whose ID is wrong,
    /// looks like this.
    NotFound,
    /// The file could not be read for some *other* reason: permission
    /// denied, a lock held by another process (common on Windows), a
    /// transient disk error. The file may well exist and be fine -- this is
    /// not evidence the session was deleted, unlike [`Self::NotFound`], so
    /// callers must not treat it the same way.
    Io { detail: String },
}

/// Brings a raw parsed session value up to [`CURRENT_SCHEMA_VERSION`], or
/// reports why it cannot.
///
/// v1 has no predecessor, so today this only validates the version field and
/// returns the session unchanged. It exists as a real function (not a no-op
/// left for later) so the very first schema bump has one seam to extend
/// instead of a decision about where migration code should even live.
pub fn migrate_session(raw: serde_json::Value) -> Result<AgentSession, SessionLoadError> {
    let found_version = raw
        .get("header")
        .and_then(|h| h.get("schemaVersion"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u16);

    match found_version {
        Some(v) if v > CURRENT_SCHEMA_VERSION => Err(SessionLoadError::UnsupportedSchemaVersion {
            found: v,
            max_supported: CURRENT_SCHEMA_VERSION,
        }),
        Some(1) => serde_json::from_value(raw).map_err(|e| SessionLoadError::Malformed {
            detail: e.to_string(),
        }),
        // No known migration path from an older version yet; v1 is the floor.
        Some(v) => Err(SessionLoadError::UnsupportedSchemaVersion {
            found: v,
            max_supported: CURRENT_SCHEMA_VERSION,
        }),
        None => Err(SessionLoadError::Malformed {
            detail: "missing header.schemaVersion".into(),
        }),
    }
}

/// The header-only equivalent of [`migrate_session`], used when rebuilding
/// the index from `header` alone without parsing the rest of the file.
pub fn migrate_header(raw: serde_json::Value) -> Result<AgentSessionHeader, SessionLoadError> {
    let found_version = raw
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .map(|v| v as u16);

    match found_version {
        Some(v) if v > CURRENT_SCHEMA_VERSION => Err(SessionLoadError::UnsupportedSchemaVersion {
            found: v,
            max_supported: CURRENT_SCHEMA_VERSION,
        }),
        Some(1) => serde_json::from_value(raw).map_err(|e| SessionLoadError::Malformed {
            detail: e.to_string(),
        }),
        Some(v) => Err(SessionLoadError::UnsupportedSchemaVersion {
            found: v,
            max_supported: CURRENT_SCHEMA_VERSION,
        }),
        None => Err(SessionLoadError::Malformed {
            detail: "missing schemaVersion".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(title: &str) -> SourceSnapshot {
        SourceSnapshot {
            title: title.into(),
            summary: "summary text".into(),
            captured_at: "2026-01-01T00:00:00Z".into(),
            live_unavailable: false,
        }
    }

    fn header(source: SessionSource) -> AgentSessionHeader {
        AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: "sess-1".into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/proj".into(),
            repo_name: "proj".into(),
            title: "A session".into(),
            source,
            intent: SessionIntent::Fix,
            state: SessionState::Draft,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            unread: false,
            changed_file_count: 0,
            active_execution_id: None,
            archived: false,
            graph_started_at: None,
        }
    }

    /// Every `SessionSource` variant, one of each. If a variant is added to
    /// the enum without a fixture here, this list -- not the round-trip test
    /// below -- is where to add it.
    fn all_source_variants() -> Vec<SessionSource> {
        vec![
            SessionSource::Manual {
                repo_id: "repo-1".into(),
            },
            SessionSource::Issue {
                host_id: "github".into(),
                owner: "acme".into(),
                repo: "widgets".into(),
                number: 42,
                url: "https://example.test/issues/42".into(),
                snapshot: snapshot("Issue title"),
            },
            SessionSource::PullRequest {
                host_id: "github".into(),
                owner: "acme".into(),
                repo: "widgets".into(),
                number: 7,
                url: "https://example.test/pull/7".into(),
                head: "feature".into(),
                base: "main".into(),
                snapshot: snapshot("PR title"),
            },
            SessionSource::OpenSpecChange {
                change_id: "add-thing".into(),
                snapshot: snapshot("Add thing"),
            },
            SessionSource::OpenSpecTask {
                change_id: "add-thing".into(),
                task_index: 3,
                task_text: "Wire the command".into(),
                snapshot: snapshot("Task 3"),
            },
            SessionSource::Commit {
                oid: "abc123".into(),
                snapshot: snapshot("Commit message"),
            },
            SessionSource::Diff {
                scope: "working".into(),
                paths: vec!["src/a.rs".into(), "src/b.rs".into()],
                snapshot: snapshot("Diff of 2 files"),
            },
            SessionSource::WorkingChanges {
                paths: vec!["src/a.rs".into()],
                snapshot: snapshot("Working changes"),
            },
            SessionSource::CheckFailure {
                provider: "ci".into(),
                check_id: "build".into(),
                url: Some("https://example.test/checks/1".into()),
                snapshot: snapshot("Build failed"),
            },
        ]
    }

    /// Every `MessageKind`, one of each.
    fn all_message_kinds() -> Vec<MessageKind> {
        vec![
            MessageKind::User,
            MessageKind::Assistant,
            MessageKind::ThoughtSummary,
            MessageKind::Tool,
            MessageKind::Approval,
            MessageKind::System,
            MessageKind::Result,
        ]
    }

    fn message(kind: MessageKind) -> SessionMessage {
        SessionMessage {
            message_id: "msg-1".into(),
            segment_id: "seg-1".into(),
            role: MessageRole::Assistant,
            timestamp: "2026-01-01T00:00:01Z".into(),
            plain_content: "hello".into(),
            rendered_content: Some("<p>hello</p>".into()),
            provider: Some("anthropic".into()),
            model: Some("claude".into()),
            kind,
            execution_id: Some("exec-1".into()),
            sequence: Some(1),
            import: None,
            targets: vec![
                MessageTarget::File {
                    path: "src/a.rs".into(),
                },
                MessageTarget::Diff {
                    scope: "working".into(),
                },
                MessageTarget::Source,
                MessageTarget::GraphNode {
                    execution_id: "exec-2".into(),
                },
                MessageTarget::OpenSpecTask {
                    change_id: "add-thing".into(),
                    task_index: 1,
                },
            ],
        }
    }

    #[test]
    fn every_source_variant_round_trips_through_json() {
        for source in all_source_variants() {
            let h = header(source.clone());
            let json = serde_json::to_string(&h).expect("header serializes");
            let back: AgentSessionHeader =
                serde_json::from_str(&json).expect("header deserializes");
            assert_eq!(back.source, source, "round trip changed the source");
        }
    }

    #[test]
    fn every_message_kind_round_trips_through_json() {
        for kind in all_message_kinds() {
            let m = message(kind);
            let json = serde_json::to_string(&m).expect("message serializes");
            let back: SessionMessage = serde_json::from_str(&json).expect("message deserializes");
            assert_eq!(back, m, "round trip changed the message");
        }
    }

    #[test]
    fn a_full_session_round_trips_through_json() {
        for source in all_source_variants() {
            let mut session = AgentSession::new(header(source));
            for kind in all_message_kinds() {
                session.messages.push(message(kind));
            }
            session.segments.push(ConversationSegment {
                segment_id: "seg-1".into(),
                label: "Plan".into(),
                started_at: "2026-01-01T00:00:00Z".into(),
            });
            session.executions.push(ExecutionRecord::minimal(
                "exec-1".into(),
                session.header.session_id.clone(),
                None,
                SessionState::Working,
                "2026-01-01T00:00:00Z".into(),
                None,
                5,
            ));
            session.attachments.push(ContextAttachment {
                attachment_id: "att-1".into(),
                label: "a.rs".into(),
                target: MessageTarget::File {
                    path: "src/a.rs".into(),
                },
                added_at: "2026-01-01T00:00:00Z".into(),
            });

            let json = serde_json::to_string_pretty(&session).expect("session serializes");
            let back: AgentSession = serde_json::from_str(&json).expect("session deserializes");
            assert_eq!(back, session, "round trip changed the session");
        }
    }

    #[test]
    fn an_imported_message_carries_its_provenance_through_json() {
        let mut m = message(MessageKind::Assistant);
        m.import = Some(ImportProvenance {
            adapter_id: "codex".into(),
            external_session_id: "ext-1".into(),
            external_message_id: "ext-msg-1".into(),
            imported_at: "2026-01-02T00:00:00Z".into(),
        });
        let json = serde_json::to_string(&m).unwrap();
        let back: SessionMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.import, m.import);
        assert!(
            json.contains("\"adapterId\""),
            "provenance must be reachable from the raw JSON, not just the struct"
        );
    }

    #[test]
    fn every_session_state_round_trips() {
        let states = [
            SessionState::Draft,
            SessionState::Preparing,
            SessionState::Ready,
            SessionState::Working,
            SessionState::NeedsInput,
            SessionState::Finished,
            SessionState::Failed,
            SessionState::Stopped,
            SessionState::MissingSource,
            SessionState::Interrupted,
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let back: SessionState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }

    #[test]
    fn every_session_intent_round_trips() {
        let intents = [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Plan,
            SessionIntent::Fix,
            SessionIntent::Review,
            SessionIntent::Summarize,
        ];
        for intent in intents {
            let json = serde_json::to_string(&intent).unwrap();
            let back: SessionIntent = serde_json::from_str(&json).unwrap();
            assert_eq!(back, intent);
        }
    }

    // -- Unknown-variant / forward-compatibility fixtures (task 1.5) --
    //
    // These prove that a session file written by a *future* build (an enum
    // tag this build has never heard of, or a schema version ahead of what
    // this build supports) fails in a specific, intentional way rather than
    // silently defaulting a field or panicking.

    #[test]
    fn an_unknown_source_kind_tag_fails_deserialization_rather_than_defaulting() {
        let raw = serde_json::json!({
            "kind": "somethingFromTheFuture",
            "someField": "value",
        });
        let result: Result<SessionSource, _> = serde_json::from_value(raw);
        assert!(
            result.is_err(),
            "an unrecognized source tag must be a hard error, not silently coerced"
        );
    }

    #[test]
    fn an_unknown_message_kind_tag_fails_deserialization() {
        let raw = serde_json::Value::String("somethingFromTheFuture".into());
        let result: Result<MessageKind, _> = serde_json::from_value(raw);
        assert!(result.is_err());
    }

    #[test]
    fn an_unknown_session_state_fails_deserialization() {
        let raw = serde_json::Value::String("somethingFromTheFuture".into());
        let result: Result<SessionState, _> = serde_json::from_value(raw);
        assert!(result.is_err());
    }

    #[test]
    fn migrate_session_accepts_the_current_schema_version() {
        let session = AgentSession::new(header(SessionSource::Manual {
            repo_id: "repo-1".into(),
        }));
        let raw = serde_json::to_value(&session).unwrap();
        let migrated = migrate_session(raw).expect("current version must migrate cleanly");
        assert_eq!(migrated, session);
    }

    #[test]
    fn migrate_session_rejects_a_schema_version_from_the_future() {
        let session = AgentSession::new(header(SessionSource::Manual {
            repo_id: "repo-1".into(),
        }));
        let mut raw = serde_json::to_value(&session).unwrap();
        raw["header"]["schemaVersion"] = serde_json::json!(CURRENT_SCHEMA_VERSION + 1);

        let result = migrate_session(raw);
        assert!(
            matches!(
                result,
                Err(SessionLoadError::UnsupportedSchemaVersion { .. })
            ),
            "a session written by a newer build must be refused, not guessed at: {result:?}"
        );
    }

    #[test]
    fn migrate_session_rejects_missing_schema_version() {
        let raw = serde_json::json!({ "header": {}, "segments": [], "messages": [], "executions": [], "attachments": [] });
        let result = migrate_session(raw);
        assert!(matches!(result, Err(SessionLoadError::Malformed { .. })));
    }

    #[test]
    fn migrate_session_rejects_malformed_json_shape_at_the_current_version() {
        // Valid schemaVersion, but the rest of the header is missing required
        // fields -- must fail as Malformed, not panic.
        let raw = serde_json::json!({
            "header": { "schemaVersion": CURRENT_SCHEMA_VERSION, "sessionId": "only-this" },
            "segments": [],
            "messages": [],
            "executions": [],
            "attachments": []
        });
        let result = migrate_session(raw);
        assert!(matches!(result, Err(SessionLoadError::Malformed { .. })));
    }

    #[test]
    fn migrate_header_mirrors_migrate_session_for_the_index() {
        let h = header(SessionSource::Manual {
            repo_id: "repo-1".into(),
        });
        let raw = serde_json::to_value(&h).unwrap();
        let migrated = migrate_header(raw).expect("current version must migrate cleanly");
        assert_eq!(migrated, h);
    }

    #[test]
    fn migrate_header_rejects_a_schema_version_from_the_future() {
        let h = header(SessionSource::Manual {
            repo_id: "repo-1".into(),
        });
        let mut raw = serde_json::to_value(&h).unwrap();
        raw["schemaVersion"] = serde_json::json!(CURRENT_SCHEMA_VERSION + 1);
        let result = migrate_header(raw);
        assert!(matches!(
            result,
            Err(SessionLoadError::UnsupportedSchemaVersion { .. })
        ));
    }

    #[test]
    fn an_unknown_field_on_an_otherwise_valid_message_is_ignored_not_fatal() {
        // Forward compatibility the other direction: an *older* build reading
        // a file written by a newer one, where only new optional-ish fields
        // were added at the same schema version, should not choke on fields
        // it does not recognize as long as everything it requires is present.
        let m = message(MessageKind::Assistant);
        let mut raw = serde_json::to_value(&m).unwrap();
        raw["fromTheFuture"] = serde_json::json!("unrecognized but harmless");
        let back: SessionMessage = serde_json::from_value(raw).expect(
            "an unknown *field* at the same schema version must not break deserialization",
        );
        assert_eq!(back, m);
    }

    #[test]
    fn session_source_struct_like_variant_fields_are_camel_case() {
        let source = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 42,
            url: "https://example.com/issues/42".into(),
            snapshot: SourceSnapshot {
                title: "Title".into(),
                summary: "Summary".into(),
                captured_at: "2026-01-01T00:00:00Z".into(),
                live_unavailable: false,
            },
        };
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("\"hostId\""), "got: {json}");
        assert!(!json.contains("host_id"), "got: {json}");
        assert!(json.contains("\"capturedAt\""), "got: {json}");
        assert!(!json.contains("captured_at"), "got: {json}");

        let back: SessionSource = serde_json::from_str(&json).unwrap();
        assert_eq!(back, source, "round trip changed the value");
    }

    #[test]
    fn message_target_struct_like_variant_fields_are_camel_case() {
        let target = MessageTarget::OpenSpecTask {
            change_id: "change-1".into(),
            task_index: 3,
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("\"changeId\""), "got: {json}");
        assert!(!json.contains("change_id"), "got: {json}");
        assert!(json.contains("\"taskIndex\""), "got: {json}");
        assert!(!json.contains("task_index"), "got: {json}");

        let back: MessageTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(back, target, "round trip changed the value");
    }

    // -- SessionSource::identity_key (agent-desk-source-kickoffs task 1.4) --

    #[test]
    fn identity_key_ignores_the_snapshot() {
        // Two sources pointing at the same issue but with different cached
        // snapshots (as happens after a refresh finds a new title) must
        // still be recognized as the same real-world source.
        let a = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 42,
            url: "https://example.test/issues/42".into(),
            snapshot: snapshot("Old title"),
        };
        let b = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 42,
            url: "https://example.test/issues/42".into(),
            snapshot: snapshot("New title after refresh"),
        };
        assert_eq!(a.identity_key(), b.identity_key());
    }

    #[test]
    fn identity_key_distinguishes_different_issues() {
        let issue_42 = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 42,
            url: "https://example.test/issues/42".into(),
            snapshot: snapshot("Title"),
        };
        let issue_43 = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 43,
            url: "https://example.test/issues/43".into(),
            snapshot: snapshot("Title"),
        };
        assert_ne!(issue_42.identity_key(), issue_43.identity_key());
    }

    #[test]
    fn identity_key_distinguishes_issue_from_pull_request_with_same_number() {
        // An issue and a PR can share a number on the same host (GitHub's
        // issue/PR numbering is one sequence per repo, but this must not be
        // assumed) -- the key must still tell them apart by kind.
        let issue = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 7,
            url: "https://example.test/issues/7".into(),
            snapshot: snapshot("Title"),
        };
        let pr = SessionSource::PullRequest {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 7,
            url: "https://example.test/pull/7".into(),
            head: "feature".into(),
            base: "main".into(),
            snapshot: snapshot("Title"),
        };
        assert_ne!(issue.identity_key(), pr.identity_key());
    }

    #[test]
    fn identity_key_distinguishes_different_hosts_for_the_same_number() {
        let github = SessionSource::Issue {
            host_id: "github".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 1,
            url: "https://github.example/issues/1".into(),
            snapshot: snapshot("Title"),
        };
        let gitlab = SessionSource::Issue {
            host_id: "gitlab".into(),
            owner: "acme".into(),
            repo: "widgets".into(),
            number: 1,
            url: "https://gitlab.example/issues/1".into(),
            snapshot: snapshot("Title"),
        };
        assert_ne!(github.identity_key(), gitlab.identity_key());
    }

    #[test]
    fn identity_key_diff_paths_ignore_input_order() {
        let a = SessionSource::Diff {
            scope: "working".into(),
            paths: vec!["b.rs".into(), "a.rs".into()],
            snapshot: snapshot("Diff"),
        };
        let b = SessionSource::Diff {
            scope: "working".into(),
            paths: vec!["a.rs".into(), "b.rs".into()],
            snapshot: snapshot("Diff"),
        };
        assert_eq!(a.identity_key(), b.identity_key());
    }

    #[test]
    fn identity_key_is_stable_across_every_variant() {
        // Every variant produces a non-empty, kind-prefixed key -- exercised
        // over the same fixture list used elsewhere so a new variant added
        // without updating `identity_key` shows up here too (it would panic
        // in the `match` at compile time, but this also proves the string is
        // sane, not just present).
        for source in all_source_variants() {
            let key = source.identity_key();
            assert!(!key.is_empty());
            assert!(
                key.starts_with(source.kind_label()),
                "key {key} should start with kind label {}",
                source.kind_label()
            );
        }
    }

    // -- R3.5: base_oid/provider/mode/team on ExecutionRecord --

    #[test]
    fn minimal_execution_record_leaves_the_r3_5_fields_unset() {
        // `::minimal` is what every non-execution-start call site in this
        // codebase uses to build a record (tests, and the pre-existing
        // `record_execution_if_not_running` scaffold before it is filled in
        // by the execution-start path that owns populating these fields).
        // Nothing here should silently default to `Some(...)`.
        let record = ExecutionRecord::minimal(
            "exec-1".into(),
            "sess-1".into(),
            None,
            SessionState::Preparing,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        );
        assert_eq!(record.base_oid, None);
        assert_eq!(record.provider, None);
        assert_eq!(record.mode, None);
        assert_eq!(record.team, None);
    }

    #[test]
    fn execution_record_json_predating_r3_5_still_deserializes() {
        // A session file written before this change has no `baseOid`/
        // `provider`/`mode`/`team` keys at all. `#[serde(default)]` must
        // keep those records loadable rather than turning every existing
        // saved session into `SessionDamaged` on the next app open.
        let legacy_json = serde_json::json!({
            "executionId": "exec-1",
            "sessionId": "sess-1",
            "parentExecutionId": null,
            "state": "finished",
            "startedAt": "2026-01-01T00:00:00Z",
            "endedAt": "2026-01-01T00:05:00Z",
            "lastSequence": 3
        });
        let record: ExecutionRecord =
            serde_json::from_value(legacy_json).expect("legacy execution JSON must still parse");
        assert_eq!(record.base_oid, None);
        assert_eq!(record.provider, None);
        assert_eq!(record.mode, None);
        assert_eq!(record.team, None);
        assert_eq!(record.worktree_path, None, "other pre-existing optional fields must also default");
    }

    #[test]
    fn execution_record_round_trips_the_r3_5_fields_through_json() {
        let mut record = ExecutionRecord::minimal(
            "exec-1".into(),
            "sess-1".into(),
            None,
            SessionState::Working,
            "2026-01-01T00:00:00Z".into(),
            None,
            0,
        );
        record.base_oid = Some("abc1234".into());
        record.provider = Some("copilot".into());
        record.mode = Some("auto".into());
        record.team = Some("lead".into());
        record.worktree_path = Some("C:/code/proj/.worktrees/gitwyrm-fix/thing".into());
        record.branch = Some("gitwyrm-fix/thing".into());

        let json = serde_json::to_string(&record).expect("serialize");
        let round_tripped: ExecutionRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round_tripped, record);
    }
}
