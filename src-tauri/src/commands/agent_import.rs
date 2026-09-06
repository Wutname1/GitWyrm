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
    self, redact_for_log, AdapterDetectionResult, AdapterError, AdapterRegistry,
    ContinuationCapability, DetectedClient, ExternalRole, ExternalSessionSummary,
};
use crate::agentdesk::import_store::{self, ImportedSessionRecord};
use crate::agentdesk::model::{
    AgentSession, AgentSessionHeader, ConversationSegment, ImportProvenance, MessageKind,
    MessageRole, SessionIntent, SessionMessage, SessionSource, SessionState, SourceSnapshot,
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
    /// The GitWyrm session this external session was imported into, when
    /// `already_imported` is `true`. This is what "Continue here" and
    /// "Unlink" (task 4.3) act on, so a row can offer them without a second
    /// lookup.
    pub imported_session_id: Option<String>,
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

    // Adapter errors can name the file that failed (`MissingPath`, I/O), so
    // they only reach the log through `redact_for_log` (task 5.2).
    let detected = match adapter.detect() {
        Ok(Some(d)) => d,
        Ok(None) => return ImportScanOutcome::ClientNotDetected,
        Err(e) => {
            log::warn!(
                "import detection for {adapter_id} failed: {}",
                redact_for_log(&e.to_string())
            );
            return ImportScanOutcome::Failed { error: e };
        }
    };

    let list = match adapter.list_sessions(&detected) {
        Ok(l) => l,
        Err(e) => {
            log::warn!(
                "import scan for {adapter_id} failed: {}",
                redact_for_log(&e.to_string())
            );
            return ImportScanOutcome::Failed { error: e };
        }
    };

    let ledger = import_store::read_ledger(root, adapter_id);
    let sessions = list
        .into_iter()
        .map(|summary| {
            let imported_session_id =
                import_store::already_imported_session(&ledger, &summary.external_session_id)
                    .map(|r| r.gitwyrm_session_id.clone());
            let already_imported = imported_session_id.is_some();
            let project = reconcile::resolve_project_path(
                summary.project_path.as_deref(),
                known_repos,
            );
            ScannedExternalSession {
                adapter_id: adapter_id.to_string(),
                summary,
                project,
                already_imported,
                imported_session_id,
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
    // Checked, because everything below writes this ledger back. The
    // defaulting read turned a damaged ledger into an empty one, and the write
    // at the end then made that permanent -- every record of what had already
    // been imported for this tool, discarded by an unrelated import, silently.
    // Refusing costs the person one import; overwriting costs them all of them.
    let mut ledger = match import_store::read_ledger_checked(root, adapter_id) {
        Ok(l) => l,
        Err(e) => {
            log::error!("import ledger unreadable for {adapter_id}: {e}");
            return ImportSessionOutcome::WriteFailed {
                detail: format!(
                    "GitWyrm could not read its record of what has already been brought in from                      this tool, so it stopped rather than risk losing it. ({e})"
                ),
            };
        }
    };
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
                    // A refresh already has its ledger entry; losing this
                    // update costs a re-check of messages it already has,
                    // which provenance handles.
                    let _ = write_ledger_logged(root, adapter_id, &ledger);
                    ImportSessionOutcome::Refreshed {
                        session,
                        new_message_count: new_count,
                    }
                }
                Err(e) => {
                    log::warn!(
                        "refreshing an imported {adapter_id} chat failed: {}",
                        redact_for_log(&e.to_string())
                    );
                    ImportSessionOutcome::WriteFailed { detail: e.to_string() }
                }
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
                    // Only the create path needs this. A refresh already has a
                    // ledger entry, and message provenance covers its messages;
                    // a first import has neither, so a lost ledger write here
                    // is the one case that silently duplicates the whole chat.
                    if write_ledger_logged(root, adapter_id, &ledger) {
                        ImportSessionOutcome::Created { session }
                    } else {
                        let mut session = session;
                        push_system_note(&mut session, LEDGER_NOT_SAVED_NOTE);
                        // Best effort: the chat is already saved without the
                        // note, so a failure here loses the warning, not the
                        // import. Nothing further can be said if this write is
                        // failing too.
                        let _ = store::write_session(root, &session);
                        ImportSessionOutcome::Created { session }
                    }
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
                    log::warn!(
                        "importing a {adapter_id} chat failed: {}",
                        redact_for_log(&detail)
                    );
                    ImportSessionOutcome::WriteFailed { detail }
                }
            }
        }
    }
}

/// A ledger that fails to write is not fatal to the import (the session is
/// already saved, and dedup falls back to message provenance), but it must
/// not vanish silently either: the next scan would show "Import" again for a
/// chat that is already here. The error text names the ledger path, so it is
/// redacted before it reaches the log.
fn write_ledger_logged(
    root: &SessionStoreRoot,
    adapter_id: &str,
    ledger: &import_store::AdapterImportLedger,
) -> bool {
    if let Err(e) = import_store::write_ledger(root, adapter_id, ledger) {
        log::warn!(
            "import ledger for {adapter_id} could not be saved: {}",
            redact_for_log(&e.to_string())
        );
        return false;
    }
    true
}

/// What to tell someone whose import was saved but not recorded.
///
/// The doc on `already_imported_session` says a lost ledger "degrades to
/// re-checks messages it already has, never to silently duplicates". That is
/// true for a session already on the ledger -- message provenance catches the
/// individual messages. It is NOT true for a first import: the create path
/// consults only the ledger, so a lost write means the next import builds a
/// second chat with a new id, never reading the first.
///
/// Logging alone did not reach the person. This does, in the one place they
/// are certain to look.
/// What the adapters substitute when an external conversation records no
/// date of its own.
///
/// It is a sentinel, not a reading. Copied onto a session header it becomes
/// the age shown in the sidebar, where it renders as "56y" -- a confident,
/// absurd measurement of something GitWyrm was never told.
///
/// Guarded here rather than by making every adapter's `updated_at` optional:
/// that is roughly seventeen substitution sites across four adapters plus
/// every sort, and this is the one boundary where the value stops being an
/// adapter's internal placeholder and becomes text a person reads.
const NO_RECORDED_DATE: &str = "1970-01-01T00:00:00Z";

/// The date to show for an imported conversation.
///
/// When the original recorded one, that is the honest answer. When it did
/// not, the honest answer is when GitWyrm imported it -- a time GitWyrm
/// genuinely measured -- rather than a year nobody was there for.
fn imported_session_updated_at(external: &str, imported_at: &str) -> String {
    if external == NO_RECORDED_DATE {
        imported_at.to_string()
    } else {
        external.to_string()
    }
}

const LEDGER_NOT_SAVED_NOTE: &str =
    "This chat was brought in, but GitWyrm could not save its note that it had. If you import this same chat again, you will get a second copy of it rather than an update to this one.";

/// Append a plain-language note to a session the caller already holds.
///
/// The graph module has `append_system_note`, but that one re-reads the
/// session under its lock. Here the session is already in hand and about to
/// be written, so re-reading would be both wasteful and a second chance to
/// fail.
fn push_system_note(session: &mut AgentSession, text: &str) {
    let now = now_rfc3339();
    let segment_id = session
        .segments
        .last()
        .map(|seg| seg.segment_id.clone())
        .unwrap_or_default();
    session.messages.push(SessionMessage {
        message_id: new_id(),
        segment_id,
        role: MessageRole::System,
        timestamp: now.clone(),
        plain_content: text.to_string(),
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
        // Says where this conversation came from. It was `Manual` --
        // indistinguishable from a chat the person started here -- so the
        // sidebar row, the source filter and the grouping all called an
        // imported chat "Chat", while every message inside it carried honest
        // provenance. The session now says the same thing its messages do.
        source: SessionSource::Imported {
            adapter_id: adapter_id.to_string(),
            external_session_id: external_session_id.to_string(),
            snapshot: SourceSnapshot {
                title: detail.summary.title.clone(),
                summary: format!("Imported from {adapter_id}"),
                captured_at: imported_at.to_string(),
                // Not "unavailable": the copy is complete and local. There is
                // simply no live source to re-read, which `refresh_source`
                // handles by declining rather than by flagging a loss.
                live_unavailable: false,
            },
        },
        intent: SessionIntent::Ask,
        state: SessionState::Finished,
        created_at: messages
            .first()
            .map(|m| m.timestamp.clone())
            .unwrap_or_else(|| imported_at.to_string()),
        updated_at: imported_session_updated_at(&detail.summary.updated_at, &imported_at.to_string()),
        unread: true,
        changed_file_count: 0,
        active_execution_id: None,
        archived: false,
        graph_started_at: None,
        // NOT the adapter id. `preferred_provider` is an instruction for the
        // next run -- it becomes the provider override when the person
        // continues the chat -- and adapter ids live in a different namespace
        // from provider ids. Only `codex` and `opencode` happen to appear in
        // both; `claude-code`, `vscode-copilot` and `openchamber` are not
        // providers at all, so continuing an imported chat from any of those
        // three refused with UnsupportedProvider before it could start.
        //
        // `None` means "whatever this person's default is", which is the
        // honest answer: GitWyrm cannot run the client the chat came from, it
        // can only continue the conversation with a tool it does have. The
        // origin is not lost -- every imported message carries its own
        // `ImportProvenance`, which is what the transcript attributes from.
        preferred_provider: None,
        preferred_mode: None,
        preferred_team: None,
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
        session.header.updated_at =
            imported_session_updated_at(&detail.summary.updated_at, &imported_at.to_string());
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

/// Result of "Unlink from <client>" (task 4.3). Unlinking only forgets the
/// ledger entry that ties a GitWyrm session to its external source; every
/// imported message, and the provenance on each one, stays in the session.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UnlinkOutcome {
    #[serde(rename_all = "camelCase")]
    Unlinked {
        session: AgentSession,
        adapter_id: String,
        adapter_display_name: String,
    },
    /// The session exists but no ledger points at it (never imported, or
    /// already unlinked), so there was nothing to change.
    NotLinked,
    NotFound,
    Failed {
        detail: String,
    },
}

#[tauri::command]
#[specta::specta]
pub async fn agent_import_unlink(
    app: AppHandle,
    locks: tauri::State<'_, std::sync::Arc<SessionLocks>>,
    session_id: String,
) -> Result<UnlinkOutcome, AppError> {
    let root = resolve_root(&app)?;
    let locks = locks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || unlink_at(&locks, &root, &session_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

fn adapter_display_name(adapter_id: &str) -> String {
    AdapterRegistry::with_default_adapters()
        .get(adapter_id)
        .map(|a| a.display_name().to_string())
        .unwrap_or_else(|| adapter_id.to_string())
}

fn unlink_at(locks: &SessionLocks, root: &SessionStoreRoot, session_id: &str) -> UnlinkOutcome {
    locks.with_session_lock(session_id, || {
        let mut session = match store::read_session(root, session_id) {
            Ok(s) => s,
            Err(_) => return UnlinkOutcome::NotFound,
        };
        let Some(link) = import_store::find_link_for_session(root, session_id) else {
            return UnlinkOutcome::NotLinked;
        };

        // The ledger goes first: if it cannot be written, the session is left
        // untouched and the user sees a failure rather than a session that
        // says "unlinked" while a re-scan still refreshes it.
        match import_store::remove_link(root, &link.adapter_id, &link.external_session_id) {
            Ok(true) => {}
            Ok(false) => return UnlinkOutcome::NotLinked,
            Err(e) => {
                log::warn!(
                    "unlink of an imported chat from {} failed: {}",
                    link.adapter_id,
                    redact_for_log(&e.to_string())
                );
                return UnlinkOutcome::Failed {
                    detail: e.to_string(),
                };
            }
        }

        let display_name = adapter_display_name(&link.adapter_id);
        let now = now_rfc3339();
        let segment_id = new_id();
        // A durable, visible mark in the transcript is what tells both the
        // UI and a future reader why this chat no longer refreshes from, or
        // offers to open, the external client (Rule #1: every action shows).
        session.segments.push(ConversationSegment {
            segment_id: segment_id.clone(),
            label: format!("Unlinked from {display_name}"),
            started_at: now.clone(),
        });
        session.messages.push(SessionMessage {
            message_id: new_id(),
            segment_id,
            role: MessageRole::System,
            timestamp: now.clone(),
            plain_content: format!(
                "This chat is no longer linked to {display_name}. The messages above stay here \
                 as they were imported. Importing the same {display_name} chat again will \
                 create a new chat instead of adding to this one."
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

        match store::write_session(root, &session) {
            Ok(()) => {
                let (headers, _) = store::rebuild_index_from_sessions(root);
                let _ = store::write_index(root, &headers);
                UnlinkOutcome::Unlinked {
                    session,
                    adapter_id: link.adapter_id,
                    adapter_display_name: display_name,
                }
            }
            Err(e) => {
                log::warn!(
                    "unlinked chat could not be saved: {}",
                    redact_for_log(&e.to_string())
                );
                UnlinkOutcome::Failed {
                    detail: e.to_string(),
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {

    /// Every registered adapter must appear in the capability list.
    ///
    /// `is_enabled` ends in `.unwrap_or(false)`, so an adapter whose id is not
    /// in `ADAPTER_CAPABILITY_FLAGS` is silently OFF: it compiles, it
    /// registers, it detects, and it never appears to the person -- with no
    /// error anywhere saying why. A sixth adapter added without a flag entry
    /// would be invisible and the build would stay green.
    ///
    /// The two lists agree today (checked when this was written); this is
    /// about the next one. Compared against the REAL registry rather than a
    /// hardcoded list, so it cannot drift from what actually ships.
    /// A conversation whose original recorded no date must not be shown with
    /// a fabricated one.
    ///
    /// The adapters substitute a 1970 sentinel when a file records nothing.
    /// Copied onto the session header it becomes the age in the sidebar,
    /// which renders it as "56y" -- a confident measurement of something
    /// GitWyrm was never told. When GitWyrm has no date from the original,
    /// the one honest date it does have is when it imported the thing.
    #[test]
    fn a_conversation_with_no_recorded_date_is_dated_when_it_was_imported() {
        assert_eq!(
            imported_session_updated_at(NO_RECORDED_DATE, "2026-09-09T12:00:00Z"),
            "2026-09-09T12:00:00Z"
        );
    }

    /// A date the original really did record is used exactly as found.
    #[test]
    fn a_recorded_date_is_kept_as_it_was_found() {
        assert_eq!(
            imported_session_updated_at("2026-01-05T08:30:00Z", "2026-09-09T12:00:00Z"),
            "2026-01-05T08:30:00Z"
        );
    }

   #[test]
    fn every_registered_adapter_has_a_capability_flag() {
        use crate::agentdesk::adapters::AdapterRegistry;
        let registry = AdapterRegistry::with_default_adapters();
        let flagged: std::collections::BTreeSet<&str> =
            super::ADAPTER_CAPABILITY_FLAGS.iter().map(|(id, _)| *id).collect();

        let unflagged: Vec<&str> = registry
            .ids()
            .into_iter()
            .filter(|id| !flagged.contains(id))
            .collect();
        assert!(
            unflagged.is_empty(),
            "these adapters would be silently hidden with no error: {unflagged:?}"
        );

        // And the other direction: a flag naming no adapter is dead config
        // that reads as a deliberate decision about something that is gone.
        let registered: std::collections::BTreeSet<&str> = registry.ids().into_iter().collect();
        let orphaned: Vec<&str> = flagged
            .iter()
            .copied()
            .filter(|id| !registered.contains(id))
            .collect();
        assert!(orphaned.is_empty(), "these flags name no adapter: {orphaned:?}");
    }

    /// A first import whose ledger note could not be saved must SAY so.
    ///
    /// `already_imported_session`'s doc says a lost ledger "degrades to
    /// re-checks messages it already has, never to silently duplicates". That
    /// holds for a refresh -- message provenance catches the messages. It does
    /// NOT hold for a first import: the create path consults only the ledger,
    /// so a lost write means the next import builds a SECOND chat with a new
    /// id, never reading the first. Logging alone never reached the person.
    ///
    /// Pinned on the note's wording rather than the whole import flow, which
    /// needs an adapter fixture: the wording IS the fix, and it is what a
    /// person reads.
    #[test]
    fn the_lost_ledger_note_says_what_happens_next_time() {
        let note = super::LEDGER_NOT_SAVED_NOTE;
        assert!(note.contains("second copy"), "must say what a re-import will do");
        assert!(note.contains("brought in"), "must confirm the chat itself was saved");
        // Plain language: no internal words for the thing that failed.
        for jargon in ["ledger", "bookkeeping", "adapter", "serialize"] {
            assert!(!note.contains(jargon), "note leaks the word {jargon:?}");
        }
    }

    /// Adapter ids and provider ids are different namespaces, and an import
    /// must not put one where the other is expected.
    ///
    /// `preferred_provider` on a session header is an instruction for the next
    /// run: it becomes the provider override when someone continues the chat.
    /// Import used to set it from the adapter id, and only `codex` and
    /// `opencode` appear in both namespaces -- so continuing an imported
    /// Claude Code, Copilot or OpenChamber chat refused with
    /// `UnsupportedProvider` before it could start.
    ///
    /// This test pins the overlap so the two id sets cannot quietly converge
    /// and make the old bug look harmless again.
    #[test]
    fn adapter_ids_are_not_provider_ids() {
        use crate::agentdesk::policy::ExecutionProvider;
        for id in ["claude-code", "vscode-copilot", "openchamber"] {
            assert!(
                ExecutionProvider::parse(id).is_none(),
                "{id} is an adapter id, not a provider id -- if this now parses, \
                 revisit why import stopped setting preferred_provider from it"
            );
        }
        // The two that do overlap, recorded so the distinction stays visible.
        assert!(ExecutionProvider::parse("codex").is_some());
        assert!(ExecutionProvider::parse("opencode").is_some());
    }
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

    fn write_linked_session(root: &SessionStoreRoot, session_id: &str, external_id: &str) {
        let detail = sample_detail(
            external_id,
            vec![
                user_msg("m1", "2026-01-01T00:00:00Z", "hello"),
                assistant_msg("m2", "2026-01-01T00:00:01Z", "hi back"),
            ],
        );
        let session = build_imported_session(
            session_id,
            "repo-1",
            "C:/code/fixture-project",
            "fixture-project",
            "codex",
            external_id,
            &detail,
            "2026-01-01T00:00:05Z",
        );
        store::write_session(root, &session).unwrap();
        let mut ledger = import_store::read_ledger(root, "codex");
        ledger.sessions.insert(
            external_id.to_string(),
            ImportedSessionRecord {
                gitwyrm_session_id: session_id.to_string(),
                last_imported_external_message_id: "m2".into(),
                last_seen_external_updated_at: "2026-01-01T00:00:02Z".into(),
            },
        );
        import_store::write_ledger(root, "codex", &ledger).unwrap();
    }

    #[test]
    fn unlink_removes_the_ledger_link_and_keeps_every_imported_message() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        write_linked_session(&root, "sess-unlink-1", "ext-1");

        let outcome = unlink_at(&locks, &root, "sess-unlink-1");
        let UnlinkOutcome::Unlinked {
            session,
            adapter_id,
            adapter_display_name,
        } = outcome
        else {
            panic!("expected Unlinked, got {outcome:?}");
        };
        assert_eq!(adapter_id, "codex");
        assert_eq!(adapter_display_name, "Codex");

        // The link is gone from the ledger...
        assert!(import_store::find_link_for_session(&root, "sess-unlink-1").is_none());
        let ledger = import_store::read_ledger(&root, "codex");
        assert!(!ledger.sessions.contains_key("ext-1"));

        // ...but the imported messages, with their provenance, are still there,
        // followed by the visible unlink note in its own segment.
        let imported: Vec<_> = session.messages.iter().filter(|m| m.import.is_some()).collect();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].plain_content, "hello");
        assert_eq!(imported[1].plain_content, "hi back");
        let note = session.messages.last().unwrap();
        assert!(note.import.is_none());
        assert!(note.plain_content.contains("no longer linked to Codex"));
        assert_eq!(session.segments.last().unwrap().label, "Unlinked from Codex");

        let back = store::read_session(&root, "sess-unlink-1").unwrap();
        assert_eq!(back, session);
    }

    #[test]
    fn unlinking_twice_reports_not_linked_without_touching_the_session_again() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        write_linked_session(&root, "sess-unlink-2", "ext-2");

        assert!(matches!(
            unlink_at(&locks, &root, "sess-unlink-2"),
            UnlinkOutcome::Unlinked { .. }
        ));
        let after_first = store::read_session(&root, "sess-unlink-2").unwrap();

        assert!(matches!(
            unlink_at(&locks, &root, "sess-unlink-2"),
            UnlinkOutcome::NotLinked
        ));
        let after_second = store::read_session(&root, "sess-unlink-2").unwrap();
        assert_eq!(after_first, after_second, "a NotLinked outcome must not add a second note");
    }

    #[test]
    fn unlinking_a_missing_session_is_not_found() {
        let (_dir, root) = temp_root();
        let locks = test_locks();
        assert!(matches!(
            unlink_at(&locks, &root, "does-not-exist"),
            UnlinkOutcome::NotFound
        ));
    }

    #[test]
    fn a_scan_row_carries_the_linked_gitwyrm_session_id() {
        let (_dir, root) = temp_root();
        write_linked_session(&root, "sess-scan-1", "ext-scan");
        let ledger = import_store::read_ledger(&root, "codex");
        let record = import_store::already_imported_session(&ledger, "ext-scan");
        assert_eq!(
            record.map(|r| r.gitwyrm_session_id.as_str()),
            Some("sess-scan-1")
        );
        assert!(import_store::already_imported_session(&ledger, "ext-other").is_none());
    }
}
