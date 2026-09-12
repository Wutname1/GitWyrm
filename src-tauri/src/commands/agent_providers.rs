//! Listing the AI tools GitWyrm can drive, for the picker.
//!
//! One command. It answers the three things a picker has to show for each
//! tool: what it is called, whether it is actually installed right now, and
//! whether it can be trusted with work that must not change anything.
//!
//! The last one is the reason this exists as a real command rather than a
//! hardcoded list in the frontend. A tool that cannot be told to refuse a
//! write is refused for read-only work (see `ai::agent::select`), and a picker
//! that offered it anyway would let the user choose an option that then fails
//! at launch. Better to show it greyed out with the reason attached.

use std::collections::HashMap;

use tauri::Emitter;

use crate::ai::agent::codex_models::{ModelCatalog, ModelSource};
use crate::ai::agent::tool_updates::UpdateCheck;
use crate::ai::agent::copilot_cli::{detect_agent, CliState};
use crate::ai::agent::registry::{self, Denial};

/// One row in the provider picker.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentProvider {
    /// Stable id, the value to pass back as a provider override.
    pub id: String,
    /// What to show the user. The tool's own spelling of its name.
    pub display_name: String,
    /// Whether this is the tool used when nothing is chosen.
    pub is_default: bool,
    /// Installed and new enough to drive right now.
    pub installed: bool,
    /// The version string the tool reported, when it is installed.
    pub version: Option<String>,
    /// Set when the tool was found but is older than the floor GitWyrm has
    /// checked against. Distinct from not installed: updating fixes it.
    pub too_old: bool,
    /// The tool is on disk but did not answer when asked its version.
    ///
    /// Distinct from `installed: false`, which sends the person to install
    /// something they already have.
    pub unresponsive: bool,
    /// Whether a newer release exists.
    ///
    /// Deliberately not folded into `too_old`. That one is a refusal -- the
    /// tool is below a floor GitWyrm has measured and will not drive. This is
    /// an FYI about a tool that works: merging them would either nag people
    /// about healthy installs or block them over a version nobody tested.
    ///
    /// `notChecked` when GitWyrm could not find out, which is never the same
    /// as up to date.
    pub update: UpdateCheck,
    /// Whether this tool can be told to leave files alone, which decides
    /// whether it may run Ask, Explain, Review, Summarize, or a Plan before
    /// Start.
    pub can_do_read_only_work: bool,
    /// Why it cannot, in words the picker can show directly. `None` when it
    /// can.
    pub read_only_limit: Option<String>,
    /// Where to send someone who wants this tool: its install page when it is
    /// missing, its documentation when it is present. One URL, one control.
    pub homepage_url: String,
    /// The command that installs it, to be read and copied. GitWyrm never runs
    /// this.
    pub install_hint: String,
    /// The binary name being looked for on this platform.
    ///
    /// Shown on every row, including missing ones, because a package name, a
    /// binary name and a product name are routinely three different strings.
    /// When detection is wrong this is the line that explains why.
    pub binary_name: String,
    /// Models this tool can be asked for, best-known first.
    ///
    /// Empty means the tool takes no model flag, which is a different fact
    /// from "no models" -- the picker shows no model control at all rather
    /// than an empty menu implying a choice that does not exist.
    pub models: Vec<AgentModelChoice>,
    /// Thinking-effort levels this tool accepts, lowest first, spelled the way
    /// the tool spells them. Empty when it cannot be asked.
    pub effort_levels: Vec<String>,
    /// Why this tool cannot ask before it acts, when it cannot.
    ///
    /// `null` is the ordinary case and shows nothing. A sentence means this
    /// tool runs commands without stopping to ask, which the person choosing
    /// it should know before they choose -- rather than after each run, when
    /// the choice is already made.
    pub approval_gate_gap: Option<String>,
    /// Where `models` came from.
    ///
    /// `fallback` means GitWyrm could not ask the tool and is showing the list
    /// built into it, which may be out of date -- exactly how two `gpt-5.4`
    /// entries survived three Codex releases. Never collapse this into the
    /// list itself: a stale list that cannot say it is stale is the defect.
    pub model_source: ModelSource,
}

/// One model a tool offers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelChoice {
    /// Passed to the tool verbatim. Never shown as-is.
    pub id: String,
    /// What the user sees.
    pub display_name: String,
    /// The tool's own one-line description, when it gives one.
    pub description: String,
    /// The model the tool would choose for itself, marked the way the tool's
    /// own picker marks it.
    pub is_default: bool,
    /// How hard THIS model can be asked to think, lowest first.
    ///
    /// Per model, not per tool: Codex accepts six levels on most models and
    /// four on `gpt-5.5`. `AgentProvider::effort_levels` is the tool-wide
    /// list and stays for the tools whose levels really are tool-wide; this
    /// is empty when the tool does not say.
    pub efforts: Vec<String>,
}

/// What the picker needs to render itself for one chat.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentProviderChoices {
    pub providers: Vec<AgentProvider>,
    /// Whether this chat may never change files, as the engine's own tool
    /// gate decides it.
    ///
    /// Answered here rather than in the frontend on purpose. The rule is
    /// `check_tool_capability(intent, started, EditFile)`, which depends on
    /// the session's INTENT and whether a Plan has been started -- not on the
    /// composer's mode. A first attempt derived it from the mode pill and got
    /// a different answer, so the picker offered a tool for a Review chat
    /// that the launch then refused. One authority, asked once.
    pub read_only: bool,
}

/// Every tool this build knows how to drive, with its current install state,
/// plus whether the given chat is read-only.
///
/// Probing is per-tool and cached only when a tool is found, so a user who
/// installs one while the picker is open sees it appear on the next open
/// rather than after a restart.
///
/// A `session_id` that cannot be read falls back to `read_only: true`. That is
/// the safe direction: it may grey out a tool that would have worked, which is
/// visible and recoverable, rather than offering one that fails at launch
/// after the user's message has already been sent.
#[tauri::command]
#[specta::specta]
pub async fn agent_providers_list(
    app: tauri::AppHandle,
    session_id: Option<String>,
) -> Result<AgentProviderChoices, crate::error::AppError> {
    let root = crate::agentdesk::store::SessionStoreRoot::resolve(&app)
        .map_err(|e| crate::error::AppError::Other(e.to_string()))?;

    // Answer from what was written down last run, which costs one small file
    // read and the version probes that are already cached. Before this, the
    // picker showed nothing for about four seconds on every launch while five
    // subprocesses and five network calls ran -- and the next launch repeated
    // all of it, having learnt nothing.
    let restored = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || restore_learned(&app))
            .await
            .map_err(|e| crate::error::AppError::Other(e.to_string()))?
    };

    let learned = if restored.is_empty() {
        // Nothing remembered -- a first run, a cleared memory, or an install
        // that has changed since. There is no faster honest answer than
        // asking, so this one time the picker waits.
        let fresh = resolve_learned().await;
        let to_write = clone_learned(&fresh);
        let app = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || remember_learned(&app, &to_write)).await;
        fresh
    } else {
        // Check again quietly behind the answer already being returned.
        // Deliberately not awaited: the whole point is that nobody waits for
        // it. The event fires only when the fresh answer actually differs, so
        // a check that confirms what is on screen never redraws it.
        let shown = clone_learned(&restored);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let fresh = resolve_learned().await;
            let differs = differs_from(&shown, &fresh);
            let to_write = clone_learned(&fresh);
            let app_for_disk = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                remember_learned(&app_for_disk, &to_write)
            })
            .await;
            if differs {
                let _ = app.emit(TOOLS_CHANGED_EVENT, ());
            }
        });
        restored
    };

    tauri::async_runtime::spawn_blocking(move || AgentProviderChoices {
        providers: list(&learned),
        read_only: session_id
            .map(|id| session_is_read_only(&root, &id))
            .unwrap_or(false),
    })
    .await
    .map_err(|e| crate::error::AppError::Other(e.to_string()))
}

/// Whether this session can never call a write tool, in either start state.
///
/// Mirrors `select::must_not_change_anything` by calling the same gate. A Plan
/// counts as read-only only until Start: after it, writing is the point, and
/// greying a tool out there would refuse a usable option.
fn session_is_read_only(root: &crate::agentdesk::store::SessionStoreRoot, session_id: &str) -> bool {
    use crate::agentdesk::policy::{check_tool_capability, ToolCapability};
    let Ok(session) = crate::agentdesk::store::read_session(root, session_id) else {
        return true;
    };
    let started = session.header.graph_started_at.is_some();
    check_tool_capability(session.header.intent, started, ToolCapability::EditFile).is_err()
}

/// Re-reads the shell's `PATH`, forgets every cached probe, and detects again.
///
/// This is the Refresh button, and the `PATH` step is the whole reason it
/// works. A GUI app holds the environment it was launched with, so a tool
/// installed a minute ago is not on the `PATH` this process can see; probing
/// again without re-reading it returns the same "not installed" answer and the
/// button looks broken. See `ai::agent::shell_path`.
///
/// Returns the same shape as [`agent_providers_list`] so the screen can
/// replace its state wholesale.
#[tauri::command]
#[specta::specta]
pub async fn agent_providers_refresh(
    app: tauri::AppHandle,
    session_id: Option<String>,
) -> Result<AgentProviderChoices, crate::error::AppError> {
    let root = crate::agentdesk::store::SessionStoreRoot::resolve(&app)
        .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
    tauri::async_runtime::spawn_blocking(|| {
        let added = crate::ai::agent::shell_path::rehydrate();
        crate::ai::agent::copilot_cli::forget_all_cached();
        // Refresh means "ask everything again". Leaving the model lists
        // remembered would make the button that exists to pick up a newly
        // installed or updated tool keep showing that tool's old models.
        crate::ai::agent::codex_models::forget_all_cached();
        // And what was learnt about newer releases: refresh means ask
        // everything again, and somebody who has just updated a tool should
        // not keep reading that a newer one is available.
        crate::ai::agent::tool_updates::forget_all_cached();
        log::info!("agent refresh: {added} new PATH folders, cached probes dropped");
    })
    .await
    .map_err(|e| crate::error::AppError::Other(e.to_string()))?;

    let learned = resolve_learned().await;
    tauri::async_runtime::spawn_blocking(move || AgentProviderChoices {
        providers: list(&learned),
        read_only: session_id
            .map(|id| session_is_read_only(&root, &id))
            .unwrap_or(false),
    })
    .await
    .map_err(|e| crate::error::AppError::Other(e.to_string()))
}

/// Fired when a quiet re-check found something different from what the picker
/// was already showing.
///
/// Carries no payload: the frontend refetches, which keeps one shape for the
/// answer instead of a second one that could disagree with it.
pub const TOOLS_CHANGED_EVENT: &str = "agent-tools-changed";

/// Everything learnt about one tool before its row is built.
///
/// One struct rather than a map per fact: `row` already takes a spec, and a
/// parameter per feature is how a signature becomes unreadable.
#[derive(Default)]
struct Learned {
    catalog: Option<ModelCatalog>,
    update: Option<UpdateCheck>,
}

/// Every tool's row, with whatever could be learnt already resolved.
///
/// A tool with nothing learnt gets its built-in model list and an unchecked
/// update state -- which is what every tool gets on a machine where none of
/// them can be reached.
fn list(learned: &HashMap<&'static str, Learned>) -> Vec<AgentProvider> {
    let nothing = Learned::default();
    registry::AGENTS
        .iter()
        .map(|spec| {
            let known = learned.get(spec.id).unwrap_or(&nothing);
            let fallback;
            let catalog = match &known.catalog {
                Some(c) => c,
                None => {
                    fallback = ModelCatalog::fallback(spec);
                    &fallback
                }
            };
            row(
                spec,
                catalog,
                known.update.clone().unwrap_or(UpdateCheck::NotChecked),
            )
        })
        .collect()
}

/// `Learned` holds a `ModelCatalog`, which is not `Clone`-derived, so this
/// spells the copy out rather than making the whole type cloneable for one use.
fn clone_learned(src: &HashMap<&'static str, Learned>) -> HashMap<&'static str, Learned> {
    src.iter()
        .map(|(k, v)| {
            (
                *k,
                Learned { catalog: v.catalog.clone(), update: v.update.clone() },
            )
        })
        .collect()
}

/// Whether a fresh answer says anything different from the one on screen.
///
/// Compares only what a reader would see -- which models are offered, in what
/// order, and whether a newer release exists. A re-check that confirms the
/// picker must not redraw it.
fn differs_from(shown: &HashMap<&'static str, Learned>, fresh: &HashMap<&'static str, Learned>) -> bool {
    if shown.len() != fresh.len() {
        return true;
    }
    shown.iter().any(|(id, was)| match fresh.get(id) {
        None => true,
        Some(now) => {
            let model_ids = |l: &Learned| {
                l.catalog
                    .as_ref()
                    .map(|c| c.models.iter().map(|m| m.id.clone()).collect::<Vec<_>>())
                    .unwrap_or_default()
            };
            was.update != now.update || model_ids(was) != model_ids(now)
        }
    })
}

/// What was remembered from a previous run, for the tools it is still about.
///
/// Reads one small file and asks each tool only for its version, which is the
/// cached probe. Nothing here goes to the network and nothing spawns a model
/// query, so the picker can paint from this immediately and check again
/// afterwards.
///
/// An entry is used only when it is about the install in front of us now --
/// same executable, same version string. Anything else is dropped, because the
/// moment a tool is updated is exactly when its remembered answer is most
/// wrong and would be most trusted.
fn restore_learned(app: &tauri::AppHandle) -> HashMap<&'static str, Learned> {
    let Ok(app_data) = crate::settings::app_data_dir(app) else {
        return HashMap::new();
    };
    let memory = crate::ai::agent::tool_memory::load(&crate::ai::agent::tool_memory::memory_path(app_data));
    let mut out: HashMap<&'static str, Learned> = HashMap::new();
    for spec in registry::AGENTS.iter() {
        let CliState::Ready { version, path } = detect_agent(spec).state else {
            continue;
        };
        let Some(entry) = memory.tools.get(spec.id) else { continue };
        if !crate::ai::agent::tool_memory::is_about(entry, &path, &version) {
            continue;
        }
        log::info!(
            "tool memory: reusing what {} said at {} (written {}s ago)",
            spec.id,
            entry.version,
            crate::ai::agent::tool_memory::now_secs().saturating_sub(entry.written_at)
        );
        let learned = out.entry(spec.id).or_default();
        if !entry.models.is_empty() {
            learned.catalog = Some(ModelCatalog {
                models: entry
                    .models
                    .iter()
                    .map(|m| crate::ai::agent::codex::CodexModel {
                        id: m.id.clone(),
                        display_name: m.display_name.clone(),
                        description: m.description.clone(),
                        is_default: m.is_default,
                        efforts: m.efforts.clone(),
                        default_effort: m.default_effort.clone(),
                    })
                    .collect(),
                // The tool's own answer, written down. The comparison against
                // the newest release is redone below rather than remembered as
                // a verdict, which would go stale the moment it is updated.
                source: ModelSource::Live,
            });
        }
        learned.update = entry
            .latest_release
            .as_deref()
            .map(|latest| crate::ai::agent::tool_updates::compare(&version, Some(latest)));
    }
    out
}

/// Writes down what was learnt, so the next run can answer immediately.
///
/// Only positive answers are kept. A tool that could not be reached leaves
/// whatever was remembered about it alone rather than recording the failure,
/// for the reason detection never caches "not installed": somebody who fixes
/// the thing that was broken must not keep reading the old answer.
fn remember_learned(app: &tauri::AppHandle, learned: &HashMap<&'static str, Learned>) {
    use crate::ai::agent::tool_memory as mem;
    let Ok(app_data) = crate::settings::app_data_dir(app) else { return };
    let path = mem::memory_path(app_data);
    let mut memory = mem::load(&path);
    let mut changed = false;

    for spec in registry::AGENTS.iter() {
        let Some(known) = learned.get(spec.id) else { continue };
        let CliState::Ready { version, path: exe } = detect_agent(spec).state else {
            continue;
        };
        let models: Vec<mem::RememberedModel> = known
            .catalog
            .as_ref()
            .filter(|c| c.source == ModelSource::Live)
            .map(|c| {
                c.models
                    .iter()
                    .map(|m| mem::RememberedModel {
                        id: m.id.clone(),
                        display_name: m.display_name.clone(),
                        description: m.description.clone(),
                        is_default: m.is_default,
                        efforts: m.efforts.clone(),
                        default_effort: m.default_effort.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Stored as what the source published, not as a verdict.
        let latest = match &known.update {
            Some(UpdateCheck::NewerAvailable { latest }) => Some(latest.clone()),
            Some(UpdateCheck::UpToDate) => Some(version.clone()),
            _ => None,
        };
        if models.is_empty() && latest.is_none() {
            continue;
        }
        let existing = memory.tools.get(spec.id);
        let entry = mem::Remembered {
            path: exe,
            version,
            // Keep whatever was already known where this run learnt nothing
            // new, so one unreachable half does not erase the other.
            models: if models.is_empty() {
                existing.map(|e| e.models.clone()).unwrap_or_default()
            } else {
                models
            },
            latest_release: latest.or_else(|| existing.and_then(|e| e.latest_release.clone())),
            written_at: mem::now_secs(),
        };
        if existing != Some(&entry) {
            memory.tools.insert(spec.id.to_string(), entry);
            changed = true;
        }
    }

    if changed {
        mem::save(&path, &memory);
    }
}

/// Asks each tool, and the place its releases are published, what they know.
///
/// Both halves are best-effort and neither can fail the list: a tool that
/// cannot be asked keeps its built-in models, and a release source that cannot
/// be reached leaves the update state unchecked.
///
/// The update checks run together rather than one after another, because they
/// are independent network calls and a picker should not wait five times.
async fn resolve_learned() -> HashMap<&'static str, Learned> {
    let mut out: HashMap<&'static str, Learned> = HashMap::new();

    if let Some(codex) = registry::find("codex") {
        out.entry(codex.id).or_default().catalog =
            Some(crate::ai::agent::codex_models::catalog(codex).await);
    }

    let mut checks = tokio::task::JoinSet::new();
    for spec in registry::AGENTS.iter() {
        // The version already probed, rather than a second probe: detection is
        // cached, so this is a map lookup in the common case.
        let installed = match detect_agent(spec).state {
            CliState::Ready { version, .. } => Some(version),
            _ => None,
        };
        checks.spawn(async move {
            (
                spec.id,
                crate::ai::agent::tool_updates::check(spec, installed.as_deref()).await,
            )
        });
    }
    while let Some(joined) = checks.join_next().await {
        // A panicking check must not take the picker with it; the tool simply
        // stays unchecked, which is a state the reader already understands.
        if let Ok((id, check)) = joined {
            out.entry(id).or_default().update = Some(check);
        }
    }

    out
}

fn row(
    spec: &'static registry::AgentSpec,
    catalog: &ModelCatalog,
    update: UpdateCheck,
) -> AgentProvider {
    let probe = detect_agent(spec);
    // `unresponsive` is separate from `installed` on purpose: the tool IS on
    // disk, so telling the person to install it -- which is what an
    // `installed: false` row does -- sends them to fix something that is not
    // broken. Reported as its own state rather than folded into either.
    let (installed, version, too_old, unresponsive) = match &probe.state {
        CliState::Ready { version, .. } => (true, Some(short_version(version)), false, false),
        CliState::TooOld { version, .. } => (false, Some(short_version(version)), true, false),
        CliState::FoundButUnresponsive { .. } => (false, None, false, true),
        CliState::NotFound => (false, None, false, false),
    };

    AgentProvider {
        id: spec.id.to_string(),
        display_name: spec.display_name.to_string(),
        is_default: spec.id == registry::DEFAULT_AGENT_ID,
        installed,
        version,
        too_old,
        unresponsive,
        update,
        can_do_read_only_work: spec.can_guarantee_read_only(),
        read_only_limit: read_only_limit(spec),
        homepage_url: spec.homepage_url.to_string(),
        install_hint: spec.install_hint.to_string(),
        binary_name: spec
            .candidate_names()
            .first()
            .copied()
            .unwrap_or(spec.id)
            .to_string(),
        models: catalog
            .models
            .iter()
            .map(|m| AgentModelChoice {
                id: m.id.clone(),
                display_name: m.display_name.clone(),
                description: m.description.clone(),
                is_default: m.is_default,
                efforts: m.efforts.clone(),
            })
            .collect(),
        effort_levels: match spec.effort {
            crate::ai::agent::registry::EffortSupport::Flag { levels, .. } => {
                levels.iter().map(|l| (*l).to_string()).collect()
            }
            crate::ai::agent::registry::EffortSupport::None => Vec::new(),
        },
        approval_gate_gap: spec.approval_gate_gap.map(str::to_string),
        model_source: catalog.source,
    }
}

/// The first line of whatever the tool printed, trimmed.
///
/// These tools do not agree on how much to say. `opencode --version` prints
/// `1.18.10` and nothing else, while `copilot version` prints its name, its
/// version, a blank line, and a sentence about whether an update is available.
/// The picker shows this in a small monospace note beside the tool's name, so
/// the whole answer would wrap into a paragraph there.
///
/// Only the first line is kept, and only up to a sane width: everything after
/// it is advice for a terminal, not an identity. Nothing is parsed out of it
/// -- the string is the tool's to format, and guessing at a version number
/// inside it would break the moment one of them reworded its output.
fn short_version(raw: &str) -> String {
    let first = raw.lines().next().unwrap_or("").trim();
    const MAX: usize = 40;
    if first.chars().count() <= MAX {
        return first.to_string();
    }
    let cut: String = first.chars().take(MAX - 1).collect();
    format!("{}…", cut.trim_end())
}

/// The sentence explaining a read-only limit, or `None` when there is none.
///
/// Written for the picker rather than reused from `select::refuse_read_only`:
/// that one is an error shown after a refusal ("pick a different tool"), while
/// this is a label on the option itself, where telling the user to pick
/// something else would be redundant with the list they are looking at.
fn read_only_limit(spec: &registry::AgentSpec) -> Option<String> {
    match spec.denial {
        Denial::None => Some(format!(
            "{} has no way to be told to leave your files alone, so it cannot be used for \
             chats that are only supposed to look and not change anything.",
            spec.display_name
        )),
        _ => None,
    }
}

#[cfg(test)]
/// Rows with nothing asked of any tool -- every model list is the built-in
/// one. What the picker shows on a machine where no tool can be reached, which
/// is the state most of these tests are about.
fn list_without_asking_any_tool() -> Vec<AgentProvider> {
    list(&HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::{
        AgentSession, AgentSessionHeader, SessionIntent, SessionSource, SessionState,
        CURRENT_SCHEMA_VERSION,
    };
    use crate::agentdesk::store::{write_session, SessionStoreRoot};

    fn stored_session(intent: SessionIntent, started: bool) -> (tempfile::TempDir, SessionStoreRoot, String) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = SessionStoreRoot::at(dir.path().join("agent-desk").join("v1")).expect("root");
        let header = AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: "s-1".into(),
            repo_id: "r".into(),
            repo_path: "C:/code/p".into(),
            repo_name: "p".into(),
            title: "t".into(),
            // NOT `Manual`. A manual chat carrying `Ask` is how every chat
            // made before the New-chat button switched to `Fix` looks on
            // disk, and `migrate_session` widens exactly that pair on load --
            // so a manual fixture here would have its intent changed out from
            // under the assertion. These intents arrive with a real source in
            // production anyway; this names one.
            source: SessionSource::Issue {
                host_id: "github".into(),
                owner: "o".into(),
                repo: "r".into(),
                number: 1,
                url: "https://example.invalid/1".into(),
                snapshot: crate::agentdesk::model::SourceSnapshot {
                    title: "t".into(),
                    summary: String::new(),
                    captured_at: "2026-01-01T00:00:00Z".into(),
                    live_unavailable: false,
                },
            },
            intent,
            state: SessionState::Ready,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            unread: false,
            changed_file_count: 0,
            active_execution_id: None,
            archived: false,
            graph_started_at: started.then(|| "2026-01-01T00:00:00Z".to_string()),
            preferred_provider: None,
            preferred_mode: None,
            preferred_team: None,
            preferred_model: None,
            preferred_effort: None,
        };
        write_session(&root, &AgentSession::new(header)).expect("write");
        (dir, root, "s-1".to_string())
    }

    #[test]
    fn a_review_chat_is_reported_read_only() {
        // The bug this replaced: the frontend derived read-only from the
        // composer's mode pill, which defaults to Auto regardless of intent.
        // A Review chat therefore looked writable, the picker offered a tool
        // that cannot promise read-only, and the launch refused it after the
        // user's message had already been sent.
        for intent in [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Review,
            SessionIntent::Summarize,
        ] {
            let (_d, root, id) = stored_session(intent, false);
            assert!(
                session_is_read_only(&root, &id),
                "{intent:?} must be read-only whatever the composer shows"
            );
        }
    }

    #[test]
    fn a_plan_is_read_only_only_until_it_is_started() {
        let (_d, root, id) = stored_session(SessionIntent::Plan, false);
        assert!(session_is_read_only(&root, &id), "a plan cannot write before Start");

        let (_d2, root2, id2) = stored_session(SessionIntent::Plan, true);
        assert!(
            !session_is_read_only(&root2, &id2),
            "a started plan writes, so greying a tool out there would refuse a usable option"
        );
    }

    #[test]
    fn a_fix_chat_is_not_read_only() {
        let (_d, root, id) = stored_session(SessionIntent::Fix, false);
        assert!(!session_is_read_only(&root, &id));
    }

    #[test]
    fn an_unreadable_session_is_treated_as_read_only() {
        // Fail safe: greying out a tool that would have worked is visible and
        // recoverable; offering one that then fails is not.
        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = SessionStoreRoot::at(dir.path().join("agent-desk").join("v1")).expect("root");
        assert!(session_is_read_only(&root, "does-not-exist"));
    }

    #[test]
    fn every_registered_tool_appears_once() {
        let rows = list_without_asking_any_tool();
        assert_eq!(rows.len(), registry::AGENTS.len());
        for spec in registry::AGENTS {
            assert!(
                rows.iter().any(|r| r.id == spec.id),
                "{} is missing from the picker",
                spec.id
            );
        }
    }

    #[test]
    fn exactly_one_tool_is_the_default() {
        let rows = list_without_asking_any_tool();
        assert_eq!(rows.iter().filter(|r| r.is_default).count(), 1);
        assert!(rows.iter().any(|r| r.is_default && r.id == "copilot"));
    }

    #[test]
    fn a_tool_that_cannot_refuse_a_write_says_so_in_words() {
        // opencode is the one in the table today. The picker has to be able
        // to explain the limit at the point of choosing, not only after a
        // refusal at launch.
        let rows = list_without_asking_any_tool();
        let opencode = rows.iter().find(|r| r.id == "opencode").expect("opencode is listed");
        assert!(!opencode.can_do_read_only_work);
        let limit = opencode.read_only_limit.as_ref().expect("the limit must be explained");
        assert!(limit.contains("opencode"), "the sentence must name the tool: {limit}");
        assert!(!limit.is_empty());
    }

    #[test]
    fn a_tool_that_can_refuse_a_write_carries_no_limit_text() {
        let rows = list_without_asking_any_tool();
        for id in ["copilot", "gemini", "claude"] {
            let row = rows.iter().find(|r| r.id == id).expect("listed");
            assert!(row.can_do_read_only_work, "{id} can enforce read-only");
            assert!(
                row.read_only_limit.is_none(),
                "{id} must not carry a limit sentence"
            );
        }
    }

    /// Every tool has somewhere to send a person who does not have it.
    ///
    /// This is the field that ends the "not found on this machine" dead end,
    /// so an empty one silently turns a row back into a dead end.
    #[test]
    fn every_tool_says_where_to_get_it() {
        for row in list_without_asking_any_tool() {
            assert!(
                row.homepage_url.starts_with("https://"),
                "{} has no install page",
                row.id
            );
            assert!(!row.install_hint.is_empty(), "{} has no install command", row.id);
            assert!(
                !row.binary_name.is_empty(),
                "{} does not say what binary it looks for",
                row.id
            );
        }
    }

    /// The binary name shown must be one detection actually tries, or the row
    /// tells the user to look for the wrong thing when detection is wrong.
    #[test]
    fn the_binary_name_shown_is_one_that_is_probed_for() {
        for spec in registry::AGENTS {
            let shown = row(spec, &ModelCatalog::fallback(spec), UpdateCheck::NotChecked).binary_name;
            assert!(
                spec.candidate_names().contains(&shown.as_str()),
                "{} shows {shown} but probes for {:?}",
                spec.id,
                spec.candidate_names()
            );
        }
    }

    #[test]
    fn a_chatty_version_string_is_cut_down_to_one_line() {
        // Copilot really does answer with three lines, the last of which is
        // advice about updating. The picker shows this in a small monospace
        // note, so the whole thing would wrap into a paragraph there.
        let raw = "GitHub Copilot CLI 1.0.81\n\nYou are running the latest version.";
        let short = short_version(raw);
        assert_eq!(short, "GitHub Copilot CLI 1.0.81");
        assert!(!short.contains('\n'));
    }

    #[test]
    fn a_short_version_is_left_exactly_as_the_tool_wrote_it() {
        // opencode answers with a bare number. Nothing is parsed or
        // reformatted: the string belongs to the tool.
        assert_eq!(short_version("1.18.10"), "1.18.10");
        assert_eq!(short_version("  1.18.10  "), "1.18.10");
    }

    #[test]
    fn a_very_long_first_line_is_truncated_rather_than_wrapped() {
        let long = "x".repeat(200);
        let short = short_version(&long);
        assert!(short.chars().count() <= 40, "got {} chars", short.chars().count());
        assert!(short.ends_with('…'));
    }

    #[test]
    fn a_tool_that_printed_nothing_does_not_panic() {
        assert_eq!(short_version(""), "");
        assert_eq!(short_version("

"), "");
    }

    /// Which intents can never write, in either start state.
    ///
    /// Pins the set the picker greys a tool out for. Checked against
    /// `check_tool_capability` -- the function the engine's own tool gate
    /// calls -- rather than `for_intent(..).can_write`, because those two
    /// disagree for Plan on purpose: the static table says `false` since a
    /// Plan cannot write *before Start*, while a started Plan may. Reading
    /// the wrong one of the two would grey out a usable tool for every
    /// started Plan chat.
    #[test]
    fn only_intents_that_never_write_are_treated_as_read_only() {
        use crate::agentdesk::model::SessionIntent;
        use crate::agentdesk::policy::{check_tool_capability, ToolCapability};

        let never_writes = |intent: SessionIntent| {
            [false, true].into_iter().all(|started| {
                check_tool_capability(intent, started, ToolCapability::EditFile).is_err()
            })
        };

        let read_only: Vec<&str> = [
            (SessionIntent::Ask, "ask"),
            (SessionIntent::Explain, "explain"),
            (SessionIntent::Plan, "plan"),
            (SessionIntent::Fix, "fix"),
            (SessionIntent::Review, "review"),
            (SessionIntent::Summarize, "summarize"),
        ]
        .into_iter()
        .filter(|(i, _)| never_writes(*i))
        .map(|(_, name)| name)
        .collect();

        assert_eq!(
            read_only,
            vec!["ask", "explain", "review", "summarize"],
            "the picker greys a tool out for exactly these"
        );
    }

    /// The half of the Plan contract the list above depends on.
    #[test]
    fn plan_is_read_only_before_start_and_writable_after() {
        use crate::agentdesk::model::SessionIntent;
        use crate::agentdesk::policy::{check_tool_capability, ToolCapability};
        assert!(
            check_tool_capability(SessionIntent::Plan, false, ToolCapability::EditFile).is_err(),
            "a plan must not write before Start"
        );
        assert!(
            check_tool_capability(SessionIntent::Plan, true, ToolCapability::EditFile).is_ok(),
            "a started plan writes, which is why plan is not in READ_ONLY_INTENTS"
        );
    }

    #[test]
    fn a_missing_tool_is_reported_as_not_installed_rather_than_too_old() {
        // The two states need different words -- one asks for an install, the
        // other for an update -- so they must never collapse into each other.
        let rows = list_without_asking_any_tool();
        for row in &rows {
            assert!(
                !(row.installed && row.too_old),
                "{} cannot be both usable and too old",
                row.id
            );
            if row.too_old {
                assert!(row.version.is_some(), "{} must report what it found", row.id);
            }
        }
    }
}

/// Prints the real picker rows for this machine.
///
/// Ignored by default because it spawns every installed tool and its answer
/// depends on what is on the box. Run it deliberately with
/// `cargo test --lib real_picker_rows -- --ignored --nocapture` to see exactly
/// what the picker will show before trusting a screenshot.
#[cfg(test)]
#[test]
#[ignore]
fn real_picker_rows_on_this_machine() {
    for row in list_without_asking_any_tool() {
        println!(
            "{:<16} installed={:<5} tooOld={:<5} readOnlyOk={:<5} version={:?}",
            row.id, row.installed, row.too_old, row.can_do_read_only_work, row.version
        );
        if let Some(limit) = &row.read_only_limit {
            println!("{:<16}   limit: {limit}", "");
        }
    }
}
