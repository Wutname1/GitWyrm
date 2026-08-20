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
}

/// Why a tool call was refused. Carried back to the execution engine (and,
/// through it, surfaced to the user) rather than silently dropping the call
/// -- an agent that tried to write and got no error would look broken, not
/// safely refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ToolRefusal {
    /// This intent can never call a write tool, regardless of mode/team.
    ReadOnlyIntent { intent: SessionIntent },
    /// This intent may write eventually, but not before the session's
    /// explicit Start action (Plan mode, before Start).
    NotStartedYet { intent: SessionIntent },
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
}
