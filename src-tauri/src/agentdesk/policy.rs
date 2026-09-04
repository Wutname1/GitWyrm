//! The intent policy table: what each [`SessionIntent`] defaults to and, most
//! importantly, what it is ALLOWED to do.
//!
//! design.md: "Policy is backend-owned and exhaustively tested." This module
//! is the single place that answers "can this session write files, and can it
//! touch a worktree" -- every enforcement point (kickoff, execution start,
//! tool dispatch) asks [`IntentPolicy::for_intent`] rather than re-deriving
//! the answer, so there is exactly one table to audit, not one convention
//! repeated at every call site.
//!
//! architecture.md section 9 is the source table this mirrors:
//!
//! | Intent    | Default mode | Default team | Writes | Worktree        |
//! |-----------|--------------|--------------|--------|------------------|
//! | Ask       | Ask          | Solo         | No     | No               |
//! | Explain   | Ask          | Solo         | No     | No               |
//! | Summarize | Ask          | Solo         | No     | No               |
//! | Review    | Ask          | Solo         | No     | No               |
//! | Plan      | Plan         | Lead         | No until Start | No until Start |
//! | Fix       | Auto         | Lead         | Yes    | Always isolated  |

use serde::{Deserialize, Serialize};
use specta::Type;

use super::model::SessionIntent;

/// `ask | plan | auto`. Kept in this module (not re-exported from
/// `commands::agent_desk::ExecutionMode`) as the canonical definition;
/// `agent_session_start_execution`'s own `ExecutionMode` predates this policy
/// table and is structurally identical, not the same Rust item -- see task
/// 1.1's shared-contract note in `commands::agent_desk`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    Ask,
    Plan,
    Auto,
}

/// `solo | lead`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionTeam {
    Solo,
    Lead,
}

/// Whether an intent may create/use an isolated worktree, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WorktreePolicy {
    /// Never provisions a worktree. Execution runs read-only against the
    /// existing checkout (which it also may not write to -- see
    /// [`IntentPolicy::can_write`]).
    Never,
    /// No worktree is created until a Plan-mode session is explicitly
    /// started (the user's Start action), at which point it behaves like
    /// [`WorktreePolicy::Always`].
    NotUntilStart,
    /// A worktree is provisioned before the first edit, every time. Refusing
    /// to fall back to the user's own checkout is enforced by the caller
    /// (task 5.2), not by this enum -- this only says isolation is
    /// mandatory, not how provisioning failure is handled.
    Always,
}

/// The full policy for one [`SessionIntent`]: defaults plus the two hard
/// permissions every other system in this package must consult before
/// letting a session touch the repository.
///
/// `can_write` and `worktree` are the enforcement surface task 4.5 exists to
/// prove: Review and Summarize (and Ask/Explain) report `can_write: false`,
/// and nothing downstream of [`IntentPolicy::for_intent`] has any other way
/// to decide whether a tool call is a write -- see
/// `PolicyGuard`/`ToolCapability` below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IntentPolicy {
    pub intent: SessionIntent,
    pub default_mode: ExecutionMode,
    pub default_team: ExecutionTeam,
    /// Whether this intent may ever call an edit/write tool (create, modify,
    /// delete a file; commit; push; post to a host). `false` means: refused
    /// unconditionally, not "refused until some later state" -- Review and
    /// Summarize can never write, full stop, regardless of mode/team
    /// overrides (task 4.5).
    pub can_write: bool,
    pub worktree: WorktreePolicy,
}

/// The complete, exhaustive policy table. A `match` over every
/// [`SessionIntent`] variant (not a `HashMap` or default fallback) so adding
/// a new intent without adding its policy row is a compile error, matching
/// this codebase's stance on `SessionSource` in `model.rs`.
pub fn for_intent(intent: SessionIntent) -> IntentPolicy {
    match intent {
        SessionIntent::Ask => IntentPolicy {
            intent,
            default_mode: ExecutionMode::Ask,
            default_team: ExecutionTeam::Solo,
            can_write: false,
            worktree: WorktreePolicy::Never,
        },
        SessionIntent::Explain => IntentPolicy {
            intent,
            default_mode: ExecutionMode::Ask,
            default_team: ExecutionTeam::Solo,
            can_write: false,
            worktree: WorktreePolicy::Never,
        },
        SessionIntent::Summarize => IntentPolicy {
            intent,
            default_mode: ExecutionMode::Ask,
            default_team: ExecutionTeam::Solo,
            can_write: false,
            worktree: WorktreePolicy::Never,
        },
        SessionIntent::Review => IntentPolicy {
            intent,
            default_mode: ExecutionMode::Ask,
            default_team: ExecutionTeam::Solo,
            can_write: false,
            worktree: WorktreePolicy::Never,
        },
        SessionIntent::Plan => IntentPolicy {
            intent,
            default_mode: ExecutionMode::Plan,
            default_team: ExecutionTeam::Lead,
            // Plan may inspect (read tools only) but cannot execute a write
            // before Start -- this table's `can_write: false` is the
            // "before Start" half of that contract. The "after Start it
            // behaves like Fix" half is a state transition the execution
            // engine performs (re-deriving `for_intent(SessionIntent::Fix)`
            // once the user starts it), not something this static table can
            // express for a single intent value.
            can_write: false,
            worktree: WorktreePolicy::NotUntilStart,
        },
        SessionIntent::Fix => IntentPolicy {
            intent,
            default_mode: ExecutionMode::Auto,
            default_team: ExecutionTeam::Lead,
            can_write: true,
            worktree: WorktreePolicy::Always,
        },
    }
}

/// Which AI command-line tool a session's execution is allowed to use.
///
/// This is the refusal boundary for task 6 ("Make unsupported provider
/// overrides fail visibly instead of silently using Copilot"). `None` means
/// "use the default," which resolves to `Copilot`. `Some(other)` that does not
/// name a tool this build knows must be refused by
/// [`ExecutionPolicy::resolve`] -- never silently downgraded to `Copilot`,
/// which is exactly the bug this type exists to close.
///
/// The variants mirror `ai::agent::registry::AGENTS` one for one. That table
/// is the authority on how each tool is found and started; this enum only
/// exists so a policy can be a plain `Copy` value the frontend can also see
/// (`specta::Type`), which a `&'static AgentSpec` cannot be. The two are kept
/// in step by [`Self::agent_id`] plus a test in the registry module that
/// checks every id here resolves there.
///
/// Naming a tool here does NOT mean it may run anything. Whether it may run a
/// particular piece of work depends on whether it can be told to refuse a
/// tool at all -- see `ai::agent::select::choose`, which turns down a tool
/// that cannot guarantee read-only for read-only work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionProvider {
    Copilot,
    Gemini,
    Claude,
    OpenCode,
    Codex,
}

impl ExecutionProvider {
    /// Parses a provider override string as it arrives from the frontend
    /// (`agent_session_start_execution`'s `provider_override: Option<String>`).
    /// Case-insensitive on the supported names; anything else is `None` -- the
    /// caller (`ExecutionPolicy::resolve`) treats a `Some` override that fails
    /// to parse as an unsupported-provider refusal, not as "no override was
    /// given."
    pub fn parse(name: &str) -> Option<Self> {
        // Matched against the same ids the agent registry uses, so a name
        // that works in settings works here and vice versa.
        for candidate in [
            ExecutionProvider::Copilot,
            ExecutionProvider::Gemini,
            ExecutionProvider::Claude,
            ExecutionProvider::OpenCode,
            ExecutionProvider::Codex,
        ] {
            if name.eq_ignore_ascii_case(candidate.agent_id()) {
                return Some(candidate);
            }
        }
        None
    }

    /// The registry id for this tool. The one place the two vocabularies meet.
    pub fn agent_id(self) -> &'static str {
        match self {
            ExecutionProvider::Copilot => "copilot",
            ExecutionProvider::Gemini => "gemini",
            ExecutionProvider::Claude => "claude",
            ExecutionProvider::OpenCode => "opencode",
            ExecutionProvider::Codex => "codex",
        }
    }
}

/// Why [`ExecutionPolicy::resolve`] refused to build a policy at all --
/// distinct from [`ToolRefusal`], which gates a single tool call inside an
/// already-running execution. This gates *starting* the execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PolicyRefusal {
    /// `provider_override` named something other than a provider this build
    /// actually supports. Task 1.6: this must surface as a typed outcome the
    /// UI can show, never fall through to starting Copilot anyway.
    UnsupportedProvider { requested: String },
}

/// The execution-wide authority for one running (or about-to-run) execution:
/// intent policy, the concrete mode/team the caller asked for, whether the
/// session has been explicitly started (only meaningful for Plan), and the
/// resolved provider. Built exactly once per execution
/// (`ExecutionPolicy::resolve`, called from `commands::agent_desk::start_execution_at`)
/// and threaded into both provider discovery (provider selection) and tool
/// dispatch (`cli_run::run_task`/`handle`) -- see task 1.1/1.2. No downstream
/// code re-derives any part of this from UI state; everything after
/// `resolve` reads fields off this struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPolicy {
    pub intent: SessionIntent,
    pub mode: ExecutionMode,
    pub team: ExecutionTeam,
    pub provider: ExecutionProvider,
    /// The intent's static write/worktree table -- kept alongside rather
    /// than re-derived at every call site.
    pub intent_policy: IntentPolicy,
    /// Repo-relative path globs a helper execution may write to (R6.4:
    /// "Enforce per-helper worktree, allowed paths ... Enforcement, not
    /// storage"). `None` for the lead and for solo executions, which are not
    /// path-scoped at all beyond their worktree boundary -- `Some(paths)`
    /// (even an empty `Vec`) marks this policy as belonging to a helper node,
    /// so [`Self::check_tool_capability`] can additionally reject an edit
    /// whose declared path falls outside every glob, on top of the ordinary
    /// intent-level write gate every execution already goes through.
    pub allowed_paths: Option<Vec<String>>,
}

impl ExecutionPolicy {
    /// Builds the policy for one execution, or refuses to start it at all.
    ///
    /// `mode`/`team` are accepted as explicit parameters (not silently
    /// replaced by `intent_policy.default_mode`/`default_team`) because a
    /// session may run with a caller-chosen mode/team that differs from the
    /// intent's default -- but a mismatched mode/team can never widen what
    /// the intent allows: [`Self::can_write`] and [`Self::check_tool_capability`]
    /// consult `intent_policy` (and, for Plan, `started`), never `mode`/`team`
    /// directly, which is what keeps Ask/Review/Summarize refused regardless
    /// of what `mode` a caller passes in.
    pub fn resolve(
        intent: SessionIntent,
        mode: ExecutionMode,
        team: ExecutionTeam,
        provider_override: Option<&str>,
    ) -> Result<Self, PolicyRefusal> {
        let provider = match provider_override {
            None => ExecutionProvider::Copilot,
            Some(name) => ExecutionProvider::parse(name).ok_or_else(|| {
                PolicyRefusal::UnsupportedProvider {
                    requested: name.to_string(),
                }
            })?,
        };
        Ok(Self {
            intent,
            mode,
            team,
            provider,
            intent_policy: for_intent(intent),
            allowed_paths: None,
        })
    }

    /// Builds the policy for one HELPER execution in a lead-and-helper graph
    /// (R6.4). A helper is never gated by [`SessionIntent`] the way a solo or
    /// lead execution is -- its write authority comes from
    /// `graph::ProposedHelperJob::role`/`allowed_paths` instead. `can_write`
    /// mirrors `SessionIntent::Fix` when the job may write at all (Builder,
    /// or any role with a non-empty path allowance -- the same rule
    /// `graph::validate_graph`'s `MissingAllowedPaths` check already enforces
    /// at proposal time) and `SessionIntent::Review` (read-only) otherwise,
    /// so a Researcher/Verifier helper cannot reach a write tool no matter
    /// what the lead's own prompt asked it to do.
    pub fn resolve_for_helper(can_write: bool, allowed_paths: Vec<String>) -> Self {
        let intent = if can_write {
            SessionIntent::Fix
        } else {
            SessionIntent::Review
        };
        Self {
            intent,
            mode: ExecutionMode::Auto,
            team: ExecutionTeam::Lead,
            provider: ExecutionProvider::Copilot,
            intent_policy: for_intent(intent),
            allowed_paths: Some(allowed_paths),
        }
    }

    /// Builds the policy for the LEAD'S OWN REVIEW turn (P1 "Finished is not
    /// a combined graph result"): a full-write `Fix`-shaped policy, exactly
    /// like [`Self::resolve`] would hand a solo Fix session, but explicitly
    /// tagged `ExecutionTeam::Lead` since this turn runs as part of a graph.
    /// `allowed_paths: None` (unrestricted, matching `resolve`) rather than
    /// `resolve_for_helper`'s `Some(vec![])` -- a helper's `Some(empty)`
    /// means "path-scoped with no allowance" (refuses every write); the
    /// lead's review turn is reviewing/finishing the WHOLE combined tree in
    /// its own dedicated integration worktree, not one path-restricted
    /// slice of it.
    pub fn resolve_for_lead_review() -> Self {
        let intent = SessionIntent::Fix;
        Self {
            intent,
            mode: ExecutionMode::Auto,
            team: ExecutionTeam::Lead,
            provider: ExecutionProvider::Copilot,
            intent_policy: for_intent(intent),
            allowed_paths: None,
        }
    }

    /// A read-only policy for checking over work someone else did.
    ///
    /// `SessionIntent::Review` rather than `Fix`: an auditor that decided to
    /// fix what it found would stop being a second opinion, and the value of
    /// the check is entirely that it did not touch the code. That intent
    /// carries `can_write: false`, so the tool is launched with writing denied
    /// -- the auditor cannot change its mind about this once it is running.
    ///
    /// Keeps the audited run's own provider so the check happens on a tool the
    /// user actually has, rather than defaulting to one that may not be
    /// installed.
    pub fn resolve_for_audit(of: &ExecutionPolicy) -> Self {
        let intent = SessionIntent::Review;
        Self {
            intent,
            mode: ExecutionMode::Ask,
            team: ExecutionTeam::Solo,
            provider: of.provider,
            intent_policy: for_intent(intent),
            allowed_paths: None,
        }
    }

    /// The agent-registry id of the tool this execution runs on.
    ///
    /// Convenience over `self.provider.agent_id()`, so provider selection
    /// (`ai::agent::select::choose`) reads the policy and nothing else --
    /// there is no second place that decides which tool an execution uses.
    pub fn agent_id(&self) -> &'static str {
        self.provider.agent_id()
    }

    /// Whether a tool call requesting `capability` may run right now.
    /// `started` is the session's own "has Start been pressed" flag (only
    /// changes the answer for Plan, see [`check_tool_capability`]'s doc
    /// comment) -- kept as a parameter rather than a field on `Self` because
    /// it can flip mid-execution (Plan -> Start) while everything else this
    /// struct carries is fixed for the execution's lifetime.
    pub fn check_tool_capability(
        &self,
        started: bool,
        capability: ToolCapability,
    ) -> Result<(), ToolRefusal> {
        check_tool_capability(self.intent, started, capability)
    }

    /// R6.4's actual enforcement, not merely storage: on top of the ordinary
    /// intent gate above, a HELPER execution (`allowed_paths.is_some()`) that
    /// is about to edit/delete/move a specific file must have that path
    /// covered by one of its allowed globs. `path` is `None` when the tool
    /// call carried no location this build could read (see
    /// `cli_run::extract_edit_path`'s doc comment) -- treated as a refusal
    /// for a path-scoped helper, same fail-closed stance as
    /// `ToolCapability::from_acp_kind`'s unknown-kind case, since a write
    /// this code cannot locate cannot be proven to stay inside the
    /// allowance.
    pub fn check_path_allowance(&self, capability: ToolCapability, path: Option<&str>) -> Result<(), ToolRefusal> {
        let Some(allowed) = &self.allowed_paths else {
            // Not a path-scoped (helper) policy -- nothing further to check.
            return Ok(());
        };
        if !capability.is_write() {
            return Ok(());
        }
        let Some(path) = path else {
            return Err(ToolRefusal::PathNotAllowed {
                path: "(unknown path)".to_string(),
            });
        };
        if allowed.iter().any(|glob| path_matches_glob(path, glob)) {
            Ok(())
        } else {
            Err(ToolRefusal::PathNotAllowed {
                path: path.to_string(),
            })
        }
    }
}

/// A small, dependency-free glob match: `**` matches any number of path
/// segments (including zero), `*` matches within one segment, everything
/// else is literal. Repo-relative, forward-slash-normalized on both sides
/// before comparing, so a Windows-style path from the tool call still
/// compares correctly against a Unix-style glob from `ProposedHelperJob`.
fn path_matches_glob(path: &str, glob: &str) -> bool {
    let path = path.replace('\\', "/");
    let glob = glob.replace('\\', "/");
    if glob == "**" || glob == "*" {
        return true;
    }
    if let Some(prefix) = glob.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = glob.strip_suffix("**") {
        return path.starts_with(prefix);
    }
    if let Some(prefix) = glob.strip_suffix("/*") {
        return path.starts_with(&format!("{prefix}/")) && !path[prefix.len() + 1..].contains('/');
    }
    path == glob
}

/// A tool an execution might try to call, coarse enough to gate against
/// [`IntentPolicy`] without depending on the full tool-call schema.
///
/// `Read`/`Search` need no permission; every other variant is a write in the
/// sense [`IntentPolicy::can_write`] governs. Kept as an explicit enum
/// (rather than a `bool is_write` on some existing tool-call type) so this
/// module's test suite can enumerate "every tool kind a real execution can
/// invoke" and prove each one is gated, rather than trusting that whoever
/// adds a new tool kind remembers to mark it correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ToolCapability {
    /// Reading a file, listing a directory, reading git status/log/diff.
    Read,
    /// Searching the repository (grep/glob-equivalent).
    Search,
    /// Creating, editing, or deleting a file in the working tree.
    EditFile,
    /// Creating or removing a worktree.
    Worktree,
    /// `git commit`.
    Commit,
    /// `git push`.
    Push,
    /// Posting a comment or review to the host (GitHub/GitLab/etc.).
    PostToHost,
}

impl ToolCapability {
    /// Every capability an execution can ever request. Used only by this
    /// module's own tests to prove the refusal is exhaustive over the real
    /// enum, not over a hand-picked subset.
    #[cfg(test)]
    fn all() -> [ToolCapability; 7] {
        [
            ToolCapability::Read,
            ToolCapability::Search,
            ToolCapability::EditFile,
            ToolCapability::Worktree,
            ToolCapability::Commit,
            ToolCapability::Push,
            ToolCapability::PostToHost,
        ]
    }

    /// Whether this tool is a write in the sense [`IntentPolicy::can_write`]
    /// governs. `Read`/`Search` are the only capabilities every intent may
    /// use unconditionally.
    fn is_write(self) -> bool {
        !matches!(self, ToolCapability::Read | ToolCapability::Search)
    }

    /// Classifies an ACP `toolCall.kind` string (agentclientprotocol/agent-client-protocol:
    /// `read | edit | delete | move | search | execute | think | fetch |
    /// switch_mode | other`) into the coarse capability this policy table
    /// gates on.
    ///
    /// Task 1.7's adversarial tests need this to fail CLOSED: an unknown or
    /// missing `kind` is classified as [`ToolCapability::EditFile`] (a
    /// write), not [`ToolCapability::Read`] -- a provider that omits `kind`,
    /// or a future ACP kind this build has not seen yet, must be refused
    /// under a read-only intent rather than waved through because this
    /// function guessed "read." `execute` (arbitrary shell) is likewise
    /// treated as a write, because a command can change files: it is gated
    /// exactly as an edit is, so a run that may edit may also run commands
    /// (each one through the person's approval gate) and a read-only run
    /// can do neither. `cli_agent::denied_tools_for` denies `shell` at the
    /// CLI's own launch flags for the same read-only runs, but this
    /// classifier does not assume that denial is in effect -- it gates on
    /// what the tool call itself claims to be.
    pub fn from_acp_kind(kind: Option<&str>) -> ToolCapability {
        match kind {
            Some("read") | Some("search") | Some("think") | Some("fetch") => ToolCapability::Read,
            Some("delete") | Some("move") => ToolCapability::EditFile,
            Some("execute") => ToolCapability::EditFile,
            // "edit", "switch_mode", "other", anything unrecognised, or
            // missing entirely: treated as a write. `switch_mode` changes
            // what the agent is permitted to do next, which is not a
            // passive read.
            _ => ToolCapability::EditFile,
        }
    }
}

/// Why a tool call was refused. Carried back to the execution engine (and,
/// through it, surfaced to the user) rather than silently dropping the call
/// -- an agent that tried to write and got no error would look broken, not
/// safely refused.
// Not `Copy`: `PathNotAllowed` carries the offending path, so this owns a
// `String` now. Cloned explicitly at the few call sites that need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ToolRefusal {
    /// This intent can never call a write tool, regardless of mode/team.
    ReadOnlyIntent { intent: SessionIntent },
    /// This intent may write eventually, but not before the session's
    /// explicit Start action (Plan mode, before Start).
    NotStartedYet { intent: SessionIntent },
    /// R6.4: a helper execution tried to write outside its own
    /// `allowed_paths`. Distinct from `ReadOnlyIntent` -- this helper CAN
    /// write, just not here.
    PathNotAllowed { path: String },
}

/// THE enforcement point (task 4.5): decides whether `capability` may be
/// invoked by a session whose intent is `intent` and which has (or has not)
/// been explicitly started.
///
/// This is not a suggestion the execution engine can route around -- it is
/// the one function every tool-dispatch call site in the engine must call
/// before actually running a tool. `started` only matters for
/// [`WorktreePolicy::NotUntilStart`] (Plan): every other intent's answer is
/// the same whether or not `started` is true, which is what makes Review and
/// Summarize provably unable to reach a write tool no matter what state the
/// session is in -- see the `read_only_intents_can_never_write` test below.
pub fn check_tool_capability(
    intent: SessionIntent,
    started: bool,
    capability: ToolCapability,
) -> Result<(), ToolRefusal> {
    if !capability.is_write() {
        return Ok(());
    }

    let policy = for_intent(intent);

    // Plan is the one intent whose `can_write: false` means "not yet" rather
    // than "never" -- its worktree policy is checked FIRST so `started`
    // getting a chance to flip the answer, before falling back to the
    // ordinary can-write gate every other intent uses. This is the one
    // branch where the same intent value answers differently depending on
    // `started`; every other intent's answer is fixed regardless of it.
    if policy.worktree == WorktreePolicy::NotUntilStart {
        return if started {
            Ok(())
        } else {
            Err(ToolRefusal::NotStartedYet { intent })
        };
    }

    if !policy.can_write {
        return Err(ToolRefusal::ReadOnlyIntent { intent });
    }

    match policy.worktree {
        WorktreePolicy::Never => {
            // can_write is true but worktree is Never: not a real
            // combination in today's table (every writer intent is
            // `Always`), but handled explicitly rather than falling through,
            // so a future intent added with this combination does not
            // silently start writing to the user's own checkout.
            Err(ToolRefusal::ReadOnlyIntent { intent })
        }
        WorktreePolicy::Always => Ok(()),
        WorktreePolicy::NotUntilStart => unreachable!("handled above"),
    }
}

#[cfg(test)]
mod tests {
    // Placed first so it is read alongside the other authority tests.
    #[test]
    fn an_auditor_is_launched_unable_to_write() {
        // The whole value of the check is that it did not touch the code. An
        // auditor that could edit would stop being a second opinion, and this
        // is enforced at launch rather than by asking it nicely.
        use super::*;
        let audited = ExecutionPolicy::resolve_for_helper(true, vec![]);
        let auditor = ExecutionPolicy::resolve_for_audit(&audited);

        for started in [false, true] {
            assert!(
                auditor
                    .check_tool_capability(started, ToolCapability::EditFile)
                    .is_err(),
                "an auditor could write with started={started}"
            );
            assert!(
                auditor
                    .check_tool_capability(started, ToolCapability::Commit)
                    .is_err(),
                "an auditor could commit with started={started}"
            );
        }
    }

    #[test]
    fn an_auditor_runs_on_the_same_tool_as_the_work_it_checks() {
        // Defaulting to a fixed provider would try to audit on a tool the
        // user may not have installed, and the audit would simply never run.
        use super::*;
        for provider in [
            ExecutionProvider::Copilot,
            ExecutionProvider::Gemini,
            ExecutionProvider::Claude,
        ] {
            let mut audited = ExecutionPolicy::resolve_for_helper(true, vec![]);
            audited.provider = provider;
            assert_eq!(ExecutionPolicy::resolve_for_audit(&audited).provider, provider);
        }
    }

    use super::*;

    fn all_intents() -> [SessionIntent; 6] {
        [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Plan,
            SessionIntent::Fix,
            SessionIntent::Review,
            SessionIntent::Summarize,
        ]
    }

    /// architecture.md section 9's table, encoded as a test so a future edit
    /// to `for_intent` that drifts from the documented defaults fails loudly.
    #[test]
    fn matches_the_documented_defaults_table() {
        let cases: &[(SessionIntent, ExecutionMode, ExecutionTeam, bool, WorktreePolicy)] = &[
            (SessionIntent::Ask, ExecutionMode::Ask, ExecutionTeam::Solo, false, WorktreePolicy::Never),
            (SessionIntent::Explain, ExecutionMode::Ask, ExecutionTeam::Solo, false, WorktreePolicy::Never),
            (SessionIntent::Summarize, ExecutionMode::Ask, ExecutionTeam::Solo, false, WorktreePolicy::Never),
            (SessionIntent::Review, ExecutionMode::Ask, ExecutionTeam::Solo, false, WorktreePolicy::Never),
            (SessionIntent::Plan, ExecutionMode::Plan, ExecutionTeam::Lead, false, WorktreePolicy::NotUntilStart),
            (SessionIntent::Fix, ExecutionMode::Auto, ExecutionTeam::Lead, true, WorktreePolicy::Always),
        ];
        for (intent, mode, team, can_write, worktree) in cases.iter().copied() {
            let policy = for_intent(intent);
            assert_eq!(policy.default_mode, mode, "{intent:?} default mode");
            assert_eq!(policy.default_team, team, "{intent:?} default team");
            assert_eq!(policy.can_write, can_write, "{intent:?} can_write");
            assert_eq!(policy.worktree, worktree, "{intent:?} worktree policy");
        }
    }

    /// Every intent variant has a row -- this compiles only because
    /// `for_intent`'s `match` is exhaustive; the loop just proves the table
    /// does not panic for any of them.
    #[test]
    fn every_intent_has_a_policy_row() {
        for intent in all_intents() {
            let _ = for_intent(intent);
        }
    }

    /// THE core proof for task 4.5: Review and Summarize (and Ask/Explain)
    /// cannot reach a write or worktree tool, in ANY state (`started` true or
    /// false), for EVERY write capability the real enum defines. This is the
    /// test the parent task asked for verbatim.
    #[test]
    fn read_only_intents_can_never_reach_a_write_or_worktree_tool() {
        let read_only = [
            SessionIntent::Ask,
            SessionIntent::Explain,
            SessionIntent::Review,
            SessionIntent::Summarize,
        ];
        for intent in read_only {
            for started in [false, true] {
                for capability in ToolCapability::all() {
                    let result = check_tool_capability(intent, started, capability);
                    if matches!(capability, ToolCapability::Read | ToolCapability::Search) {
                        assert!(
                            result.is_ok(),
                            "{intent:?} must still be able to {capability:?} (started={started})"
                        );
                    } else {
                        assert!(
                            matches!(result, Err(ToolRefusal::ReadOnlyIntent { .. })),
                            "{intent:?} must be refused for {capability:?} (started={started}), got {result:?}"
                        );
                    }
                }
            }
        }
    }

    /// Specifically the two intents the parent task names by product
    /// behavior (Review, Summarize): every write AND the worktree capability
    /// itself are refused. This is the literal "cannot call edit or worktree
    /// tools" proof, kept as its own test so it reads as the direct answer
    /// to the requirement even though it is a subset of the broader test
    /// above.
    #[test]
    fn review_and_summarize_cannot_call_edit_or_worktree_tools() {
        for intent in [SessionIntent::Review, SessionIntent::Summarize] {
            for started in [false, true] {
                assert!(matches!(
                    check_tool_capability(intent, started, ToolCapability::EditFile),
                    Err(ToolRefusal::ReadOnlyIntent { .. })
                ));
                assert!(matches!(
                    check_tool_capability(intent, started, ToolCapability::Worktree),
                    Err(ToolRefusal::ReadOnlyIntent { .. })
                ));
                assert!(matches!(
                    check_tool_capability(intent, started, ToolCapability::Commit),
                    Err(ToolRefusal::ReadOnlyIntent { .. })
                ));
                assert!(matches!(
                    check_tool_capability(intent, started, ToolCapability::Push),
                    Err(ToolRefusal::ReadOnlyIntent { .. })
                ));
                assert!(matches!(
                    check_tool_capability(intent, started, ToolCapability::PostToHost),
                    Err(ToolRefusal::ReadOnlyIntent { .. })
                ));
            }
        }
    }

    /// Fix is the only intent that can ever write, and only once isolation
    /// is unconditional (`Always`, not gated on `started`).
    #[test]
    fn fix_can_write_unconditionally_once_isolated() {
        for capability in [
            ToolCapability::EditFile,
            ToolCapability::Worktree,
            ToolCapability::Commit,
        ] {
            assert!(check_tool_capability(SessionIntent::Fix, false, capability).is_ok());
            assert!(check_tool_capability(SessionIntent::Fix, true, capability).is_ok());
        }
    }

    /// Fix is never allowed to push or post to the host as part of kickoff
    /// (task 5.4: "Never push or post a host comment/review as part of
    /// kickoff"). This module's `can_write` is coarse (one bool for every
    /// write kind), so today Push/PostToHost pass the same gate EditFile
    /// does; the kickoff-time refusal for those two specifically is enforced
    /// by the caller (kickoff never calls them), not by this table. This
    /// test documents that boundary explicitly rather than leaving it
    /// implicit, so a future change to this table does not accidentally
    /// start treating push/post as automatically safe for Fix without
    /// someone noticing this comment.
    #[test]
    fn fix_write_gate_does_not_by_itself_authorize_push_or_host_posts_at_kickoff() {
        // This intentionally documents current scope rather than asserting a
        // refusal this module does not implement: kickoff-time push/host-post
        // prevention lives in `commands::agent_desk::start_execution_at`
        // (task 5.4), which never constructs a Push/PostToHost tool call
        // during kickoff regardless of what this table allows.
        assert!(check_tool_capability(SessionIntent::Fix, true, ToolCapability::Push).is_ok());
    }

    /// Plan cannot write before Start, but the SAME intent value can once
    /// `started` flips true -- the one case where `started` changes the
    /// answer.
    /// The distinction the default-tool setting rests on.
    ///
    /// An explicit per-chat override must fail loudly -- the person named that
    /// tool and has to be told it cannot be used. A DEFAULT naming a tool this
    /// build does not know must be ignored instead, or a setting left behind
    /// by an older build would block every chat in the app with an error
    /// nobody could connect to a settings page they last touched months ago.
    /// `start_execution_at` filters the default through this; the override
    /// goes straight to `resolve`.
    #[test]
    fn an_unknown_tool_name_is_recognisable_as_unknown() {
        assert!(ExecutionProvider::parse("copilot").is_some());
        assert!(ExecutionProvider::parse("Claude").is_some(), "matching ignores case");
        assert!(ExecutionProvider::parse("codex").is_some());
        assert!(ExecutionProvider::parse("a-tool-from-a-later-build").is_none());
        assert!(ExecutionProvider::parse("").is_none());
    }

    #[test]
    fn an_explicit_override_of_an_unknown_tool_still_refuses() {
        // The loud half: this is what a person picking a tool by name gets.
        let refusal = ExecutionPolicy::resolve(
            SessionIntent::Fix,
            ExecutionMode::Auto,
            ExecutionTeam::Solo,
            Some("a-tool-from-a-later-build"),
        );
        assert!(matches!(
            refusal,
            Err(PolicyRefusal::UnsupportedProvider { .. })
        ));
    }

    #[test]
    fn plan_cannot_write_before_start_but_can_after() {
        assert!(matches!(
            check_tool_capability(SessionIntent::Plan, false, ToolCapability::EditFile),
            Err(ToolRefusal::NotStartedYet { .. })
        ));
        assert!(check_tool_capability(SessionIntent::Plan, true, ToolCapability::EditFile).is_ok());
    }

    /// Read and Search are never gated, for any intent, in any state --
    /// otherwise Review/Summarize could not do their actual job.
    #[test]
    fn read_and_search_are_always_allowed() {
        for intent in all_intents() {
            for started in [false, true] {
                assert!(check_tool_capability(intent, started, ToolCapability::Read).is_ok());
                assert!(check_tool_capability(intent, started, ToolCapability::Search).is_ok());
            }
        }
    }

    /// Every (intent, started, capability) combination round-trips through
    /// serde -- this table is exported to the frontend (`specta::Type`), so a
    /// bad rename here would silently desync the UI's own copy of "can this
    /// write".
    #[test]
    fn intent_policy_serializes_camel_case() {
        let policy = for_intent(SessionIntent::Fix);
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("\"defaultMode\""), "got: {json}");
        assert!(json.contains("\"defaultTeam\""), "got: {json}");
        assert!(json.contains("\"canWrite\""), "got: {json}");
    }

    // -- ExecutionPolicy::resolve / provider override (task 1.1, 1.6) --

    #[test]
    fn no_override_resolves_to_the_default_copilot_provider() {
        let policy =
            ExecutionPolicy::resolve(SessionIntent::Fix, ExecutionMode::Auto, ExecutionTeam::Lead, None)
                .expect("no override must always resolve");
        assert_eq!(policy.provider, ExecutionProvider::Copilot);
    }

    #[test]
    fn an_override_naming_the_supported_provider_resolves() {
        for name in ["copilot", "Copilot", "COPILOT"] {
            let policy = ExecutionPolicy::resolve(
                SessionIntent::Fix,
                ExecutionMode::Auto,
                ExecutionTeam::Lead,
                Some(name),
            )
            .unwrap_or_else(|e| panic!("{name:?} must resolve, got {e:?}"));
            assert_eq!(policy.provider, ExecutionProvider::Copilot);
        }
    }

    /// Every tool the agent registry knows is nameable as an override, and
    /// each resolves to its own provider rather than quietly to the default.
    #[test]
    fn every_supported_tool_can_be_named_as_an_override() {
        let cases = [
            ("copilot", ExecutionProvider::Copilot),
            ("gemini", ExecutionProvider::Gemini),
            ("Claude", ExecutionProvider::Claude),
            ("OPENCODE", ExecutionProvider::OpenCode),
        ];
        for (name, expected) in cases {
            let policy = ExecutionPolicy::resolve(
                SessionIntent::Fix,
                ExecutionMode::Auto,
                ExecutionTeam::Lead,
                Some(name),
            )
            .unwrap_or_else(|e| panic!("{name:?} must resolve, got {e:?}"));
            assert_eq!(policy.provider, expected, "{name:?}");
        }
    }

    /// The two vocabularies -- this enum and the agent registry's ids -- must
    /// agree. A provider whose id is not in the registry would resolve here
    /// and then fail to launch with a confusing message instead of being
    /// refused up front.
    #[test]
    fn every_provider_id_names_a_tool_the_registry_knows() {
        for provider in [
            ExecutionProvider::Copilot,
            ExecutionProvider::Gemini,
            ExecutionProvider::Claude,
            ExecutionProvider::OpenCode,
            ExecutionProvider::Codex,
        ] {
            let id = provider.agent_id();
            assert!(
                crate::ai::agent::registry::find(id).is_some(),
                "{provider:?} names {id:?}, which the registry does not have"
            );
        }
    }

    /// Task 1.6's core proof: an override naming something this build does
    /// not support must be a typed refusal, never a silent fall-through to
    /// Copilot.
    #[test]
    fn an_unsupported_override_is_refused_visibly_not_silently_downgraded() {
        for name in ["gpt-4", "cop1lot", "", "claude-code", "chatgpt"] {
            let outcome = ExecutionPolicy::resolve(
                SessionIntent::Fix,
                ExecutionMode::Auto,
                ExecutionTeam::Lead,
                Some(name),
            );
            assert!(
                matches!(
                    outcome,
                    Err(PolicyRefusal::UnsupportedProvider { ref requested }) if requested == name
                ),
                "{name:?} must be refused, got {outcome:?}"
            );
        }
    }

    /// A read-only intent stays read-only through `ExecutionPolicy` no
    /// matter what `mode`/`team` a caller passes -- those two never widen
    /// what the intent allows (see `resolve`'s doc comment).
    #[test]
    fn execution_policy_never_lets_mode_or_team_widen_a_read_only_intent() {
        for mode in [ExecutionMode::Ask, ExecutionMode::Plan, ExecutionMode::Auto] {
            for team in [ExecutionTeam::Solo, ExecutionTeam::Lead] {
                let policy =
                    ExecutionPolicy::resolve(SessionIntent::Review, mode, team, None).unwrap();
                assert!(matches!(
                    policy.check_tool_capability(true, ToolCapability::EditFile),
                    Err(ToolRefusal::ReadOnlyIntent { .. })
                ));
                assert!(matches!(
                    policy.check_tool_capability(false, ToolCapability::EditFile),
                    Err(ToolRefusal::ReadOnlyIntent { .. })
                ));
            }
        }
    }

    #[test]
    fn execution_policy_delegates_plan_started_gating() {
        let policy =
            ExecutionPolicy::resolve(SessionIntent::Plan, ExecutionMode::Plan, ExecutionTeam::Lead, None)
                .unwrap();
        assert!(matches!(
            policy.check_tool_capability(false, ToolCapability::EditFile),
            Err(ToolRefusal::NotStartedYet { .. })
        ));
        assert!(policy.check_tool_capability(true, ToolCapability::EditFile).is_ok());
    }

    // -- ToolCapability::from_acp_kind (task 1.7: fail closed on unknown ACP kinds) --

    #[test]
    fn known_read_only_acp_kinds_classify_as_read() {
        for kind in ["read", "search", "think", "fetch"] {
            assert_eq!(ToolCapability::from_acp_kind(Some(kind)), ToolCapability::Read);
        }
    }

    #[test]
    fn known_write_acp_kinds_classify_as_edit_file() {
        for kind in ["edit", "delete", "move", "execute", "switch_mode", "other"] {
            assert_eq!(
                ToolCapability::from_acp_kind(Some(kind)),
                ToolCapability::EditFile,
                "{kind} must be treated as a write"
            );
        }
    }

    /// The exact adversarial case task 1.7 asks for: a tool call with no
    /// `kind` at all, or a `kind` this build has never seen, must fail
    /// closed as a write -- never fall back to "read" and slip past a
    /// read-only intent's refusal.
    #[test]
    fn missing_or_unknown_acp_kind_fails_closed_as_a_write() {
        assert_eq!(ToolCapability::from_acp_kind(None), ToolCapability::EditFile);
        assert_eq!(
            ToolCapability::from_acp_kind(Some("some_future_kind_this_build_has_never_seen")),
            ToolCapability::EditFile
        );
    }

    /// A shell request (`execute`) gets the same answer as a file edit for
    /// every intent, before and after Start. Fix and a started Plan may run
    /// commands (through the person's gate) so the agent can test and build
    /// its own edits; every read-only intent and Plan before Start cannot,
    /// because a command can change files and would be a way around the
    /// write refusal.
    #[test]
    fn a_shell_request_is_gated_exactly_like_a_file_edit() {
        let shell = ToolCapability::from_acp_kind(Some("execute"));
        for intent in all_intents() {
            let policy = ExecutionPolicy::resolve(intent, ExecutionMode::Auto, ExecutionTeam::Lead, None)
                .expect("no provider override, cannot fail");
            for started in [false, true] {
                assert_eq!(
                    policy.check_tool_capability(started, shell),
                    policy.check_tool_capability(started, ToolCapability::EditFile),
                    "{intent:?} (started={started}): shell and edit must be gated identically"
                );
            }
        }
    }

    // -- ExecutionPolicy::resolve_for_helper / check_path_allowance (R6.4) --

    #[test]
    fn a_helper_writing_inside_its_allowed_paths_is_permitted() {
        let policy = ExecutionPolicy::resolve_for_helper(true, vec!["src/**".into()]);
        assert!(policy
            .check_path_allowance(ToolCapability::EditFile, Some("src/lib.rs"))
            .is_ok());
        assert!(policy
            .check_path_allowance(ToolCapability::EditFile, Some("src/mod/inner.rs"))
            .is_ok());
    }

    #[test]
    fn a_helper_writing_outside_its_allowed_paths_is_refused() {
        let policy = ExecutionPolicy::resolve_for_helper(true, vec!["src/**".into()]);
        let result = policy.check_path_allowance(ToolCapability::EditFile, Some("Cargo.toml"));
        assert!(matches!(result, Err(ToolRefusal::PathNotAllowed { ref path }) if path == "Cargo.toml"));
    }

    /// A write whose path this code could not read from the tool call at all
    /// must fail closed for a path-scoped helper -- an unlocatable write
    /// cannot be proven to stay inside the allowance.
    #[test]
    fn a_helper_write_with_no_readable_path_fails_closed() {
        let policy = ExecutionPolicy::resolve_for_helper(true, vec!["src/**".into()]);
        assert!(matches!(
            policy.check_path_allowance(ToolCapability::EditFile, None),
            Err(ToolRefusal::PathNotAllowed { .. })
        ));
    }

    /// A read-only helper (Researcher/Verifier, empty `allowed_paths`) still
    /// refuses at the ordinary `check_tool_capability` gate -- proven here so
    /// the two enforcement layers (intent-level and path-level) are shown to
    /// compose rather than one silently substituting for the other.
    #[test]
    fn a_read_only_helper_is_refused_at_the_capability_gate_before_path_checking_matters() {
        let policy = ExecutionPolicy::resolve_for_helper(false, vec![]);
        assert!(matches!(
            policy.check_tool_capability(true, ToolCapability::EditFile),
            Err(ToolRefusal::ReadOnlyIntent { .. })
        ));
    }

    /// A solo/lead policy (`allowed_paths: None`) is never path-scoped --
    /// `check_path_allowance` must be a no-op for it regardless of path.
    #[test]
    fn a_non_helper_policy_is_never_path_scoped() {
        let policy = ExecutionPolicy::resolve(SessionIntent::Fix, ExecutionMode::Auto, ExecutionTeam::Lead, None)
            .unwrap();
        assert!(policy
            .check_path_allowance(ToolCapability::EditFile, Some("anything/at/all.rs"))
            .is_ok());
    }

    #[test]
    fn glob_matching_supports_double_star_single_star_and_literal_paths() {
        assert!(path_matches_glob("src/lib.rs", "src/**"));
        assert!(path_matches_glob("src/a/b/c.rs", "src/**"));
        assert!(path_matches_glob("src/lib.rs", "src/*"));
        assert!(!path_matches_glob("src/a/b.rs", "src/*"));
        assert!(path_matches_glob("Cargo.toml", "Cargo.toml"));
        assert!(!path_matches_glob("Cargo.lock", "Cargo.toml"));
        assert!(path_matches_glob("anything/here.rs", "**"));
    }
}
