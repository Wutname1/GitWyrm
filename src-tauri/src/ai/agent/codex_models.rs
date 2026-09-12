//! What models the installed Codex offers, asked of Codex and remembered.
//!
//! The registry carries a hand-written list (`registry::AGENTS`, the `codex`
//! row) that was correct against codex-cli 0.151.0 and wrong by 0.154.0: it
//! named two `gpt-5.4` models where the tool had moved to six, none of them
//! 5.4. A compile-time constant cannot notice that, which is the fault this
//! module exists to remove -- not the particular names it got wrong.
//!
//! The registry list stays, as the fallback and only as the fallback. It is
//! what the picker shows when Codex is not installed, is too old to answer, or
//! fails -- a stale list beats an empty menu -- and `ModelSource` keeps the
//! difference visible so it can never quietly become the source of truth
//! again.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::codex::{CodexConnection, CodexModel};
use super::copilot_cli::{detect_agent, CliState};
use super::registry::{AgentSpec, ModelSupport};

/// Where a model list came from.
///
/// The same distinction `UsageSource` draws for usage figures: a value GitWyrm
/// obtained is not the same kind of thing as one it fell back to, and the UI
/// must be able to tell the reader which it is holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModelSource {
    /// The tool's own answer, rather than a list GitWyrm made up.
    ///
    /// This axis is WHO AUTHORED the list, not how recently it arrived. An
    /// answer restored from disk is still the tool's own answer -- it was
    /// asked, and the reply was written down verbatim -- and it is only
    /// restored while the executable path and version match the install in
    /// front of us. How fresh an answer is has its own mechanism in `TTL`,
    /// and how old a remembered one is has its own field in `written_at`;
    /// neither belongs here.
    ///
    /// Recorded rather than left implicit because it was argued twice: the
    /// alternative was a third variant for remembered answers, which would
    /// hedge a value that does not warrant hedging and put a word on screen
    /// nobody can act on.
    Live,
    /// The tool could not be asked, so this is the list built into GitWyrm.
    /// It may be out of date, and says so.
    Fallback,
}

/// A model list plus where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalog {
    pub models: Vec<CodexModel>,
    pub source: ModelSource,
}

impl ModelCatalog {
    /// The compile-time list, as the honest second choice.
    ///
    /// Carries no efforts and no default: the registry row never knew either,
    /// and inventing them here would be the same guess that made the model
    /// names wrong. An empty `efforts` means the effort control is not offered,
    /// which is the truthful reading of "GitWyrm cannot ask this tool".
    pub fn fallback(spec: &AgentSpec) -> Self {
        let models = match spec.model {
            ModelSupport::Flag { choices, .. } => choices
                .iter()
                .map(|c| CodexModel {
                    id: c.id.to_string(),
                    display_name: c.display_name.to_string(),
                    description: String::new(),
                    is_default: false,
                    efforts: Vec::new(),
                    default_effort: None,
                })
                .collect(),
            ModelSupport::None => Vec::new(),
        };
        Self { models, source: ModelSource::Fallback }
    }
}

/// How long a live answer is trusted before Codex is asked again.
///
/// Long enough that opening the picker repeatedly costs nothing, short enough
/// that a tool updated while GitWyrm is running is noticed without a restart --
/// which is exactly how the stale list went unseen in the first place.
const TTL: Duration = Duration::from_secs(15 * 60);

/// Guards against a binary that starts but never answers.
const LIST_TIMEOUT: Duration = Duration::from_secs(20);

struct Cached {
    catalog: ModelCatalog,
    at: Instant,
}

static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Cached>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drops what was remembered, so the next read asks again.
pub fn forget_all_cached() {
    if let Ok(mut guard) = cache().lock() {
        guard.clear();
    }
}

/// Codex's models: live when it can be asked, the built-in list when it cannot.
///
/// Only a live answer is remembered. A failure is re-tried next time, for the
/// same reason `copilot_cli` never caches "not installed": somebody who reads
/// that their tool could not be reached and then fixes it must not keep being
/// told the old answer until they restart the app.
pub async fn catalog(spec: &'static AgentSpec) -> ModelCatalog {
    if let Ok(guard) = cache().lock() {
        if let Some(hit) = guard.get(spec.id) {
            if hit.at.elapsed() < TTL {
                return hit.catalog.clone();
            }
        }
    }

    match ask(spec).await {
        Ok(models) if !models.is_empty() => {
            let catalog = ModelCatalog { models, source: ModelSource::Live };
            if let Ok(mut guard) = cache().lock() {
                guard.insert(spec.id.to_string(), Cached { catalog: catalog.clone(), at: Instant::now() });
            }
            catalog
        }
        Ok(_) => {
            // Answered, with nothing in it. Not cached: an empty answer is far
            // more likely to be a tool mid-update than a real statement that it
            // has no models, and the built-in list is the better guess either
            // way.
            log::warn!("codex models: the tool answered with an empty list; using the built-in one");
            ModelCatalog::fallback(spec)
        }
        Err(detail) => {
            log::info!("codex models: could not ask the tool ({detail}); using the built-in list");
            ModelCatalog::fallback(spec)
        }
    }
}

/// One round trip to the tool, or a sentence saying why there was not one.
async fn ask(spec: &'static AgentSpec) -> Result<Vec<CodexModel>, String> {
    // Reuses the detection every other surface uses, rather than a second
    // copy of the PATH walk. It is already cached, and it distinguishes "not
    // installed" from "found but did not answer" -- both of which are reasons
    // not to ask, with different words.
    let path = match detect_agent(spec).state {
        CliState::Ready { path, .. } => path,
        CliState::TooOld { version, minimum } => {
            return Err(format!("it is version {version} and GitWyrm needs {minimum} or newer"))
        }
        CliState::NotFound => return Err("it is not installed".to_string()),
        CliState::FoundButUnresponsive { .. } => return Err("it did not answer when asked its version".to_string()),
    };
    let path = std::path::PathBuf::from(path);
    let args: Vec<String> = spec.acp_args.iter().map(|a| (*a).to_string()).collect();
    // The current directory is irrelevant to listing models -- no thread is
    // started and no repository is touched -- but the process needs one that
    // exists, and a repository would tie a global answer to whichever chat
    // happened to ask first.
    let cwd = std::env::temp_dir();

    let run = async {
        let conn = CodexConnection::spawn(&path, &cwd, &args)
            .await
            .map_err(|e| e.to_string())?;
        let models = conn.list_models().await.map_err(|e| e.to_string());
        conn.shutdown().await;
        models
    };

    match tokio::time::timeout(LIST_TIMEOUT, run).await {
        Ok(result) => result,
        Err(_) => Err("it did not answer in time".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::agent::registry;

    fn codex() -> &'static AgentSpec {
        registry::find("codex").expect("codex row")
    }

    /// The defect this module exists for: a hand-written list that could not
    /// tell when it had gone stale. It is still here, deliberately, as the
    /// answer when the tool cannot be asked -- so it must say that is what it
    /// is.
    #[test]
    fn the_built_in_list_is_labelled_as_a_fallback() {
        let catalog = ModelCatalog::fallback(codex());
        assert_eq!(catalog.source, ModelSource::Fallback);
        assert!(
            !catalog.models.is_empty(),
            "a fallback with nothing in it would leave the picker empty, which is what it exists to prevent"
        );
    }

    /// "GitWyrm could not ask" and "the tool offers nothing" are different
    /// facts and must not render the same. The source is what tells them
    /// apart, which is why it travels with the list rather than being inferred
    /// from its length.
    #[test]
    fn a_fallback_list_is_distinguishable_from_an_empty_one() {
        let fallback = ModelCatalog::fallback(codex());
        let empty = ModelCatalog { models: Vec::new(), source: ModelSource::Live };
        assert_ne!(fallback.source, empty.source);
        assert!(!fallback.models.is_empty() && empty.models.is_empty());
    }

    /// The fallback claims nothing it cannot back up. The registry row never
    /// knew which model Codex prefers or how hard each can think, and
    /// inventing either would be the same guess that made the names wrong.
    #[test]
    fn the_fallback_invents_no_default_and_no_efforts() {
        for model in ModelCatalog::fallback(codex()).models {
            assert!(!model.is_default, "{} claims to be the default", model.id);
            assert!(model.efforts.is_empty(), "{} claims efforts nobody asked about", model.id);
            assert!(model.default_effort.is_none());
        }
    }

    /// The real thing, against the installed Codex. Ignored by default so CI
    /// and machines without it are not gated on a subprocess; run with
    /// `cargo test --lib codex_models -- --ignored --nocapture`.
    ///
    /// This is the check the hardcoded list never had: it fails the day Codex
    /// stops answering `model/list`, instead of the list quietly going stale
    /// for three releases.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn the_installed_codex_answers_with_its_own_models() {
        forget_all_cached();
        let catalog = catalog(codex()).await;
        for m in &catalog.models {
            println!(
                "{:<26} default={:<5} efforts={}",
                m.id,
                m.is_default,
                m.efforts.join("/")
            );
        }
        assert_eq!(
            catalog.source,
            ModelSource::Live,
            "Codex is installed but its model list could not be read"
        );
        // `Live` alone is not proof this ran: a remembered answer restored
        // from disk carries the same label, correctly, because it is equally
        // the tool's own answer. This test exists to catch `model/list`
        // breaking, so it must prove the round trip happened rather than that
        // a label says it did.
        //
        // `catalog` never reads the disk -- persistence lives one layer up, in
        // `commands::agent_providers` -- and `forget_all_cached` above clears
        // the in-process one, so reaching the tool is the only way to get here.
        // Asserted rather than left to that layering, because a later build
        // that teaches this module to restore would silently disarm the one
        // check on the live protocol.
        assert!(
            catalog.models.iter().any(|m| !m.efforts.is_empty()),
            "no model reported its reasoning efforts, which only the live protocol carries"
        );
        assert!(!catalog.models.is_empty());
        assert!(
            catalog.models.iter().any(|m| m.is_default),
            "no model is marked default, so the picker cannot show which one Codex would choose"
        );
        assert!(
            catalog.models.iter().any(|m| !m.efforts.is_empty()),
            "no model reported how hard it can be asked to think"
        );
    }

    /// A tool that takes no model flag falls back to nothing, not to a made-up
    /// row -- "this tool has no model control" is its own true answer.
    #[test]
    fn a_tool_with_no_model_flag_falls_back_to_an_empty_list() {
        let opencode = registry::find("opencode").expect("opencode row");
        if matches!(opencode.model, ModelSupport::None) {
            assert!(ModelCatalog::fallback(opencode).models.is_empty());
        }
    }
}
