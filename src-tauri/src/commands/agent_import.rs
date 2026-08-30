//! Commands for external chat import (`agent-desk-external-chat-import`).
//!
//! This module owns everything about *bringing external history in* --
//! detection, listing, reading, importing into a durable
//! [`crate::agentdesk::AgentSession`], and the honest "continue" actions.
//! It never touches `commands::agent_desk` or `commands::spec_desk`
//! internals directly; it composes their public store/model surface the same
//! way those modules do, so command ownership stays partitioned during
//! concurrent development.
//!
//! # Capability flags (task 1.4)
//!
//! Each adapter ships behind its own flag in [`ADAPTER_CAPABILITY_FLAGS`].
//! An adapter can be fully implemented and still disabled here -- that is
//! the documented Gate 6 outcome for one that "cannot be finished honestly"
//! (build-order.md): registered so detection/tests exist, but never surfaced
//! to a user via [`agent_import_list_adapters`] while its flag is off.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::agentdesk::adapters::{
    self, AdapterDetectionResult, AdapterError, AdapterRegistry, ContinuationCapability,
    DetectedClient, ExternalRole, ExternalSessionSummary,
};
use crate::agentdesk::import_store::{self, ImportedSessionRecord};
use crate::agentdesk::model::{
    AgentSession, AgentSessionHeader, ConversationSegment, ImportProvenance, MessageKind,
    MessageRole, SessionIntent, SessionMessage, SessionSource, SessionState,
    CURRENT_SCHEMA_VERSION,
};
use crate::agentdesk::reconcile::{self, KnownRepo, ProjectResolution};
use crate::agentdesk::store::{self, SessionStoreRoot, WriteError};
use crate::agentdesk::SessionLocks;
use crate::error::AppError;

/// Whether each adapter is offered to users at all, independent of whether
/// its client is detected on this machine. Flip an adapter to `true` only
/// after its own Gate 6 evidence (task 3.6/5.4) is real: supported,
/// unsupported-version, corrupt-session, missing-path, and 1,000-session
/// fixtures passing, plus the no-writes proof.
///
/// Codex, Claude Code, OpenCode, and VS Code Copilot were each built and
/// tested against a real installation's on-disk layout during this change
/// (see each adapter module's doc comment for what was verified) and their
/// fixture suites cover every Gate 6 case, so they ship enabled.
///
/// OpenChamber could not be verified against a real installation on this
/// machine (see `agentdesk::adapters::openchamber`'s doc comment) -- its
/// adapter is registered so detection exists, but stays off here per
/// build-order.md: "An adapter that fails its gate stays hidden behind its
/// capability flag without blocking the rest of Agent Desk."
pub const ADAPTER_CAPABILITY_FLAGS: &[(&str, bool)] = &[
    ("codex", true),
    ("claude-code", true),
    ("opencode", true),
    ("vscode-copilot", true),
    ("openchamber", false),
];

fn is_enabled(adapter_id: &str) -> bool {
    ADAPTER_CAPABILITY_FLAGS
        .iter()
        .find(|(id, _)| *id == adapter_id)
        .map(|(_, enabled)| *enabled)
        .unwrap_or(false)
}

fn resolve_root(app: &AppHandle) -> Result<SessionStoreRoot, AppError> {
    SessionStoreRoot::resolve(app).map_err(|e| AppError::Other(e.to_string()))
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

// -- 1.4 / 4.1: list adapters with detection state --

/// One adapter's row for the detected-clients UI (task 4.1): identity,
/// whether its capability flag is on, and its detection outcome. Detection
/// still runs for a disabled adapter (OpenChamber) so the UI can say "found,
/// not supported yet" rather than nothing at all -- `enabled` is what gates
/// whether import actions are offered, completely independent of whether the
/// client was found.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdapterListEntry {
    pub adapter_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub supported_version_range: String,
    pub detection: crate::agentdesk::adapters::DetectionOutcome,
}

#[tauri::command]
#[specta::specta]
pub async fn agent_import_list_adapters(app: AppHandle) -> Result<Vec<AdapterListEntry>, AppError> {
    let _ = &app; // detection touches no app state today; kept for signature symmetry with other agent_* commands.
    tauri::async_runtime::spawn_blocking(list_adapters_now)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

fn list_adapters_now() -> Vec<AdapterListEntry> {
    let registry = AdapterRegistry::with_default_adapters();
    let detections: HashMap<String, AdapterDetectionResult> = registry
        .detect_all()
        .into_iter()
        .map(|d| (d.adapter_id.clone(), d))
        .collect();

    registry
        .ids()
        .into_iter()
        .filter_map(|id| {
            let adapter = registry.get(id)?;
            let detection = detections.get(id)?;
            Some(AdapterListEntry {
                adapter_id: id.to_string(),
                display_name: adapter.display_name().to_string(),
                enabled: is_enabled(id),
                supported_version_range: adapter.supported_version_range().to_string(),
                detection: detection.outcome.clone(),
            })
        })
        .collect()
}

// -- 1.5: scan / list / read --

/// One external session as offered to the import picker: the adapter's own
/// summary, plus how its project path resolves against known GitWyrm repos
/// (task 2.4) so the UI can show an honest "unresolved" state per-row
/// without a second round trip.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScannedExternalSession {
    pub adapter_id: String,
    pub summary: ExternalSessionSummary,
    pub project: ProjectResolution,
    /// `true` when this external session already has a matching GitWyrm
    /// session per the import ledger, so the UI can offer "Open imported
    /// session" instead of "Import" (task 2.3).
    pub already_imported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportScanOutcome {
    Scanned {
        sessions: Vec<ScannedExternalSession>,
    },
    AdapterDisabled,
    ClientNotDetected,
    Failed {
        error: AdapterError,
    },
}

#[tauri::command]
#[specta::specta]
pub async fn agent_import_scan(
    app: AppHandle,
    adapter_id: String,
) -> Result<ImportScanOutcome, AppError> {
    let root = resolve_root(&app)?;
    let known_repos = known_repos_from_settings(&app);
    tauri::async_runtime::spawn_blocking(move || scan_at(&root, &adapter_id, &known_repos))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Reads `settings.recents` for repo reconciliation candidates. A thin
/// adapter over `settings::read_settings` kept local to this module so
/// `agentdesk::reconcile` itself stays free of any Tauri/settings
/// dependency (see that module's doc comment) and stays unit-testable
/// without one.
fn known_repos_from_settings(app: &AppHandle) -> Vec<KnownRepo> {
    let settings = crate::settings::read_settings(app).unwrap_or_default();
    settings
        .recents
        .into_iter()
        .map(|r| KnownRepo {
            repo_id: crate::state::repo_id_for(std::path::Path::new(&r.path)),
            repo_name: r.name,
            repo_path: r.path,
        })
        .collect()
}

fn scan_at(
    root: &SessionStoreRoot,
    adapter_id: &str,
    known_repos: &[KnownRepo],
) -> ImportScanOutcome {
    if !is_enabled(adapter_id) {
        return ImportScanOutcome::AdapterDisabled;
    }

    let registry = AdapterRegistry::with_default_adapters();
    let Some(adapter) = registry.get(adapter_id) else {
        return ImportScanOutcome::Failed {
            error: AdapterError::ClientNotDetected,
        };
    };

    let detected = match adapter.detect() {
        Ok(Some(d)) => d,
        Ok(None) => return ImportScanOutcome::ClientNotDetected,
        Err(e) => return ImportScanOutcome::Failed { error: e },
    };

    let list = match adapter.list_sessions(&detected) {
        Ok(l) => l,
        Err(e) => return ImportScanOutcome::Failed { error: e },
    };

    let ledger = import_store::read_ledger(root, adapter_id);
    let sessions = list
        .into_iter()
        .map(|summary| {
            let already_imported =
                import_store::already_imported_session(&ledger, &summary.external_session_id)
                    .is_some();
            let project = reconcile::resolve_project_path(
                summary.project_path.as_deref(),
                known_repos,
            );
            ScannedExternalSession {
                adapter_id: adapter_id.to_string(),
                summary,
                project,
                already_imported,
            }
        })
        .collect();

    ImportScanOutcome::Scanned { sessions }
}

// -- 2: import model mapping --

/// Maps one [`ExternalRole`]/raw-flag pair to the durable model's
/// [`MessageRole`]/[`MessageKind`] (task 2.1). A raw/unrecognized record
/// always becomes `MessageKind::System` regardless of its guessed role --
/// the *kind* is what tells the UI "this is not a normal turn," independent
/// of who technically authored it.
fn map_external_message(
    adapter_id: &str,
    external_session_id: &str,
    segment_id: &str,
    msg: &adapters::ExternalMessage,
    imported_at: &str,
) -> SessionMessage {
    let (role, kind) = if msg.raw_unrecognized {
        (ExternalRole::System, MessageKind::System)
    } else {
        match msg.role {
            ExternalRole::User => (ExternalRole::User, MessageKind::User),
            ExternalRole::Assistant => (ExternalRole::Assistant, MessageKind::Assistant),
            ExternalRole::System => (ExternalRole::System, MessageKind::System),
        }
    };
    let role = match role {
        ExternalRole::User => MessageRole::User,
        ExternalRole::Assistant => MessageRole::Assistant,
        ExternalRole::System => MessageRole::System,
    };

    SessionMessage {
        message_id: new_id(),
        segment_id: segment_id.to_string(),
        role,
        timestamp: msg.timestamp.clone(),
        plain_content: msg.plain_content.clone(),
        rendered_content: None,
        provider: Some(adapter_id.to_string()),
        model: msg.model.clone(),
        kind,
        execution_id: None,
        sequence: None,
        import: Some(ImportProvenance {
            adapter_id: adapter_id.to_string(),
            external_session_id: external_session_id.to_string(),
            external_message_id: msg.external_message_id.clone(),
            imported_at: imported_at.to_string(),
        }),
        targets: Vec::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportSessionOutcome {
    /// A brand-new GitWyrm session was created from the external history.
    Created {
        session: AgentSession,
    },
    /// The session had already been imported before; only messages newer
    /// than the ledger's dedup anchor (task 2.3) were appended, if any.
    #[serde(rename_all = "camelCase")]
    Refreshed {
        session: AgentSession,
        new_message_count: u32,
    },
    AdapterDisabled,
    ClientNotDetected,
    SessionNotFound,
    CorruptSession {
        detail: String,
    },
    WriteFailed {
        detail: String,
    },
}

#[tauri::command]
#[specta::specta]
pub async fn agent_import_session(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    adapter_id: String,
    external_session_id: String,
) -> Result<ImportSessionOutcome, AppError> {
    let root = resolve_root(&app)?;
    let known_repos = known_repos_from_settings(&app);
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        import_session_at(&locks, &root, &adapter_id, &external_session_id, &known_repos)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

fn import_session_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    adapter_id: &str,
    external_session_id: &str,
    known_repos: &[KnownRepo],
) -> ImportSessionOutcome {
    if !is_enabled(adapter_id) {
        return ImportSessionOutcome::AdapterDisabled;
    }

    let registry = AdapterRegistry::with_default_adapters();
    let Some(adapter) = registry.get(adapter_id) else {
        return ImportSessionOutcome::ClientNotDetected;
    };
    let detected = match adapter.detect() {
        Ok(Some(d)) => d,
        Ok(None) => return ImportSessionOutcome::ClientNotDetected,
        Err(_) => return ImportSessionOutcome::ClientNotDetected,
    };
    let detail = match adapter.read_session(&detected, external_session_id) {
        Ok(d) => d,
        Err(AdapterError::SessionNotFound { .. }) => return ImportSessionOutcome::SessionNotFound,
        Err(AdapterError::CorruptSession { detail }) => {
            return ImportSessionOutcome::CorruptSession { detail }
        }
        Err(e) => return ImportSessionOutcome::CorruptSession { detail: e.to_string() },
    };

    let imported_at = now_rfc3339();
    let mut ledger = import_store::read_ledger(root, adapter_id);
    let existing = import_store::already_imported_session(&ledger, external_session_id).cloned();

    let project = reconcile::resolve_project_path(detail.summary.project_path.as_deref(), known_repos);
    let (repo_id, repo_path, repo_name) = match &project {
        ProjectResolution::Resolved {
            repo_id,
            repo_path,
            repo_name,
        } => (repo_id.clone(), repo_path.clone(), repo_name.clone()),
        // Unresolved/no-project sessions are still imported -- the spec
        // requires unresolved projects to stay visible, not to block import
        // entirely. They land under a synthetic repo scope so they still
        // have somewhere to be listed; the UI is expected to show the
        // "project not found" state per task 2.4/4.x rather than a normal
        // project-scoped row.
        _ => (
            format!("unresolved:{adapter_id}"),
            detail
                .summary
                .project_path
                .clone()
                .unwrap_or_else(|| "(unknown project)".into()),
            "Unresolved project".to_string(),
        ),
    };

    match existing {
        Some(record) => {
            let session_id = record.gitwyrm_session_id.clone();
            match locks.with_session_lock(&session_id, || {
                append_imported_messages(
                    root,
                    &session_id,
                    adapter_id,
                    external_session_id,
                    &detail,
                    &record,
                    &imported_at,
                )
            }) {
                Ok((session, new_count)) => {
                    ledger.sessions.insert(
                        external_session_id.to_string(),
                        ImportedSessionRecord {
                            gitwyrm_session_id: session_id,
                            last_imported_external_message_id: detail
                                .messages
                                .last()
                                .map(|m| m.external_message_id.clone())
                                .unwrap_or(record.last_imported_external_message_id),
                            last_seen_external_updated_at: detail.summary.updated_at.clone(),
                        },
                    );
                    let _ = import_store::write_ledger(root, adapter_id, &ledger);
                    ImportSessionOutcome::Refreshed {
                        session,
                        new_message_count: new_count,
                    }
                }
                Err(e) => ImportSessionOutcome::WriteFailed { detail: e.to_string() },
            }
        }
        None => {
            let session_id = new_id();
            let session = build_imported_session(
                &session_id,
                &repo_id,
                &repo_path,
                &repo_name,
                adapter_id,
                external_session_id,
                &detail,
                &imported_at,
            );
            match store::write_session(root, &session) {
                Ok(()) => {
                    let (headers, _) = store::rebuild_index_from_sessions(root);
                    let _ = store::write_index(root, &headers);
                    ledger.sessions.insert(
                        external_session_id.to_string(),
                        ImportedSessionRecord {
                            gitwyrm_session_id: session_id,
                            last_imported_external_message_id: detail
                                .messages
                                .last()
                                .map(|m| m.external_message_id.clone())
                                .unwrap_or_default(),
                            last_seen_external_updated_at: detail.summary.updated_at.clone(),
                        },
                    );
                    let _ = import_store::write_ledger(root, adapter_id, &ledger);
                    ImportSessionOutcome::Created { session }
                }
                Err(WriteError::Serialize { detail })
                | Err(WriteError::CreateTemp { detail, .. })
                | Err(WriteError::WriteTemp { detail })
                | Err(WriteError::Flush { detail })
                | Err(WriteError::Rename { detail, .. })
                // Import never deletes, so this arm is unreachable. Listed
                // rather than wildcarded so a future variant still has to be
                // decided on here instead of silently becoming a write
                // failure.
                | Err(WriteError::Delete { detail, .. }) => {
                    ImportSessionOutcome::WriteFailed { detail }
                }
            }
        }
    }
}

fn build_imported_session(
    session_id: &str,
    repo_id: &str,
    repo_path: &str,
    repo_name: &str,
    adapter_id: &str,
    external_session_id: &str,
    detail: &adapters::ExternalSessionDetail,
    imported_at: &str,
) -> AgentSession {
    let segment_id = new_id();
    let messages: Vec<SessionMessage> = detail
        .messages
        .iter()
        .map(|m| {
            map_external_message(adapter_id, external_session_id, &segment_id, m, imported_at)
        })
        .collect();

    let header = AgentSessionHeader {
        schema_version: CURRENT_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        repo_id: repo_id.to_string(),
        repo_path: repo_path.to_string(),
        repo_name: repo_name.to_string(),
        title: detail.summary.title.clone(),
        source: SessionSource::Manual {
            repo_id: repo_id.to_string(),
        },
        intent: SessionIntent::Ask,
        state: SessionState::Finished,
        created_at: messages
            .first()
            .map(|m| m.timestamp.clone())
            .unwrap_or_else(|| imported_at.to_string()),
        updated_at: detail.summary.updated_at.clone(),
        unread: true,
        changed_file_count: 0,
        active_execution_id: None,
        archived: false,
        graph_started_at: None,
    };

    let mut session = AgentSession::new(header);
    session.segments.push(ConversationSegment {
        segment_id,
        label: format!("Imported from {adapter_id}"),
        started_at: messages
            .first()
            .map(|m| m.timestamp.clone())
            .unwrap_or_else(|| imported_at.to_string()),
    });
    session.messages = messages;
    session
}

/// Append only messages newer than the ledger's dedup anchor to an
/// already-imported session (task 2.3: incremental refresh). Runs inside
/// the caller's `with_session_lock` so this read-modify-write is atomic with
/// respect to any other mutation of the same session, matching every other
/// mutating path in `commands::agent_desk`.
fn append_imported_messages(
    root: &SessionStoreRoot,
    session_id: &str,
    adapter_id: &str,
    external_session_id: &str,
    detail: &adapters::ExternalSessionDetail,
    record: &ImportedSessionRecord,
    imported_at: &str,
) -> Result<(AgentSession, u32), WriteError> {
    let mut session = match store::read_session(root, session_id) {
        Ok(s) => s,
        Err(_) => {
            // The GitWyrm session the ledger points at is gone/damaged.
            // Rebuilding it from scratch here would silently fork history;
            // instead this surfaces as a write failure so the caller can
            // decide (task 2.3's "handle external edits/deletions honestly"
            // extends to *our own* bookkeeping going stale too).
            return Err(WriteError::Serialize {
                detail: format!("imported session {session_id} is missing or damaged"),
            });
        }
    };

    // Messages carry no external ordering key besides their position in
    // `detail.messages` (client-original order, per `ExternalSessionDetail`'s
    // doc comment) plus their own IDs, which are opaque per-adapter strings
    // -- not guaranteed sortable. Dedup is therefore by ID-seen-before, not
    // by "everything after the anchor": every external message ID already
    // present as an `ImportProvenance::external_message_id` on this session
    // is skipped, and everything else is appended in the external client's
    // own order. This is correct even if a client's IDs are not
    // monotonic, and it is what makes a re-scan idempotent (spec: "Repeated
    // scans SHALL not duplicate").
    let already_present: std::collections::HashSet<String> = session
        .messages
        .iter()
        .filter_map(|m| m.import.as_ref())
        .filter(|p| p.adapter_id == adapter_id && p.external_session_id == external_session_id)
        .map(|p| p.external_message_id.clone())
        .collect();

    let segment_id = session
        .segments
        .iter()
        .find(|s| s.label == format!("Imported from {adapter_id}"))
        .map(|s| s.segment_id.clone())
        .unwrap_or_else(|| {
            let id = new_id();
            session.segments.push(ConversationSegment {
                segment_id: id.clone(),
                label: format!("Imported from {adapter_id}"),
                started_at: imported_at.to_string(),
            });
            id
        });

    let mut new_count = 0u32;
    for m in &detail.messages {
        if already_present.contains(m.external_message_id.as_str()) {
            continue;
        }
        session.messages.push(map_external_message(
            adapter_id,
            external_session_id,
            &segment_id,
            m,
            imported_at,
        ));
        new_count += 1;
    }

    if new_count > 0 {
        session.header.updated_at = detail.summary.updated_at.clone();
        session.header.unread = true;
    }
    let _ = record; // anchor already folded into the id-set dedup above.

    store::write_session(root, &session)?;
    let (headers, _) = store::rebuild_index_from_sessions(root);
    let _ = store::write_index(root, &headers);
    Ok((session, new_count))
}

// -- 4.3: Continue here / Continue externally / unlink --

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ContinuationOutcome {
    /// The client cannot resume this exact session, but it can be opened.
    /// The UI must render this as "Open client," never "Continue session"
    /// (spec scenario "Open only").
    OpenOnly,
    /// No supported launch mechanism exists at all.
    Unsupported,
    ClientNotDetected,
    AdapterDisabled,
}

#[tauri::command]
#[specta::specta]
pub async fn agent_import_continuation_capability(
    adapter_id: String,
    external_session_id: String,
) -> Result<ContinuationOutcome, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        continuation_capability_now(&adapter_id, &external_session_id)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

fn continuation_capability_now(adapter_id: &str, external_session_id: &str) -> ContinuationOutcome {
    if !is_enabled(adapter_id) {
        return ContinuationOutcome::AdapterDisabled;
    }
    let registry = AdapterRegistry::with_default_adapters();
    let Some(adapter) = registry.get(adapter_id) else {
        return ContinuationOutcome::ClientNotDetected;
    };
    let detected: DetectedClient = match adapter.detect() {
        Ok(Some(d)) => d,
        Ok(None) => return ContinuationOutcome::ClientNotDetected,
        Err(_) => return ContinuationOutcome::ClientNotDetected,
    };
    match adapter.continuation_capability(&detected, external_session_id) {
        ContinuationCapability::ResumeSession | ContinuationCapability::OpenOnly => {
            ContinuationOutcome::OpenOnly
        }
        ContinuationCapability::Unsupported => ContinuationOutcome::Unsupported,
    }
}

/// "Continue here" (task 4.4/9): append a new native segment to an imported
/// session with a short handoff summary, preserving every prior message's
/// provenance untouched. The handoff message itself carries no
/// `ImportProvenance` -- it is genuinely native GitWyrm output from this
/// point forward, and that boundary is exactly what the new segment marks.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ContinueHereOutcome {
    Continued { session: AgentSession },
    NotFound,
    WriteFailed { detail: String },
}

#[tauri::command]
#[specta::specta]
pub async fn agent_import_continue_here(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    session_id: String,
) -> Result<ContinueHereOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || continue_here_at(&locks, &root, &session_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

fn continue_here_at(
    locks: &SessionLocks,
    root: &SessionStoreRoot,
    session_id: &str,
) -> ContinueHereOutcome {
    locks.with_session_lock(session_id, || {
        let mut session = match store::read_session(root, session_id) {
            Ok(s) => s,
            Err(_) => return ContinueHereOutcome::NotFound,
        };

        let now = now_rfc3339();
        let new_segment_id = new_id();
        let source_count = session.messages.iter().filter(|m| m.import.is_some()).count();
        session.segments.push(ConversationSegment {
            segment_id: new_segment_id.clone(),
            label: "Continued in GitWyrm".into(),
            started_at: now.clone(),
        });
        session.messages.push(SessionMessage {
            message_id: new_id(),
            segment_id: new_segment_id,
            role: MessageRole::System,
            timestamp: now.clone(),
            plain_content: format!(
                "Continuing from {source_count} imported message(s). The messages above were \
                 produced outside GitWyrm and are shown with their original source badge; \
                 everything from here on is native to this session."
            ),
            rendered_content: None,
            provider: None,
            model: None,
            kind: MessageKind::System,
            execution_id: None,
            sequence: None,
            import: None,
            targets: Vec::new(),
        });
        session.header.updated_at = now;
        session.header.state = SessionState::Ready;

        match store::write_session(root, &session) {
            Ok(()) => {
                let (headers, _) = store::rebuild_index_from_sessions(root);
                let _ = store::write_index(root, &headers);
                ContinueHereOutcome::Continued { session }
            }
            Err(e) => ContinueHereOutcome::WriteFailed {
                detail: e.to_string(),
            },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::adapters::{ExternalMessage, ExternalSessionDetail};

    fn test_locks() -> SessionLocks {
        SessionLocks::new()
    }

    fn temp_root() -> (tempfile::TempDir, SessionStoreRoot) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = SessionStoreRoot::at(dir.path().join("agent-desk").join("v1")).unwrap();
        (dir, root)
    }

    fn sample_detail(session_id: &str, messages: Vec<ExternalMessage>) -> ExternalSessionDetail {
        ExternalSessionDetail {
            summary: ExternalSessionSummary {
                external_session_id: session_id.into(),
                title: "Test session".into(),
                updated_at: "2026-01-01T00:00:02Z".into(),
                project_path: Some("C:/code/fixture-project".into()),
                message_count: messages.len() as u32,
                model: Some("test-model".into()),
            },
            messages,
        }
    }

    fn user_msg(id: &str, ts: &str, text: &str) -> ExternalMessage {
        ExternalMessage {
            external_message_id: id.into(),
            role: ExternalRole::User,
            timestamp: ts.into(),
            plain_content: text.into(),
            model: None,
            raw_unrecognized: false,
        }
    }

    fn assistant_msg(id: &str, ts: &str, text: &str) -> ExternalMessage {
        ExternalMessage {
            external_message_id: id.into(),
            role: ExternalRole::Assistant,
            timestamp: ts.into(),
            plain_content: text.into(),
            model: None,
            raw_unrecognized: false,
        }
    }

    #[test]
    fn adapter_capability_flags_cover_all_five_shipped_adapters() {
        let ids: Vec<&str> = ADAPTER_CAPABILITY_FLAGS.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), 5);
        for id in ["codex", "claude-code", "opencode", "vscode-copilot", "openchamber"] {
            assert!(ids.contains(&id), "missing flag entry for {id}");
        }
    }

    #[test]
    fn openchamber_is_disabled_by_default_pending_a_verified_schema() {
        assert!(!is_enabled("openchamber"));
    }

    #[test]
    fn every_other_shipped_adapter_is_enabled_by_default() {
        for id in ["codex", "claude-code", "opencode", "vscode-copilot"] {
            assert!(is_enabled(id), "{id} should be enabled");
        }
    }

    #[test]
    fn mapping_a_raw_unrecognized_message_always_becomes_system_kind() {
        let msg = ExternalMessage {
            external_message_id: "m1".into(),
            role: ExternalRole::User, // even though the guessed role is User
            timestamp: "2026-01-01T00:00:00Z".into(),
            plain_content: String::new(),
            model: None,
            raw_unrecognized: true,
        };
        let mapped = map_external_message("codex", "ext-1", "seg-1", &msg, "2026-01-01T00:00:01Z");
        assert_eq!(mapped.kind, MessageKind::System);
        assert!(mapped.import.is_some());
    }

    #[test]
    fn mapping_preserves_full_provenance() {
        let msg = user_msg("m1", "2026-01-01T00:00:00Z", "hi");
        let mapped = map_external_message("codex", "ext-1", "seg-1", &msg, "2026-01-01T00:00:05Z");
        let provenance = mapped.import.expect("must carry provenance");
        assert_eq!(provenance.adapter_id, "codex");
        assert_eq!(provenance.external_session_id, "ext-1");
        assert_eq!(provenance.external_message_id, "m1");
        assert_eq!(provenance.imported_at, "2026-01-01T00:00:05Z");
    }

    #[test]
    fn a_scan_against_a_disabled_adapter_says_so_rather_than_pretending_not_detected() {
        let (_dir, root) = temp_root();
        let outcome = scan_at(&root, "openchamber", &[]);
        assert!(matches!(outcome, ImportScanOutcome::AdapterDisabled));
    }

    #[test]
    fn importing_a_session_creates_a_new_gitwyrm_session_with_preserved_provenance() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let detail = sample_detail(
            "ext-1",
            vec![
                user_msg("m1", "2026-01-01T00:00:00Z", "hello"),
                assistant_msg("m2", "2026-01-01T00:00:01Z", "hi back"),
            ],
        );

        // Bypass the adapter registry (no real Codex install in tests) by
        // calling the pure mapping/build helpers directly through a stub
        // that mirrors import_session_at's own logic for a Created outcome.
        let session_id = "sess-import-1";
        let session = build_imported_session(
            session_id,
            "repo-1",
            "C:/code/fixture-project",
            "fixture-project",
            "codex",
            "ext-1",
            &detail,
            "2026-01-01T00:00:05Z",
        );
        assert_eq!(session.messages.len(), 2);
        assert!(session.messages.iter().all(|m| m.import.is_some()));
        assert_eq!(session.header.title, "Test session");

        store::write_session(&root, &session).unwrap();
        let back = store::read_session(&root, session_id).unwrap();
        assert_eq!(back, session);
        let _ = locks; // exercised in append test below
    }

    #[test]
    fn re_importing_only_appends_genuinely_new_messages() {
        let (_dir, root) = temp_root();
        let locks = test_locks();

        let first_detail = sample_detail("ext-1", vec![user_msg("m1", "2026-01-01T00:00:00Z", "hello")]);
        let session_id = "sess-refresh-1";
        let session = build_imported_session(
            session_id,
            "repo-1",
            "C:/code/fixture-project",
            "fixture-project",
            "codex",
            "ext-1",
            &first_detail,
            "2026-01-01T00:00:05Z",
        );
        store::write_session(&root, &session).unwrap();

        let record = ImportedSessionRecord {
            gitwyrm_session_id: session_id.to_string(),
            last_imported_external_message_id: "m1".into(),
            last_seen_external_updated_at: "2026-01-01T00:00:00Z".into(),
        };

        // Re-scan reports the same message plus one genuinely new one.
        let second_detail = sample_detail(
            "ext-1",
            vec![
                user_msg("m1", "2026-01-01T00:00:00Z", "hello"),
                assistant_msg("m2", "2026-01-01T00:00:01Z", "hi back"),
            ],
        );

        let (updated, new_count) = locks
            .with_session_lock(session_id, || {
                append_imported_messages(
                    &root,
                    session_id,
                    "codex",
                    "ext-1",
                    &second_detail,
                    &record,
                    "2026-01-01T00:00:10Z",
                )
            })
            .unwrap();

        assert_eq!(new_count, 1, "only the genuinely new message should be appended");
        assert_eq!(updated.messages.len(), 2);

        // A third scan with the same data appends nothing further.
        let (updated_again, new_count_again) = locks
            .with_session_lock(session_id, || {
                append_imported_messages(
                    &root,
                    session_id,
                    "codex",
                    "ext-1",
                    &second_detail,
                    &record,
                    "2026-01-01T00:00:20Z",
                )
            })
            .unwrap();
        assert_eq!(new_count_again, 0, "re-scanning the same data must not duplicate");
        assert_eq!(updated_again.messages.len(), 2);
    }

    #[test]
    fn continue_here_appends_a_native_segment_and_preserves_prior_provenance() {
        let (_dir, root) = temp_root();
        let locks = test_locks();

        let detail = sample_detail("ext-1", vec![user_msg("m1", "2026-01-01T00:00:00Z", "hello")]);
        let session_id = "sess-continue-1";
        let session = build_imported_session(
            session_id,
            "repo-1",
            "C:/code/fixture-project",
            "fixture-project",
            "codex",
            "ext-1",
            &detail,
            "2026-01-01T00:00:05Z",
        );
        store::write_session(&root, &session).unwrap();

        let outcome = continue_here_at(&locks, &root, session_id);
        let ContinueHereOutcome::Continued { session } = outcome else {
            panic!("expected Continued, got {outcome:?}");
        };
        // Original imported message is untouched.
        assert!(session.messages[0].import.is_some());
        // The new handoff message carries no import provenance -- it is
        // genuinely native.
        let handoff = session.messages.last().unwrap();
        assert!(handoff.import.is_none());
        assert_eq!(session.segments.len(), 2);
    }

    #[test]
    fn continue_here_on_a_missing_session_is_not_found() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        let outcome = continue_here_at(&locks, &root, "does-not-exist");
        assert!(matches!(outcome, ContinueHereOutcome::NotFound));
    }
}
