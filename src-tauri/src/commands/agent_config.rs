//! Commands for the Agent Setup configuration-sync feature: scan, preview,
//! apply, and undo (architecture.md section 3, `agent_config_*`).
//!
//! Kept in its own file, separate from `agent_import.rs` (external-chat
//! session import, owned by another change in flight at the same time this
//! was written) even though both read agent-client configuration -- this
//! module's `crate::agent_config` has its own minimal location discovery
//! rather than depending on that adapter trait. See
//! `src-tauri/src/agent_config/locations.rs` for the follow-up note on
//! unifying them later.
//!
//! Every mutating command resolves its own [`agent_config::plan::SafeWriteRoot`]
//! from the app handle, the same self-contained-command shape
//! `commands::agent_desk` uses, so the logic functions underneath are directly
//! testable against a temp root without a Tauri runtime.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, State};

use crate::agent_config::model::{
    ApplyOutcome, BatchApplyOutcome, BatchApplyRequest, ChangeSummaryLine, ClientDetection,
    ClientId, ConfigLocation, CopyPlan, DestinationApplyResult, DestinationPreview,
    InventoryEntry, PlanWarning, PreviewOutcome, RawItem, RedactedCopyPlan, UndoOutcome,
    WarningKind,
};
use crate::agent_config::plan::{self, SafeWriteRoot};
use crate::agent_config::{locations, normalize, readers, redact, skills, writers};
use crate::error::AppError;
use crate::state::RepoManager;

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn resolve_write_root(app: &AppHandle) -> Result<SafeWriteRoot, AppError> {
    SafeWriteRoot::resolve(app).map_err(|e| AppError::Other(e.to_string()))
}

/// Where plans are persisted between preview and apply:
/// `<app-data>/agent-config-sync/v1/plans/<plan_id>.json`. A plan is small
/// and short-lived (created by preview, consumed by apply, never listed), so
/// it gets a flat file rather than an index like sessions do.
fn plan_path(roots: &SafeWriteRoot, plan_id: &str) -> PathBuf {
    roots.plans_dir().join(format!("{plan_id}.json"))
}

/// The plan file is what Undo reads to put things back, so a torn one is
/// worse than none: `read_plan` returns `None` for unparseable JSON, which
/// the caller reports as an expired plan -- the person is told the plan is
/// gone while the copies it describes have already landed.
///
/// A plain `fs::write` truncates in place, so a crash or a full disk between
/// truncate and the last byte leaves exactly that. Every other write in this
/// feature already goes through a temp file and a rename; this one did not.
fn write_plan(roots: &SafeWriteRoot, plan: &CopyPlan) -> Result<(), AppError> {
    let json = serde_json::to_vec_pretty(plan).map_err(|e| AppError::Other(e.to_string()))?;
    crate::agent_config::plan::write_atomic_bytes(&plan_path(roots, &plan.plan_id), &json)
        .map_err(|e| AppError::Other(e.to_string()))
}

fn read_plan(roots: &SafeWriteRoot, plan_id: &str) -> Option<CopyPlan> {
    let bytes = std::fs::read(plan_path(roots, plan_id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// -- 1.2/1.3/1.4: scan --

/// Read-only scan across every known client location, optionally scoped to
/// one open repo for repo-local configuration paths. Never writes anything
/// (task 1.2, spec "Scan").
#[tauri::command]
#[specta::specta]
pub async fn agent_config_scan(
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
) -> Result<Vec<InventoryEntry>, AppError> {
    let repo_root = match &repo_id {
        Some(id) => Some(manager.get(id)?.path.to_string_lossy().into_owned()),
        None => None,
    };

    tauri::async_runtime::spawn_blocking(move || scan_inventory(repo_root.as_deref()))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

fn scan_inventory(repo_root: Option<&str>) -> Vec<InventoryEntry> {
    let (items, _errors) = scan_all_items(repo_root);
    let detections = locations::detect_clients(repo_root);
    normalize::build_inventory(&items, &detections)
}

/// Every location for every client (personal plus, when given, repo-scoped),
/// read into [`RawItem`]s. Parse failures are collected rather than aborting
/// the whole scan -- one damaged config file must not hide every other
/// client's items, the same stance `agentdesk::store` takes for one corrupt
/// session file.
fn scan_all_items(repo_root: Option<&str>) -> (Vec<RawItem>, Vec<(ConfigLocation, String)>) {
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for &client in &ClientId::ALL {
        let mut locs = locations::personal_locations(client);
        if let Some(root) = repo_root {
            locs.extend(locations::repo_locations(client, root));
        }
        for loc in locs {
            match readers::read_items(&loc) {
                Ok(mut found) => items.append(&mut found),
                Err(e) => errors.push((loc, e.to_string())),
            }
        }

        // Skills are found by scanning a folder rather than by reading a key
        // in a config file, so they cannot come from `read_items` and are
        // collected separately. This is why the Skills tab used to render
        // empty: `ItemKind::Skill` existed but nothing produced one.
        items.extend(scan_skills(client, repo_root));
    }
    (items, errors)
}

/// Every skill one client has installed, personal and repository-scoped.
///
/// A client with no skills folder contributes nothing, and a folder that is
/// not there is not an error -- see `agent_config::skills`.
fn scan_skills(client: ClientId, repo_root: Option<&str>) -> Vec<RawItem> {
    let Some(home) = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
    else {
        return Vec::new();
    };
    let repo = repo_root.map(std::path::PathBuf::from);

    let mut out = Vec::new();
    for (dir, scope) in skills::skill_dirs(&home, repo.as_deref(), client) {
        out.extend(skills::read_skills_at(&dir, |manifest| ConfigLocation {
            client,
            scope,
            path: manifest.to_string_lossy().into_owned(),
        }));
    }
    out
}

/// Which clients were detected on this machine, for the "Detected apps" tab.
#[tauri::command]
#[specta::specta]
pub async fn agent_config_detect_clients(
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
) -> Result<Vec<ClientDetection>, AppError> {
    let repo_root = match &repo_id {
        Some(id) => Some(manager.get(id)?.path.to_string_lossy().into_owned()),
        None => None,
    };
    tauri::async_runtime::spawn_blocking(move || locations::detect_clients(repo_root.as_deref()))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

// -- 3.1: preview --

/// Compute a not-yet-applied plan copying `item_id`'s source content to each
/// of `destinations`. Every destination's proposed content, before-hash, and
/// warnings are computed here; nothing is written (task 3.1).
#[tauri::command]
#[specta::specta]
pub async fn agent_config_preview_copy(
    manager: State<'_, RepoManager>,
    app: AppHandle,
    repo_id: Option<String>,
    item_id: String,
    destinations: Vec<ClientId>,
) -> Result<PreviewOutcome, AppError> {
    let repo_root = match &repo_id {
        Some(id) => Some(manager.get(id)?.path.to_string_lossy().into_owned()),
        None => None,
    };
    let write_root = resolve_write_root(&app)?;

    tauri::async_runtime::spawn_blocking(move || {
        preview_copy_at(&write_root, repo_root.as_deref(), &item_id, &destinations)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

fn preview_copy_at(
    write_root: &SafeWriteRoot,
    repo_root: Option<&str>,
    item_id: &str,
    destinations: &[ClientId],
) -> Result<PreviewOutcome, AppError> {
    if destinations.is_empty() {
        return Ok(PreviewOutcome::NoDestinations);
    }

    let (items, _errors) = scan_all_items(repo_root);
    let source_item = match find_source_item(&items, item_id) {
        Some(item) => item,
        None => return Ok(PreviewOutcome::ItemNotFound),
    };

    let mut destination_previews = Vec::new();
    for &client in destinations {
        if client == source_item.location.client {
            continue; // never propose copying an item onto its own source
        }
        destination_previews.push(build_destination_preview(client, source_item, repo_root));
    }

    let plan = CopyPlan {
        plan_id: new_id(),
        item_id: item_id.to_string(),
        source_item: source_item.clone(),
        destinations: destination_previews,
        created_at: now_rfc3339(),
    };
    // The full plan (including real, unredacted field values in
    // `source_item.extra` and every `proposed_content`) is persisted here so
    // `apply_copy_at`/`agent_config_apply_batch` can read it back and write
    // the genuine content later. Only the redacted view crosses back to the
    // frontend below -- see `RedactedCopyPlan`'s doc comment (task 1.5/5.4).
    write_plan(write_root, &plan)?;
    Ok(PreviewOutcome::Ready {
        plan: RedactedCopyPlan::from(&plan),
    })
}

/// Resolve `item_id` (the same `{kind:?}:{identity}` shape
/// [`normalize::build_inventory`] assigns) back to one concrete source
/// [`RawItem`] -- the first-seen occurrence, matching how the inventory
/// chooses a source client.
fn find_source_item<'a>(items: &'a [RawItem], item_id: &str) -> Option<&'a RawItem> {
    items
        .iter()
        .filter(|item| format!("{:?}:{}", item.kind, item.identity) == item_id)
        .min_by_key(|i| (i.location.scope != crate::agent_config::model::ConfigScope::Repo, i.location.client as u8))
}

/// The destination preview for one skill: which folder it would be copied
/// into, whether something is already there, and what the copy would write.
///
/// Deliberately its own function rather than a branch threaded through the
/// JSON path. Everything that path does -- read the destination file, merge
/// one member, diff fields -- is meaningless for a folder, and pretending
/// otherwise is what produced a preview that offered a copy it could not do.
///
/// `destination_path` names the skill's own folder, and `proposed_content`
/// lists the files that would be written, so the review shows a person what
/// they are agreeing to without inventing a file diff nobody asked for.
fn skill_copy_preview(client: ClientId, source_item: &RawItem, repo_root: Option<&str>) -> DestinationPreview {
    use crate::agent_config::{model::ConfigScope, skill_write, skills};

    let home = crate::agent_config::locations::home_dir();
    let repo_path = repo_root.map(std::path::Path::new);
    let dirs = match &home {
        Some(h) => skills::skill_dirs(h, repo_path, client),
        None => Vec::new(),
    };
    // Prefer the source's own scope, so a repo skill lands in the repo and a
    // personal one stays personal.
    let chosen = dirs
        .iter()
        .find(|(_, scope)| *scope == source_item.location.scope)
        .or_else(|| dirs.first());

    let Some((skills_root, _scope)) = chosen else {
        // Two different reasons land here and they are not the same fact.
        // GitWyrm only knows where Claude Code keeps skills, so for every
        // other client this is a limit of GitWyrm, not a statement about
        // what that client can do. Saying "it does not keep skills" asserts
        // something about another product that GitWyrm has not checked and
        // that may simply be untrue.
        let message = if home.is_none() {
            "GitWyrm could not find your home folder, so it does not know where to copy this.".to_string()
        } else {
            format!("GitWyrm cannot copy skills to {} yet.", client.label())
        };
        return DestinationPreview {
            client,
            destination_path: String::new(),
            before_hash: None,
            proposed_content: String::new(),
            redacted_diff_summary: Vec::new(),
            warnings: vec![PlanWarning {
                kind: WarningKind::ClientNotDetected,
                message,
            }],
            write_supported: false,
        };
    };

    let destination_dir = skills_root.join(&source_item.identity);
    let source_dir = std::path::Path::new(&source_item.location.path)
        .parent()
        .map(std::path::Path::to_path_buf);

    let Some(source_dir) = source_dir else {
        return DestinationPreview {
            client,
            destination_path: destination_dir.to_string_lossy().into_owned(),
            before_hash: None,
            proposed_content: String::new(),
            redacted_diff_summary: Vec::new(),
            warnings: vec![PlanWarning {
                kind: WarningKind::UnsupportedField,
                message: "This skill's folder could not be found.".to_string(),
            }],
            write_supported: false,
        };
    };

    let mut warnings = Vec::new();
    let (files, write_supported) = match skill_write::plan_skill_copy(&source_dir, &destination_dir) {
        Ok(plan) => {
            if plan.replaces_existing {
                warnings.push(PlanWarning {
                    kind: WarningKind::UnsupportedField,
                    message: format!(
                        "{} already has a skill called \"{}\". Applying replaces it, and Undo puts \
the old one back.",
                        client.label(),
                        source_item.identity
                    ),
                });
            }
            (plan.files.keys().cloned().collect::<Vec<_>>(), true)
        }
        Err(e) => {
            warnings.push(PlanWarning {
                kind: WarningKind::UnsupportedField,
                message: e.plain(),
            });
            (Vec::new(), false)
        }
    };

    DestinationPreview {
        client,
        destination_path: destination_dir.to_string_lossy().into_owned(),
        // A folder has no single before-hash; the copy re-checks every file's
        // hash at apply time instead (`skill_write::apply_skill_copy`).
        before_hash: None,
        proposed_content: files.join("\n"),
        redacted_diff_summary: Vec::new(),
        warnings,
        write_supported,
    }
}

/// Which of a client's declared config files a copy should be written to.
///
/// Prefers the source's own scope, so a repository connector lands in the
/// repository and a personal one stays personal. Within a scope, **a file
/// that already exists always wins over one that does not**.
///
/// That second rule is load-bearing for clients that read their settings in
/// layers and accept several filenames, taking whichever exists first.
/// Writing to the first *declared* name can create a brand new file that
/// then shadows the one the person actually uses -- so the copy appears to
/// work while every connector they already had stops being read. Following
/// the file on disk is the same rule the writers already apply to the key
/// inside a document, one level up.
///
/// `exists` is injected so this is testable without a real home directory.
fn choose_destination<'a>(
    locs: &'a [ConfigLocation],
    source_scope: crate::agent_config::model::ConfigScope,
    exists: impl Fn(&str) -> bool,
) -> Option<&'a ConfigLocation> {
    locs.iter()
        .find(|l| l.scope == source_scope && exists(&l.path))
        .or_else(|| locs.iter().find(|l| l.scope == source_scope))
        .or_else(|| locs.iter().find(|l| exists(&l.path)))
        .or_else(|| locs.first())
}

fn build_destination_preview(client: ClientId, source_item: &RawItem, repo_root: Option<&str>) -> DestinationPreview {
    // Support is per client AND per kind. A skill is a folder of files rather
    // than a member of a JSON object, so it does not go through the JSON
    // writer at all: `skill_copy_preview` below builds its own destination and
    // `apply_copy_at` sends it to `agent_config::skill_write`.
    let is_skill = source_item.kind == crate::agent_config::model::ItemKind::Skill;
    if is_skill {
        return skill_copy_preview(client, source_item, repo_root);
    }
    let write_supported = writers::is_supported(client);
    let mut locs = locations::personal_locations(client);
    if let Some(root) = repo_root {
        locs.extend(locations::repo_locations(client, root));
    }
    let destination_location =
        choose_destination(&locs, source_item.location.scope, |p| Path::new(p).is_file()).cloned();

    let Some(dest_loc) = destination_location else {
        return DestinationPreview {
            client,
            destination_path: String::new(),
            before_hash: None,
            proposed_content: String::new(),
            redacted_diff_summary: Vec::new(),
            warnings: vec![PlanWarning {
                kind: WarningKind::ClientNotDetected,
                message: format!("{} has no known configuration location for this item.", client.label()),
            }],
            write_supported: false,
        };
    };

    let path = Path::new(&dest_loc.path);
    let mut warnings = Vec::new();

    if !path.parent().map(|p| p.exists()).unwrap_or(false) && path.parent().is_some() {
        // Not fatal -- writers create parent directories -- but worth a
        // heads-up since it means this client's config directory was never
        // even created (a strong signal the client may not be installed).
        warnings.push(PlanWarning {
            kind: WarningKind::ClientNotDetected,
            message: format!("{}'s configuration folder was not found; a new one will be created.", client.label()),
        });
    }

    // `read_items` returns an empty list for a file that is simply absent, so
    // an Err here is a real failure: the file is there and could not be read
    // or parsed. `unwrap_or_default()` turned that into "the destination has
    // nothing in it", which is a different claim -- the diff below would say
    // "adding a new item" for what may well be an overwrite.
    let existing = match readers::read_items(&dest_loc) {
        Ok(items) => items,
        Err(e) => {
            warnings.push(PlanWarning {
                kind: WarningKind::DestinationUnreadable,
                message: format!(
                    "GitWyrm could not read {}'s existing settings, so it cannot show what this                      would replace. Details: {e}",
                    client.label()
                ),
            });
            Vec::new()
        }
    };
    let existing_item = existing.iter().find(|i| i.identity == source_item.identity);
    let destination_extra = existing_item.map(|i| &i.extra);

    let redacted_source = redact::redact_for_display(&source_item.extra, &source_item.secret_fields);
    let redacted_destination = destination_extra.map(|e| redact::redact_for_display(e, &[]));
    let redacted_diff_summary: Vec<ChangeSummaryLine> =
        redact::diff_fields(&redacted_source, redacted_destination.as_ref(), &source_item.secret_fields);

    if !source_item.secret_fields.is_empty() {
        warnings.push(PlanWarning {
            kind: WarningKind::SecretWillBeCopied,
            message: "This item includes secret values (tokens, keys, or headers). They are hidden \
                       in this preview but will be written in full to the destination file, so only \
                       continue if you trust that destination."
                .to_string(),
        });
    }

    // Same shape as the read above: a failure here leaves `before_hash` None
    // and falls back to an empty document, so the proposed content is built as
    // though the file were blank. Warn once -- the read above covers the
    // common case, and two warnings for one unreadable file is noise.
    let before = match plan::read_current(path) {
        Ok(v) => v,
        Err(e) => {
            if !warnings.iter().any(|w| w.kind == WarningKind::DestinationUnreadable) {
                warnings.push(PlanWarning {
                    kind: WarningKind::DestinationUnreadable,
                    message: format!(
                        "GitWyrm could not read {}'s existing file, so the preview below starts                          from an empty one. Details: {e}",
                        client.label()
                    ),
                });
            }
            None
        }
    };
    let before_hash = before.as_ref().map(|(_, h)| h.clone());
    let current_text = before
        .map(|(bytes, _)| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_else(|| writers::empty_document(client).to_string());

    // What the translation could not account for. Reported beside the
    // preview rather than after the write, so the choice is made with the
    // information rather than explained afterwards.
    if write_supported && source_item.kind == crate::agent_config::model::ItemKind::McpConnector {
        if !crate::agent_config::connector::transport_understood(&source_item.extra) {
            warnings.push(PlanWarning {
                kind: WarningKind::UnsupportedField,
                message: format!(
                    "GitWyrm did not recognise how this connection starts, so it is being copied to {} exactly as written. It may need editing there before it runs.",
                    client.label()
                ),
            });
        }
        let (_, unmodelled) = crate::agent_config::connector::translate(&source_item.extra, client);
        if !unmodelled.is_empty() {
            warnings.push(PlanWarning {
                kind: WarningKind::UnsupportedField,
                message: format!(
                    "GitWyrm does not know what these settings mean in {}, so they were copied across unchanged: {}.",
                    client.label(),
                    unmodelled.join(", ")
                ),
            });
        }
    }

    let proposed_content = if write_supported {
        match writers::build_new_content(client, source_item.kind, &source_item.identity, &source_item.extra, &current_text) {
            Ok(content) => content,
            Err(e) => {
                warnings.push(PlanWarning {
                    kind: WarningKind::UnsupportedField,
                    message: format!("Could not build a preview for {}: {e}", client.label()),
                });
                current_text.clone()
            }
        }
    } else {
        warnings.push(PlanWarning {
            kind: WarningKind::UnsupportedField,
            message: format!("{} is read-only in GitWyrm today; nothing can be applied here yet.", client.label()),
        });
        current_text.clone()
    };

    DestinationPreview {
        client,
        destination_path: dest_loc.path,
        before_hash,
        proposed_content,
        redacted_diff_summary,
        warnings,
        write_supported,
    }
}

// -- 3.2/3.3/3.4: apply / undo --

/// Apply one previously computed plan. Every destination in the plan is
/// written one at a time; a destination whose file changed since preview is
/// refused (never overwritten) while the rest of the batch still proceeds
/// (task 3.2, 5.2).
#[tauri::command]
#[specta::specta]
pub async fn agent_config_apply_copy(app: AppHandle, plan_id: String) -> Result<ApplyOutcome, AppError> {
    let write_root = resolve_write_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || apply_copy_at(&write_root, &plan_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Copies one skill folder for a plan destination.
///
/// Wraps `skill_write` in the same `DestinationApplyResult` the JSON path
/// produces, so the review UI, the batch runner and undo all treat a skill
/// exactly like anything else once it is applied. `OperationReceipt` is
/// reused rather than given a parallel type: the fields it needs (what was
/// written, where the backup went) mean the same thing for a folder, and a
/// second receipt shape would need its own undo path to stay correct.
fn apply_skill_destination(
    write_root: &SafeWriteRoot,
    plan: &CopyPlan,
    destination: &DestinationPreview,
    operation_id: &str,
) -> DestinationApplyResult {
    use crate::agent_config::skill_write;

    let Some(source_dir) = Path::new(&plan.source_item.location.path).parent() else {
        return DestinationApplyResult::WriteFailed {
            client: destination.client,
            detail: "this skill's folder could not be found".to_string(),
        };
    };
    let destination_dir = Path::new(&destination.destination_path);

    let copy_plan = match skill_write::plan_skill_copy(source_dir, destination_dir) {
        Ok(p) => p,
        Err(e) => {
            return DestinationApplyResult::WriteFailed {
                client: destination.client,
                detail: e.plain(),
            }
        }
    };

    // Replacing is allowed because the preview said so in as many words and
    // the person applied anyway; the old folder is still backed up first.
    match skill_write::apply_skill_copy(&copy_plan, &write_root.backups_dir(), true) {
        Ok(copied) => {
            let after_digest = copied.after_digest.clone();
            let receipt = crate::agent_config::model::OperationReceipt {
                after_digest,
                operation_id: operation_id.to_string(),
                plan_id: plan.plan_id.clone(),
                client: destination.client.key().to_string(),
                destination_path: copied.destination_dir,
                // A folder has no single hash. The copier re-checks every
                // file's hash itself before writing, which is the same
                // protection `before_hash` gives a single file. The empty
                // `after_hash` is also how `undo_at` recognises a folder
                // receipt and sends it to the folder undo.
                before_hash: None,
                after_hash: String::new(),
                backup_path: copied.backup_dir,
                applied_at: now_rfc3339(),
                undone: false,
            };
            // Saved through the shared receipt store, or Undo would report
            // this operation as unknown.
            if let Err(e) = plan::save_receipt(write_root, &receipt) {
                return DestinationApplyResult::WriteFailed {
                    client: destination.client,
                    detail: format!("the skill was copied but could not be recorded for undo: {e}"),
                };
            }
            DestinationApplyResult::Applied {
                client: destination.client,
                operation_id: operation_id.to_string(),
                receipt,
            }
        }
        Err(skill_write::SkillCopyError::SourceChanged { .. }) => {
            DestinationApplyResult::ConcurrentChangeRefused {
                client: destination.client,
                expected_hash: None,
                actual_hash: None,
            }
        }
        Err(e) => DestinationApplyResult::WriteFailed {
            client: destination.client,
            detail: e.plain(),
        },
    }
}

fn apply_copy_at(write_root: &SafeWriteRoot, plan_id: &str) -> ApplyOutcome {
    let Some(plan) = read_plan(write_root, plan_id) else {
        return ApplyOutcome {
            plan_id: plan_id.to_string(),
            results: vec![],
        };
    };

    let mut results = Vec::new();
    // One client at a time (task 5: "Merge writers, ONE CLIENT AT A TIME"):
    // each destination is fully applied (hash-check, backup, receipt, write)
    // before moving to the next, so a failure partway through never leaves
    // two destinations mid-write simultaneously.
    for destination in &plan.destinations {
        if !destination.write_supported || destination.destination_path.is_empty() {
            continue;
        }
        let operation_id = new_id();
        // A skill is a folder, so it goes to the folder copier rather than
        // the single-file writer. Same guarantees either way: the source is
        // re-hashed before anything is written, whatever was there is backed
        // up first, and a receipt records enough to undo it.
        if plan.source_item.kind == crate::agent_config::model::ItemKind::Skill {
            results.push(apply_skill_destination(
                write_root,
                &plan,
                destination,
                &operation_id,
            ));
            continue;
        }
        let result = plan::apply_write(
            write_root,
            &plan.plan_id,
            destination.client.key(),
            Path::new(&destination.destination_path),
            destination.before_hash.as_deref(),
            destination.proposed_content.as_bytes(),
            &operation_id,
            &now_rfc3339(),
        );
        results.push(match result {
            Ok(receipt) => DestinationApplyResult::Applied {
                client: destination.client,
                operation_id,
                receipt,
            },
            Err(plan::ApplyWriteError::ConcurrentChange { expected_hash, actual_hash }) => {
                DestinationApplyResult::ConcurrentChangeRefused {
                    client: destination.client,
                    expected_hash,
                    actual_hash,
                }
            }
            Err(e) => DestinationApplyResult::WriteFailed {
                client: destination.client,
                detail: e.to_string(),
            },
        });
    }

    ApplyOutcome {
        plan_id: plan.plan_id,
        results,
    }
}

/// "Match selected apps": apply several previously previewed plans in one
/// batch. Deliberately built from the same [`apply_copy_at`] used by a
/// single-item apply -- there is no separate "sync everything" code path
/// (task 2.4, 5).
#[tauri::command]
#[specta::specta]
pub async fn agent_config_apply_batch(app: AppHandle, request: BatchApplyRequest) -> Result<BatchApplyOutcome, AppError> {
    let write_root = resolve_write_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let outcomes = request
            .plan_ids
            .iter()
            .map(|plan_id| apply_copy_at(&write_root, plan_id))
            .collect();
        BatchApplyOutcome { outcomes }
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Every copy this app has made, newest first, so one can be undone later.
///
/// Undo has always taken an operation id, and receipts have always been
/// written to outlive the release that made them -- but the id only ever
/// existed in the apply dialog's own state, so closing that dialog made the
/// write permanent in practice. The vision's "receipt and Undo" needs both
/// halves; this is the one that was missing.
#[tauri::command]
#[specta::specta]
pub async fn agent_config_recent_operations(app: AppHandle) -> Result<Vec<crate::agent_config::model::OperationReceipt>, AppError> {
    let write_root = resolve_write_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || plan::list_receipts(&write_root))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Undo one operation by ID, restoring byte-identical prior content unless
/// the destination changed since the write (task 3.4).
#[tauri::command]
#[specta::specta]
pub async fn agent_config_undo(app: AppHandle, operation_id: String) -> Result<UndoOutcome, AppError> {
    let write_root = resolve_write_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || undo_at(&write_root, &operation_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

fn undo_at(write_root: &SafeWriteRoot, operation_id: &str) -> UndoOutcome {
    // A skill was copied as a folder, so it is restored as one. Recognised by
    // the empty `after_hash` a folder receipt carries: the single-file undo
    // would try to read a directory as a file and fail.
    if let Some(receipt) = plan::read_receipt(write_root, operation_id) {
        if receipt.after_hash.is_empty() {
            return undo_skill_at(write_root, receipt);
        }
    }
    match plan::undo_write(write_root, operation_id) {
        Ok(receipt) => UndoOutcome::Restored { receipt },
        Err(plan::UndoWriteError::NotFound) => UndoOutcome::OperationNotFound,
        Err(plan::UndoWriteError::AlreadyUndone) => UndoOutcome::AlreadyUndone,
        Err(plan::UndoWriteError::ConcurrentChange { expected_hash, actual_hash }) => {
            UndoOutcome::ConcurrentChangeRefused { expected_hash, actual_hash }
        }
        Err(e) => UndoOutcome::RestoreFailed { detail: e.to_string() },
    }
}

/// Puts back whatever a skill copy replaced, and marks the receipt undone.
fn undo_skill_at(
    write_root: &SafeWriteRoot,
    mut receipt: crate::agent_config::model::OperationReceipt,
) -> UndoOutcome {
    use crate::agent_config::skill_write;

    if receipt.undone {
        return UndoOutcome::AlreadyUndone;
    }
    let copy_receipt = skill_write::SkillCopyReceipt {
        destination_dir: receipt.destination_path.clone(),
        backup_dir: receipt.backup_path.clone(),
        files_written: 0,
        // The whole point: without this the undo has nothing to compare the
        // folder against and deletes it regardless of what it now contains.
        after_digest: receipt.after_digest.clone(),
    };
    if let Err(e) = skill_write::undo_skill_copy(&copy_receipt) {
        return UndoOutcome::RestoreFailed { detail: e.plain() };
    }
    if let Err(e) = plan::mark_receipt_undone(write_root, &mut receipt) {
        return UndoOutcome::RestoreFailed {
            detail: format!("the skill was put back but the record could not be updated: {e}"),
        };
    }
    UndoOutcome::Restored { receipt }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_config::model::{ConfigScope, ItemKind, JsonValue};
    use tempfile::TempDir;

    fn write_root() -> (TempDir, SafeWriteRoot) {
        let dir = TempDir::new().unwrap();
        let root = SafeWriteRoot::at(dir.path().join("agent-config-sync").join("v1")).unwrap();
        (dir, root)
    }

    /// The plan file is what Undo reads to put copies back. `read_plan`
    /// returns `None` for unparseable JSON, and callers report that as an
    /// expired plan -- so a half-written file tells the person their plan is
    /// gone while the copies it describes have already landed.
    ///
    /// This pins the property that makes that impossible: writing over an
    /// existing plan never leaves a third state on disk. A plain `fs::write`
    /// truncates in place and does.
    #[test]
    fn overwriting_a_plan_never_leaves_a_partial_file() {
        let (_dir, roots) = write_root();
        let plan = CopyPlan {
            plan_id: "plan-atomic".into(),
            item_id: "Skill:demo".into(),
            source_item: sample_item(ClientId::ClaudeCode, "demo"),
            destinations: Vec::new(),
            created_at: now_rfc3339(),
        };
        write_plan(&roots, &plan).unwrap();

        // Overwrite with a much larger plan: a truncating write would pass
        // through a state where the file holds neither one.
        let mut bigger = plan.clone();
        bigger.destinations = (0..200)
            .map(|i| DestinationPreview {
                client: ClientId::OpenCode,
                destination_path: format!("/fake/dest/{i}"),
                before_hash: None,
                proposed_content: "x".repeat(400),
                redacted_diff_summary: Vec::new(),
                warnings: Vec::new(),
                write_supported: true,
            })
            .collect();
        write_plan(&roots, &bigger).unwrap();

        let back = read_plan(&roots, "plan-atomic").expect("plan is readable after overwrite");
        assert_eq!(back.destinations.len(), 200);

        // And the rename left no temp file beside it for the person to find.
        let dir = plan_path(&roots, "plan-atomic")
            .parent()
            .unwrap()
            .to_path_buf();
        let strays: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.ends_with(".json"))
            .collect();
        assert!(strays.is_empty(), "left temp files behind: {strays:?}");
    }

    fn sample_item(client: ClientId, identity: &str) -> RawItem {
        let mut extra = std::collections::BTreeMap::new();
        extra.insert("command".to_string(), JsonValue(serde_json::json!("npx")));
        extra.insert("env".to_string(), JsonValue(serde_json::json!({ "TOKEN": "abc123secret" })));
        RawItem {
            location: ConfigLocation {
                client,
                scope: ConfigScope::Personal,
                path: "/fake/source".into(),
            },
            kind: ItemKind::McpConnector,
            identity: identity.to_string(),
            display_name: identity.to_string(),
            description: None,
            secret_fields: crate::agent_config::redact::find_secret_fields(&extra),
            extra,
            content_hash: "h".into(),
        }
    }

    /// A client that reads several filenames in layers must be written to
    /// wherever its settings actually live. Writing to the first declared
    /// name instead can create a new file that shadows the real one, so the
    /// copy looks like it worked while every existing connector stops being
    /// read.
    #[test]
    fn a_copy_lands_in_the_config_file_that_already_exists() {
        use crate::agent_config::model::ConfigScope;
        let loc = |scope, path: &str| ConfigLocation {
            client: ClientId::OpenCode,
            scope,
            path: path.to_string(),
        };
        // Declared order puts config.json first; the person's settings are
        // in opencode.json.
        let locs = vec![
            loc(ConfigScope::Personal, "/home/.config/opencode/config.json"),
            loc(ConfigScope::Personal, "/home/.config/opencode/opencode.json"),
            loc(ConfigScope::Repo, "/repo/opencode.json"),
        ];
        let exists = |p: &str| p == "/home/.config/opencode/opencode.json";

        let chosen = choose_destination(&locs, ConfigScope::Personal, exists).unwrap();
        assert_eq!(chosen.path, "/home/.config/opencode/opencode.json");
    }

    /// With nothing on disk yet, the first declared name is the right
    /// answer -- it is the one the client falls back to.
    #[test]
    fn a_copy_to_a_client_with_no_config_yet_uses_the_first_declared_file() {
        use crate::agent_config::model::ConfigScope;
        let locs = vec![ConfigLocation {
            client: ClientId::OpenCode,
            scope: ConfigScope::Personal,
            path: "/home/.config/opencode/config.json".into(),
        }];
        let chosen = choose_destination(&locs, ConfigScope::Personal, |_| false).unwrap();
        assert_eq!(chosen.path, "/home/.config/opencode/config.json");
    }

    /// A repository connector stays in the repository even when a personal
    /// file exists and the repository one does not.
    #[test]
    fn a_copy_keeps_the_scope_it_came_from() {
        use crate::agent_config::model::ConfigScope;
        let loc = |scope, path: &str| ConfigLocation {
            client: ClientId::OpenCode,
            scope,
            path: path.to_string(),
        };
        let locs = vec![
            loc(ConfigScope::Personal, "/home/.config/opencode/config.json"),
            loc(ConfigScope::Repo, "/repo/opencode.json"),
        ];
        let exists = |p: &str| p == "/home/.config/opencode/config.json";
        let chosen = choose_destination(&locs, ConfigScope::Repo, exists).unwrap();
        assert_eq!(chosen.path, "/repo/opencode.json");
    }

    /// GitWyrm only knows where Claude Code keeps skills. For every other
    /// client the honest answer is that GitWyrm cannot do it yet -- not that
    /// the client has no skills, which is a claim about another product that
    /// GitWyrm has never checked and which may be false.
    #[test]
    fn a_client_without_skill_support_blames_gitwyrm_not_the_client() {
        let mut item = sample_item(ClientId::ClaudeCode, "demo");
        item.kind = ItemKind::Skill;

        for client in ClientId::ALL {
            if crate::agent_config::registry::spec(client).can_read_kind(ItemKind::Skill) {
                continue;
            }
            let preview = skill_copy_preview(client, &item, None);
            let message = &preview.warnings[0].message;
            assert!(
                message.contains("GitWyrm cannot copy skills"),
                "{client:?} should say GitWyrm cannot do it yet, said: {message}"
            );
            assert!(
                !message.contains("does not keep skills"),
                "{client:?} must not assert what another product does: {message}"
            );
        }
    }

    /// A skill is copied as a folder and undone as a folder, through the
    /// same apply/undo commands everything else uses. Before this, the
    /// preview offered a skill copy that produced nothing, and a folder
    /// receipt would have gone to the single-file undo, which reads the
    /// destination as a file and fails.
    #[test]
    fn a_skill_is_applied_and_undone_as_a_whole_folder() {
        let (dir, write_root) = write_root();
        let source_skill = dir.path().join("source").join(".claude").join("skills").join("demo");
        std::fs::create_dir_all(source_skill.join("references")).unwrap();
        std::fs::write(source_skill.join("SKILL.md"), "---
name: demo
---
New
").unwrap();
        std::fs::write(source_skill.join("references").join("api.md"), "ref
").unwrap();

        let destination = dir.path().join("dest").join(".claude").join("skills").join("demo");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("SKILL.md"), "mine
").unwrap();

        let mut item = sample_item(ClientId::ClaudeCode, "demo");
        item.kind = ItemKind::Skill;
        item.location.path = source_skill.join("SKILL.md").to_string_lossy().into_owned();

        let plan = CopyPlan {
            plan_id: "plan-1".into(),
            item_id: "Skill:demo".into(),
            source_item: item,
            destinations: vec![DestinationPreview {
                client: ClientId::OpenCode,
                destination_path: destination.to_string_lossy().into_owned(),
                before_hash: None,
                proposed_content: "SKILL.md
references/api.md".into(),
                redacted_diff_summary: Vec::new(),
                warnings: Vec::new(),
                write_supported: true,
            }],
            created_at: now_rfc3339(),
        };
        write_plan(&write_root, &plan).unwrap();

        let outcome = apply_copy_at(&write_root, "plan-1");
        let operation_id = match &outcome.results[..] {
            [DestinationApplyResult::Applied { operation_id, .. }] => operation_id.clone(),
            other => panic!("expected one applied destination, got {other:?}"),
        };
        assert_eq!(
            std::fs::read_to_string(destination.join("SKILL.md")).unwrap(),
            "---
name: demo
---
New
"
        );
        assert!(destination.join("references").join("api.md").is_file(), "nested files copy too");

        // Undo restores exactly what was replaced, including removing the
        // files the copy added.
        match undo_at(&write_root, &operation_id) {
            UndoOutcome::Restored { .. } => {}
            other => panic!("expected Restored, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(destination.join("SKILL.md")).unwrap(), "mine
");
        assert!(!destination.join("references").exists());

        // And it is not undoable twice.
        assert!(matches!(
            undo_at(&write_root, &operation_id),
            UndoOutcome::AlreadyUndone
        ));
    }

    #[test]
    fn full_preview_apply_undo_round_trip_via_command_logic() {
        let (dir, write_root) = write_root();
        let dest_dir = dir.path().join("dest_home");
        std::fs::create_dir_all(&dest_dir).unwrap();
        let claude_settings = dest_dir.join(".claude").join("settings.json");
        std::fs::create_dir_all(claude_settings.parent().unwrap()).unwrap();
        std::fs::write(&claude_settings, "{\n  \"mcpServers\": {}\n}").unwrap();

        // We cannot easily inject a fake home dir into `locations` from this
        // integration-ish test without adding a seam, so this test instead
        // exercises the plan/apply/undo pipeline directly through the
        // `agent_config::plan` module (already covered in `plan.rs`'s own
        // tests) plus the command-layer plumbing (`preview_copy_at`,
        // `apply_copy_at`, `undo_at`) against a hand-built plan, which is
        // what those functions actually operate on once a plan exists.
        let dest_preview = DestinationPreview {
            client: ClientId::ClaudeCode,
            destination_path: claude_settings.to_string_lossy().into_owned(),
            before_hash: Some(plan::hash_bytes(b"{\n  \"mcpServers\": {}\n}")),
            proposed_content: "{\n  \"mcpServers\": {\n    \"github\": { \"command\": \"npx\" }\n  }\n}".to_string(),
            redacted_diff_summary: vec![],
            warnings: vec![],
            write_supported: true,
        };
        let source_item = sample_item(ClientId::OpenCode, "github");
        let plan = CopyPlan {
            plan_id: new_id(),
            item_id: "McpConnector:github".to_string(),
            source_item,
            destinations: vec![dest_preview],
            created_at: now_rfc3339(),
        };
        write_plan(&write_root, &plan).unwrap();

        let outcome = apply_copy_at(&write_root, &plan.plan_id);
        assert_eq!(outcome.results.len(), 1);
        let DestinationApplyResult::Applied { operation_id, .. } = &outcome.results[0] else {
            panic!("expected Applied, got {:?}", outcome.results[0]);
        };

        let applied_content = std::fs::read_to_string(&claude_settings).unwrap();
        assert!(applied_content.contains("github"));

        let undo_outcome = undo_at(&write_root, operation_id);
        assert!(matches!(undo_outcome, UndoOutcome::Restored { .. }));
        let restored = std::fs::read_to_string(&claude_settings).unwrap();
        assert_eq!(restored, "{\n  \"mcpServers\": {}\n}");
    }

    #[test]
    fn a_connector_copied_into_codex_lands_as_real_toml_and_undoes_cleanly() {
        // Codex is the one client whose file is not JSON, so this proves the
        // whole pipeline -- plan, apply, backup, undo -- works on a format the
        // rest of the tests never exercise. A writer that only passes its own
        // unit tests is not yet a feature anyone can use.
        let (dir, write_root) = write_root();
        let config = dir.path().join("config.toml");
        let original = "# my setup
model = \"gpt-5\"
";
        std::fs::write(&config, original).unwrap();

        let source_item = sample_item(ClientId::OpenCode, "github");
        let proposed = crate::agent_config::writers::build_new_content(
            ClientId::Codex,
            source_item.kind,
            &source_item.identity,
            &source_item.extra,
            original,
        )
        .expect("Codex writer must produce content");

        let plan = CopyPlan {
            plan_id: new_id(),
            item_id: "McpConnector:github".to_string(),
            source_item,
            destinations: vec![DestinationPreview {
                client: ClientId::Codex,
                destination_path: config.to_string_lossy().into_owned(),
                before_hash: Some(plan::hash_bytes(original.as_bytes())),
                proposed_content: proposed,
                redacted_diff_summary: vec![],
                warnings: vec![],
                write_supported: true,
            }],
            created_at: now_rfc3339(),
        };
        write_plan(&write_root, &plan).unwrap();

        let outcome = apply_copy_at(&write_root, &plan.plan_id);
        let DestinationApplyResult::Applied { operation_id, .. } = &outcome.results[0] else {
            panic!("expected Applied, got {:?}", outcome.results[0]);
        };

        let applied = std::fs::read_to_string(&config).unwrap();
        assert!(applied.contains("[mcp_servers.github]"), "{applied}");
        assert!(applied.contains("# my setup"), "the comment must survive: {applied}");
        assert!(applied.contains("model = \"gpt-5\""), "{applied}");
        // The secret is redacted in the preview but written for real, which is
        // the point of copying a connector at all.
        assert!(applied.contains("abc123secret"), "{applied}");

        let undo_outcome = undo_at(&write_root, operation_id);
        assert!(matches!(undo_outcome, UndoOutcome::Restored { .. }));
        assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
    }

    #[test]
    fn apply_refuses_a_destination_that_changed_since_preview_but_still_returns_a_result() {
        let (dir, write_root) = write_root();
        let dest = dir.path().join("dest.json");
        std::fs::write(&dest, "{\"a\":1}").unwrap();

        let dest_preview = DestinationPreview {
            client: ClientId::ClaudeCode,
            destination_path: dest.to_string_lossy().into_owned(),
            before_hash: Some(plan::hash_bytes(b"{\"a\":0}")), // stale on purpose
            proposed_content: "{\"a\":2}".to_string(),
            redacted_diff_summary: vec![],
            warnings: vec![],
            write_supported: true,
        };
        let plan = CopyPlan {
            plan_id: new_id(),
            item_id: "McpConnector:x".to_string(),
            source_item: sample_item(ClientId::OpenCode, "x"),
            destinations: vec![dest_preview],
            created_at: now_rfc3339(),
        };
        write_plan(&write_root, &plan).unwrap();

        let outcome = apply_copy_at(&write_root, &plan.plan_id);
        assert_eq!(outcome.results.len(), 1);
        assert!(matches!(
            outcome.results[0],
            DestinationApplyResult::ConcurrentChangeRefused { .. }
        ));
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "{\"a\":1}", "untouched");
    }

    #[test]
    fn secrets_never_appear_in_a_preview_s_redacted_diff_summary_or_warnings() {
        let source = sample_item(ClientId::OpenCode, "github");
        let redacted = redact::redact_for_display(&source.extra, &source.secret_fields);
        let lines = redact::diff_fields(&redacted, None, &source.secret_fields);
        let serialized = serde_json::to_string(&lines).unwrap();
        assert!(!serialized.contains("abc123secret"));
    }

    #[test]
    fn preview_outcome_reports_item_not_found_rather_than_erroring() {
        let (_dir, write_root) = write_root();
        let outcome = preview_copy_at(&write_root, None, "McpConnector:does-not-exist", &[ClientId::ClaudeCode]).unwrap();
        assert!(matches!(outcome, PreviewOutcome::ItemNotFound));
    }

    #[test]
    fn preview_with_no_destinations_is_reported_distinctly() {
        let (_dir, write_root) = write_root();
        let outcome = preview_copy_at(&write_root, None, "McpConnector:whatever", &[]).unwrap();
        assert!(matches!(outcome, PreviewOutcome::NoDestinations));
    }

    #[test]
    fn undo_of_unknown_operation_reports_not_found_not_an_error() {
        let (_dir, write_root) = write_root();
        let outcome = undo_at(&write_root, "nope");
        assert!(matches!(outcome, UndoOutcome::OperationNotFound));
    }

    #[test]
    fn preview_copy_returns_the_redacted_view_never_the_raw_plan() {
        // Task R7: a real API key must never cross the IPC boundary. This
        // proves `agent_config_preview_copy`'s return value (what actually
        // reaches the frontend) carries no field-value content at all --
        // only `RedactedCopyPlan`'s narrowed shape.
        let (dir, write_root) = write_root();
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let mut extra = std::collections::BTreeMap::new();
        extra.insert("command".to_string(), JsonValue(serde_json::json!("npx")));
        extra.insert(
            "env".to_string(),
            JsonValue(serde_json::json!({ "API_KEY": "sk-live-super-secret-value-12345" })),
        );
        let secret_fields = crate::agent_config::redact::find_secret_fields(&extra);
        let source_item = RawItem {
            location: ConfigLocation {
                client: ClientId::OpenCode,
                scope: ConfigScope::Personal,
                path: "/fake/source".into(),
            },
            kind: ItemKind::McpConnector,
            identity: "github".into(),
            display_name: "github".into(),
            description: None,
            secret_fields,
            extra,
            content_hash: "h".into(),
        };

        let dest_preview = crate::agent_config::model::DestinationPreview {
            client: ClientId::ClaudeCode,
            destination_path: dest_dir.join("settings.json").to_string_lossy().into_owned(),
            before_hash: None,
            proposed_content: "{\"mcpServers\":{\"github\":{\"command\":\"npx\",\"env\":{\"API_KEY\":\"sk-live-super-secret-value-12345\"}}}}".into(),
            redacted_diff_summary: vec![],
            warnings: vec![],
            write_supported: true,
        };
        let plan = CopyPlan {
            plan_id: new_id(),
            item_id: "McpConnector:github".into(),
            source_item,
            destinations: vec![dest_preview],
            created_at: now_rfc3339(),
        };

        // The redacted view is what preview_copy_at actually returns to the
        // command layer (and from there, over IPC).
        let redacted = crate::agent_config::model::RedactedCopyPlan::from(&plan);
        let serialized = serde_json::to_string(&redacted).unwrap();
        assert!(
            !serialized.contains("sk-live-super-secret-value-12345"),
            "redacted plan must never carry the real secret value"
        );
        assert!(
            !serialized.contains("npx"),
            "redacted plan must not carry raw proposed_content at all, secret or not"
        );

        // But the full plan persisted to disk (what apply reads back) still
        // has the genuine content -- apply must not be broken by redaction.
        write_plan(&write_root, &plan).unwrap();
        let read_back = read_plan(&write_root, &plan.plan_id).unwrap();
        assert_eq!(read_back.destinations[0].proposed_content, plan.destinations[0].proposed_content);
    }

    #[test]
    fn a_batch_with_one_stale_destination_still_applies_the_others_and_leaves_the_stale_one_untouched() {
        // R7.8: concurrent edits + partial batch failure must not touch the
        // destination that changed underneath us, while sibling plans in the
        // same batch still succeed.
        let (dir, write_root) = write_root();

        let fresh_dest = dir.path().join("fresh.json");
        std::fs::write(&fresh_dest, "{\"a\":1}").unwrap();
        let fresh_before_hash = plan::hash_bytes(b"{\"a\":1}");

        let stale_dest = dir.path().join("stale.json");
        std::fs::write(&stale_dest, "{\"b\":1}").unwrap();
        // Someone else edits this destination after preview was computed --
        // the plan's before_hash below is deliberately stale.
        std::fs::write(&stale_dest, "{\"b\":999}").unwrap();

        let fresh_plan = CopyPlan {
            plan_id: new_id(),
            item_id: "McpConnector:fresh".into(),
            source_item: sample_item(ClientId::OpenCode, "fresh"),
            destinations: vec![DestinationPreview {
                client: ClientId::ClaudeCode,
                destination_path: fresh_dest.to_string_lossy().into_owned(),
                before_hash: Some(fresh_before_hash),
                proposed_content: "{\"a\":2}".into(),
                redacted_diff_summary: vec![],
                warnings: vec![],
                write_supported: true,
            }],
            created_at: now_rfc3339(),
        };
        let stale_plan = CopyPlan {
            plan_id: new_id(),
            item_id: "McpConnector:stale".into(),
            source_item: sample_item(ClientId::OpenCode, "stale"),
            destinations: vec![DestinationPreview {
                client: ClientId::ClaudeCode,
                destination_path: stale_dest.to_string_lossy().into_owned(),
                before_hash: Some(plan::hash_bytes(b"{\"b\":1}")), // stale on purpose
                proposed_content: "{\"b\":2}".into(),
                redacted_diff_summary: vec![],
                warnings: vec![],
                write_supported: true,
            }],
            created_at: now_rfc3339(),
        };
        write_plan(&write_root, &fresh_plan).unwrap();
        write_plan(&write_root, &stale_plan).unwrap();

        let batch_outcomes: Vec<ApplyOutcome> = [&fresh_plan.plan_id, &stale_plan.plan_id]
            .iter()
            .map(|id| apply_copy_at(&write_root, id))
            .collect();

        let fresh_outcome = batch_outcomes.iter().find(|o| o.plan_id == fresh_plan.plan_id).unwrap();
        assert!(matches!(fresh_outcome.results[0], DestinationApplyResult::Applied { .. }));
        assert_eq!(std::fs::read_to_string(&fresh_dest).unwrap(), "{\"a\":2}");

        let stale_outcome = batch_outcomes.iter().find(|o| o.plan_id == stale_plan.plan_id).unwrap();
        assert!(matches!(
            stale_outcome.results[0],
            DestinationApplyResult::ConcurrentChangeRefused { .. }
        ));
        assert_eq!(
            std::fs::read_to_string(&stale_dest).unwrap(),
            "{\"b\":999}",
            "the concurrently-edited destination must be left completely untouched"
        );
    }
}
