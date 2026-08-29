//! The CLI transport: an agent connection backed by a command-line tool's own
//! ACP server.
//!
//! Exists so a subscription-only user is not shut out. GitWyrm never reads the
//! tool's stored credentials -- it asks the tool whether it can work and
//! believes the answer, which is the whole difference between driving a tool
//! and impersonating one.
//!
//! Nothing here names a particular tool. Which one to use comes from the
//! execution's own policy, and how to find and start it comes from
//! [`super::registry`]. Copilot is still the default, and the path it takes
//! through this module is unchanged.

use std::path::PathBuf;

use crate::agentdesk::policy::ExecutionPolicy;

use super::acp::AcpConnection;
use super::copilot_cli::{self, CliState};
use super::registry::AgentSpec;
use super::select;
use super::transport::{AgentError, Transport};

/// Permission kinds denied to the CLI's own agent for the whole run,
/// unconditionally.
///
/// Verified against Copilot CLI 1.0.76 (`copilot help permissions`). The
/// vocabulary is permission *kinds* -- `shell(command)`, `write(path)`,
/// `url(domain)`, and MCP server names -- not the file-operation names an
/// earlier reading of the docs suggested.
///
/// Denials rather than an allow-list, because `--available-tools` names what
/// the model can see while `--deny-tool` is what it cannot use, and denial
/// takes precedence over every allow rule including `--allow-all-tools`. That
/// precedence is the property worth having: it cannot be widened by anything
/// the model or a config file says later.
///
/// `shell` and `url` are denied outright, always. Those are the side effects
/// the engine gates itself, and a tool the CLI never has is one that cannot
/// slip past a gate. `write` is NOT in this fixed list -- whether it is
/// denied depends on the execution's own policy (see [`denied_tools_for`]):
/// editing files is the job for Fix and a started Plan, but must be refused
/// at CLI launch (not just at the engine's own permission-request handler,
/// which only fires when the provider chooses to ask) for every read-only
/// intent and for Plan before Start.
const ALWAYS_DENIED_TOOLS: &[&str] = &["shell", "url"];

/// The full `--deny-tool` set for one execution: [`ALWAYS_DENIED_TOOLS`] plus
/// `"write"` whenever this execution's policy says it must never write.
///
/// This is the actual P0 fix for "read-only is advisory when the provider
/// does not ask": before this function existed, `DENIED_TOOLS` was a single
/// process-wide constant that never included `"write"`, so a Review or
/// Summarize execution launched the CLI with write capability -- the
/// engine-boundary refusal in `airun::cli_run::handle` only fires when the
/// provider chooses to ASK for permission, and provider-side allow rules
/// (remembered approvals, `--allow-all-tools`, config) can suppress that ask
/// entirely, in which case nothing ever stopped the write. Denying at launch
/// closes that gap: per `copilot help permissions`, a `--deny-tool` takes
/// precedence over every allow rule the CLI itself might apply, including
/// `--allow-all-tools` -- so this is not merely "ask nicely," it is a bound
/// the CLI cannot be talked out of.
///
/// `started` mirrors `run_task`'s own parameter of the same name (see its
/// doc comment): a Plan execution denies write before the user's Start
/// action and allows it after, exactly matching
/// `policy::check_tool_capability`'s own "not yet" vs. "never" distinction
/// for [`policy::WorktreePolicy::NotUntilStart`] -- computed the same way
/// here (by asking `policy` directly) so the two enforcement layers (this
/// launch-time denial, and `handle`'s own runtime check) can never disagree
/// about which state a given (intent, started) pair is in.
pub fn denied_tools_for(policy: &ExecutionPolicy, started: bool) -> Vec<&'static str> {
    let mut tools = ALWAYS_DENIED_TOOLS.to_vec();
    let write_allowed = policy
        .check_tool_capability(started, crate::agentdesk::policy::ToolCapability::EditFile)
        .is_ok();
    if !write_allowed {
        tools.push("write");
    }
    tools
}

pub struct CliAgent {
    program: PathBuf,
    cwd: PathBuf,
    /// Which tool this is. Carried so [`Self::connect`] starts it the way that
    /// tool expects and words a failure with that tool's name.
    spec: &'static AgentSpec,
}

impl CliAgent {
    /// Builds the transport for the tool this execution's policy chose, or
    /// refuses.
    ///
    /// Two different refusals can come out of this, and they are different on
    /// purpose. One is "the tool you picked is not installed", which the user
    /// fixes by installing it. The other is "the tool you picked cannot be
    /// told to leave your files alone, and this job must not change anything"
    /// -- see [`select::choose`], which is where the read-only guarantee is
    /// actually held. Checking that BEFORE looking for the binary means a
    /// user who picks an unsuitable tool is told why it is unsuitable rather
    /// than being sent to install it first and refused afterwards.
    pub fn discover_for(policy: &ExecutionPolicy, started: bool, cwd: PathBuf) -> Result<Self, AgentError> {
        let spec = select::choose(policy, started)?;
        Self::discover_agent(spec, cwd)
    }

    /// Builds the transport if a usable copy of `spec`'s tool is installed.
    ///
    /// The version floor and the "not installed" case produce different
    /// sentences, because they need different actions from the user.
    pub fn discover_agent(spec: &'static AgentSpec, cwd: PathBuf) -> Result<Self, AgentError> {
        let name = spec.display_name;
        match &copilot_cli::detect_agent(spec).state {
            CliState::Ready { path, version } => {
                log::info!("{name} command-line tool ready: {version}");
                Ok(Self {
                    program: PathBuf::from(path),
                    cwd,
                    spec,
                })
            }
            CliState::TooOld { version, minimum } => Err(AgentError::TransportUnavailable {
                transport: Transport::Cli,
                // Copilot keeps its own update command in the message, because
                // naming the exact command is the fastest fix for the one
                // tool we know that command for. The others get the general
                // sentence rather than a guess at their update command.
                detail: if spec.id == "copilot" {
                    format!(
                        "the {name} command-line tool is version {version}, but {minimum} or newer is \
                         needed. Running `copilot update` will bring it up to date"
                    )
                } else {
                    format!(
                        "the {name} command-line tool is version {version}, but {minimum} or newer is \
                         needed. Updating it will fix this"
                    )
                },
            }),
            CliState::NotFound => Err(AgentError::TransportUnavailable {
                transport: Transport::Cli,
                detail: format!("the {name} command-line tool is not installed"),
            }),
        }
    }

    /// Opens a session, proving the tool is installed *and* signed in.
    ///
    /// This is the check that matters: a credential file can look complete while
    /// its sign-in lacks the scope the tool needs, which is the spike's own
    /// failure case. Only the tool can answer that, so we ask it.
    ///
    /// `policy`/`started` decide the `--deny-tool` set via
    /// [`denied_tools_for`] -- this is the actual enforcement point for
    /// "deny write at provider launch for every read-only operation and Plan
    /// before Start": the CLI is spawned with `write` in its denied-tools
    /// list for exactly those cases, so a remembered approval or
    /// `--allow-all-tools` inside the CLI can never grant what was refused
    /// here (denial takes precedence over every allow rule -- see
    /// `denied_tools_for`'s own doc comment).
    pub async fn connect(&self, policy: &ExecutionPolicy, started: bool) -> Result<AcpConnection, AgentError> {
        // Checked again here, not only in `discover_for`. A `CliAgent` can be
        // built for the default tool by a caller that had no policy yet
        // (`discover`), so this is the last point before a process starts
        // where the read-only promise can still be kept.
        if select::must_not_change_anything(policy, started) && !self.spec.can_guarantee_read_only() {
            return Err(select::refuse_read_only(self.spec));
        }
        let denied = denied_tools_for(policy, started);
        let mut conn = AcpConnection::spawn_agent(self.spec, &self.program, &self.cwd, &denied).await?;
        conn.start_session(&self.cwd).await?;
        Ok(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::SessionIntent;
    use crate::agentdesk::policy::{ExecutionMode, ExecutionTeam};

    fn policy_for(intent: SessionIntent) -> ExecutionPolicy {
        ExecutionPolicy::resolve(intent, ExecutionMode::Auto, ExecutionTeam::Lead, None)
            .expect("no provider override, cannot fail")
    }

    #[test]
    fn shell_and_network_access_are_always_denied() {
        // The engine gates side effects itself; a tool the CLI never has cannot
        // slip past a gate at all. Guards against a denial being dropped, for
        // every intent regardless of write authority.
        for intent in [SessionIntent::Review, SessionIntent::Fix] {
            for started in [false, true] {
                let denied = denied_tools_for(&policy_for(intent), started);
                assert!(denied.contains(&"shell"), "the CLI must not run shell commands");
                assert!(denied.contains(&"url"), "the CLI must not reach the network");
            }
        }
    }

    /// THE core proof for P0-B (task: "deny write at provider launch for
    /// every read-only operation and Plan before Start"): a read-only
    /// intent's `--deny-tool` set CONTAINS `write`, regardless of `started`.
    #[test]
    fn a_read_only_intent_denies_write_at_launch() {
        for intent in [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Summarize,
            SessionIntent::Review,
        ] {
            for started in [false, true] {
                let denied = denied_tools_for(&policy_for(intent), started);
                assert!(
                    denied.contains(&"write"),
                    "{intent:?} (started={started}) must deny write at CLI launch, got {denied:?}"
                );
            }
        }
    }

    /// Fix may write once isolated -- unconditionally, not gated on
    /// `started` -- so `write` must NOT be in its denied-tools set.
    #[test]
    fn fix_does_not_deny_write() {
        for started in [false, true] {
            let denied = denied_tools_for(&policy_for(SessionIntent::Fix), started);
            assert!(
                !denied.contains(&"write"),
                "Fix (started={started}) must not deny write, got {denied:?}"
            );
        }
    }

    /// Plan denies write before Start and allows it after -- the one case
    /// where the same intent's launch-time denial differs by `started`,
    /// mirroring `policy::check_tool_capability`'s own "not yet" vs. "never"
    /// distinction for `WorktreePolicy::NotUntilStart`.
    #[test]
    fn plan_denies_write_before_start_but_not_after() {
        let policy = policy_for(SessionIntent::Plan);
        let before = denied_tools_for(&policy, false);
        assert!(before.contains(&"write"), "Plan before Start must deny write, got {before:?}");
        let after = denied_tools_for(&policy, true);
        assert!(!after.contains(&"write"), "Plan after Start must not deny write, got {after:?}");
    }

    #[test]
    fn denied_tools_use_the_clis_own_vocabulary() {
        // Verified against `copilot help permissions` on 1.0.76: the kinds are
        // shell, write, url, and MCP server names. An earlier list guessed at
        // file-operation names ("view", "edit", "ls") that the CLI does not
        // recognise -- and silently ignores rather than rejecting, so a wrong name
        // here would look like it worked while filtering nothing.
        const KINDS: &[&str] = &["shell", "write", "url"];
        for intent in [SessionIntent::Review, SessionIntent::Fix, SessionIntent::Plan] {
            for started in [false, true] {
                for tool in denied_tools_for(&policy_for(intent), started) {
                    assert!(KINDS.contains(&tool), "{tool} is not a permission kind the CLI knows");
                }
            }
        }
    }

    /// A helper's own resolved policy (never gated by `SessionIntent`
    /// directly, see `ExecutionPolicy::resolve_for_helper`) still denies
    /// write at launch when the helper cannot write -- proving this launch-
    /// time gate composes with the helper path, not just the solo/lead one.
    #[test]
    fn a_read_only_helper_denies_write_at_launch() {
        let policy = ExecutionPolicy::resolve_for_helper(false, vec![]);
        let denied = denied_tools_for(&policy, true);
        assert!(denied.contains(&"write"));
    }

    #[test]
    fn a_writing_helper_does_not_deny_write_at_launch() {
        let policy = ExecutionPolicy::resolve_for_helper(true, vec!["src/**".into()]);
        let denied = denied_tools_for(&policy, true);
        assert!(!denied.contains(&"write"));
    }

    /// The denials this function produces are GitWyrm's own words, and every
    /// tool that denies by name must have a word of its own for each of them
    /// -- otherwise a denial is silently dropped at launch and the run is
    /// less restricted than the policy said.
    #[test]
    fn every_denial_this_produces_is_one_the_tools_can_actually_express() {
        use super::super::registry::{self, Denial};
        for intent in [SessionIntent::Review, SessionIntent::Fix, SessionIntent::Plan] {
            for started in [false, true] {
                let denied = denied_tools_for(&policy_for(intent), started);
                for spec in registry::AGENTS {
                    if !matches!(spec.denial, Denial::LaunchFlags { .. } | Denial::SessionMeta) {
                        continue;
                    }
                    // Every denial has to survive the translation into this
                    // tool's own vocabulary. A dropped one would leave the
                    // launched tool holding a capability the policy refused.
                    let on_command_line = spec
                        .launch_args(&denied)
                        .len()
                        .saturating_sub(spec.acp_args.len());
                    let in_session_meta = spec
                        .session_meta(&denied)
                        .and_then(|m| {
                            m["claudeCode"]["options"]["disallowedTools"]
                                .as_array()
                                .map(Vec::len)
                        })
                        .unwrap_or(0);
                    let expressed = on_command_line + in_session_meta;
                    assert_eq!(
                        expressed,
                        denied.len(),
                        "{} dropped a denial from {denied:?} ({intent:?}, started={started})",
                        spec.id
                    );
                }
            }
        }
    }

    /// THE safety rule, at the launch boundary: a tool with no way to refuse
    /// anything must be turned down for every piece of read-only work, before
    /// any process starts.
    ///
    /// `discover_for` refuses before it even looks for the binary, so this
    /// holds on a machine where the tool is not installed too -- which is what
    /// makes it a test rather than a manual check.
    #[test]
    fn a_tool_that_cannot_refuse_anything_is_turned_down_for_read_only_work() {
        let read_only: &[(SessionIntent, bool)] = &[
            (SessionIntent::Ask, false),
            (SessionIntent::Ask, true),
            (SessionIntent::Explain, false),
            (SessionIntent::Explain, true),
            (SessionIntent::Review, false),
            (SessionIntent::Review, true),
            (SessionIntent::Summarize, false),
            (SessionIntent::Summarize, true),
            (SessionIntent::Plan, false),
        ];
        for (intent, started) in read_only.iter().copied() {
            let policy = ExecutionPolicy::resolve(
                intent,
                ExecutionMode::Auto,
                ExecutionTeam::Lead,
                Some("opencode"),
            )
            .expect("opencode is a tool this build knows");
            let outcome = CliAgent::discover_for(&policy, started, std::env::temp_dir());
            assert!(
                outcome.is_err(),
                "{intent:?} (started={started}) must not launch on a tool that cannot be told to leave files alone"
            );
        }
    }
}
