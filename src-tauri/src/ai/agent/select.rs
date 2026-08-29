//! Choosing which agent runs a piece of work, and refusing the ones that
//! cannot be trusted with it.
//!
//! This module used to be a leftover: a single function turning an
//! [`AgentError`] into a sentence, left behind when a multi-transport design
//! was cut. It has a real job now, and it is the security-critical one.
//!
//! ## The rule
//!
//! Some agent tools cannot be told to refuse a tool at all -- opencode is the
//! one in the table today. For those, GitWyrm cannot promise that a session
//! will not change files. Asking nicely in the system prompt is not a promise;
//! a prompt is a request the model can ignore, and the whole reason the launch
//! flags exist is that the engine's own permission handler only fires when the
//! agent chooses to ask.
//!
//! So an agent with no way to deny a tool is refused for every piece of work
//! that must not change anything: Ask, Explain, Review, Summarize, and a Plan
//! before the user presses Start. It stays available for Fix and for a started
//! Plan, where writing is the point.
//!
//! Read-only is not re-derived here. [`crate::agentdesk::policy`] already owns
//! that question -- the same table the engine's own tool-dispatch gate reads --
//! and this module asks it rather than keeping a second list of intents that
//! could drift out of step.

use super::registry::{self, AgentSpec};
use super::transport::{AgentError, Transport};
use crate::agentdesk::policy::{ExecutionPolicy, ToolCapability};

/// Picks the agent for one execution, or refuses to start it.
///
/// `started` is the session's own "has Start been pressed" flag, threaded
/// through unchanged from the caller -- it is what makes a Plan read-only
/// before Start and writable after, and it is the reason this takes the flag
/// rather than reading the intent alone.
///
/// The refusal is a [`AgentError::TransportUnavailable`], the same shape as a
/// missing tool, because from the user's side it is the same kind of problem:
/// the thing they picked cannot do this job, and the message says which thing
/// and why.
pub fn choose(policy: &ExecutionPolicy, started: bool) -> Result<&'static AgentSpec, AgentError> {
    let id = policy.agent_id();
    let spec = registry::find(id).ok_or_else(|| AgentError::TransportUnavailable {
        transport: Transport::Cli,
        detail: format!("GitWyrm does not know how to use an AI tool called \"{id}\""),
    })?;

    if must_not_change_anything(policy, started) && !spec.can_guarantee_read_only() {
        return Err(refuse_read_only(spec));
    }

    Ok(spec)
}

/// The refusal itself, so the last-moment check in `cli_agent::connect` says
/// the same thing this one does rather than building a second message.
pub fn refuse_read_only(spec: &AgentSpec) -> AgentError {
    AgentError::TransportUnavailable {
        transport: Transport::Cli,
        detail: read_only_refusal(spec),
    }
}

/// Whether this execution, in this state, is one that must not change
/// anything.
///
/// Asks the policy table the same question the engine's tool gate asks: may
/// this execution edit a file? A "no" is exactly the read-only case, and it
/// covers both shapes of no -- an intent that can never write (Ask, Explain,
/// Review, Summarize) and one that cannot write yet (Plan before Start) --
/// without this module needing to know which intents those are.
pub fn must_not_change_anything(policy: &ExecutionPolicy, started: bool) -> bool {
    policy
        .check_tool_capability(started, ToolCapability::EditFile)
        .is_err()
}

/// What the user is told when their chosen tool cannot be trusted with
/// read-only work.
///
/// Names the tool, says plainly what it cannot do, and says what would work
/// instead. Never suggests the work is impossible or that GitWyrm is broken.
fn read_only_refusal(spec: &AgentSpec) -> String {
    format!(
        "{} cannot be told to leave your files alone, so GitWyrm will not use it for work that is only \
         supposed to look and not change anything. Pick a different AI tool for this, or use {} for work \
         that is meant to make changes",
        spec.display_name, spec.display_name
    )
}

/// One sentence explaining an engine failure, in words a beginner can act on.
///
/// Never implies GitWyrm is broken: every branch names the thing that is
/// missing and what would fix it.
pub fn plain_explanation(err: &AgentError) -> String {
    match err {
        AgentError::TransportUnavailable { detail, .. } => detail.clone(),
        AgentError::NeedsReconnect { detail } => detail.clone(),
        AgentError::Refused { detail } => {
            format!("Your AI provider turned this down: {detail}")
        }
        AgentError::Cancelled => "Stopped.".into(),
        AgentError::Failed { detail } => format!("This didn't work: {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::SessionIntent;
    use crate::agentdesk::policy::{ExecutionMode, ExecutionTeam};

    /// Every intent whose work must not change anything, in the states where
    /// that is true. Plan appears with `started: false` only -- after Start it
    /// is a writing session.
    fn read_only_cases() -> Vec<(SessionIntent, bool)> {
        let mut cases = Vec::new();
        for intent in [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Review,
            SessionIntent::Summarize,
        ] {
            cases.push((intent, false));
            cases.push((intent, true));
        }
        cases.push((SessionIntent::Plan, false));
        cases
    }

    fn policy_for(intent: SessionIntent, agent: &str) -> ExecutionPolicy {
        ExecutionPolicy::resolve(intent, ExecutionMode::Auto, ExecutionTeam::Lead, Some(agent))
            .expect("the agent name must be one this build supports")
    }

    /// THE core proof of this module's reason to exist: a tool with no way to
    /// deny anything is refused for every piece of read-only work, in every
    /// state.
    ///
    /// The current read-only guarantee rests entirely on the launched tool
    /// being unable to write. opencode has no tool-restriction flags at all,
    /// so letting it run an Ask or a Review would quietly turn a promise into
    /// a hope.
    #[test]
    fn an_agent_that_cannot_deny_anything_is_refused_for_every_read_only_intent() {
        for (intent, started) in read_only_cases() {
            let policy = policy_for(intent, "opencode");
            let outcome = choose(&policy, started);
            assert!(
                outcome.is_err(),
                "{intent:?} (started={started}) must not run on a tool that cannot be told to leave files alone"
            );
        }
    }

    /// The same tool is fine for work that is meant to change things -- the
    /// refusal is scoped to the read-only promise, not a ban on the tool.
    #[test]
    fn an_agent_that_cannot_deny_anything_still_runs_work_meant_to_make_changes() {
        let fix = policy_for(SessionIntent::Fix, "opencode");
        assert!(choose(&fix, false).is_ok(), "Fix may use it");
        assert!(choose(&fix, true).is_ok(), "Fix may use it once started");

        let plan = policy_for(SessionIntent::Plan, "opencode");
        assert!(
            choose(&plan, true).is_ok(),
            "a Plan that has been started is a writing session"
        );
    }

    /// Every tool that CAN be told to refuse something is allowed everywhere,
    /// including the default. A regression here would break the shipping
    /// path, not just the new ones.
    #[test]
    fn every_agent_that_can_deny_something_is_allowed_for_read_only_work() {
        for agent in ["copilot", "gemini", "claude"] {
            for (intent, started) in read_only_cases() {
                let policy = policy_for(intent, agent);
                let chosen = choose(&policy, started)
                    .unwrap_or_else(|e| panic!("{agent} must run {intent:?}: {e}"));
                assert_eq!(chosen.id, agent);
            }
        }
    }

    #[test]
    fn the_default_execution_still_chooses_copilot() {
        let policy =
            ExecutionPolicy::resolve(SessionIntent::Review, ExecutionMode::Ask, ExecutionTeam::Solo, None)
                .expect("no override must always resolve");
        assert_eq!(choose(&policy, false).expect("the default must run").id, "copilot");
    }

    /// A helper execution is not gated by `SessionIntent` the way a solo or
    /// lead one is, so this proves the refusal follows the policy's own
    /// write authority rather than an intent name -- a read-only helper is
    /// refused, a writing helper is not.
    #[test]
    fn a_read_only_helper_is_refused_a_tool_that_cannot_deny_anything() {
        let mut reader = ExecutionPolicy::resolve_for_helper(false, vec![]);
        reader.provider = crate::agentdesk::policy::ExecutionProvider::OpenCode;
        assert!(choose(&reader, true).is_err(), "a read-only helper must be refused");

        let mut writer = ExecutionPolicy::resolve_for_helper(true, vec!["src/**".into()]);
        writer.provider = crate::agentdesk::policy::ExecutionProvider::OpenCode;
        assert!(choose(&writer, true).is_ok(), "a writing helper may use it");
    }

    /// The refusal has to read as "this tool cannot do this job", naming the
    /// tool, not as a fault in GitWyrm.
    #[test]
    fn the_read_only_refusal_names_the_tool_and_says_what_would_work() {
        let policy = policy_for(SessionIntent::Review, "opencode");
        let err = choose(&policy, false).expect_err("opencode must be refused");
        let msg = plain_explanation(&err);
        assert!(msg.contains("opencode"), "the message must name the tool: {msg}");
        for blame in ["error", "failed", "broken", "unexpected"] {
            assert!(
                !msg.to_lowercase().contains(blame),
                "{msg} reads as a GitWyrm fault"
            );
        }
    }

    #[test]
    fn read_only_is_read_from_the_policy_table_not_a_second_list() {
        // Every read-only case the policy table refuses an edit for is one
        // this module treats as read-only, and no others. If the two ever
        // disagreed, a tool could be allowed to run work the engine's own
        // gate considers read-only.
        for intent in [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Review,
            SessionIntent::Summarize,
            SessionIntent::Plan,
            SessionIntent::Fix,
        ] {
            for started in [false, true] {
                let policy = policy_for(intent, "copilot");
                assert_eq!(
                    must_not_change_anything(&policy, started),
                    policy
                        .check_tool_capability(started, ToolCapability::EditFile)
                        .is_err(),
                    "{intent:?} (started={started})"
                );
            }
        }
    }

    #[test]
    fn no_explanation_blames_gitwyrm() {
        let errors = [
            AgentError::TransportUnavailable {
                transport: Transport::Cli,
                detail: "the Copilot command-line tool is not installed".into(),
            },
            AgentError::NeedsReconnect {
                detail: "sign in to Copilot in settings".into(),
            },
            AgentError::Refused {
                detail: "rate limited".into(),
            },
            AgentError::Failed {
                detail: "the connection to the tool closed".into(),
            },
        ];
        for err in &errors {
            let msg = plain_explanation(err);
            for blame in ["error", "failed", "broken", "unexpected"] {
                assert!(
                    !msg.to_lowercase().contains(blame),
                    "{msg} reads as a GitWyrm fault"
                );
            }
            assert!(!msg.is_empty());
        }
    }
}
