//! What every agent tool tells GitWyrm, in GitWyrm's own words.
//!
//! # Why this module exists
//!
//! The run loop used to read another protocol's JSON directly: it pulled
//! `toolCall.kind` to decide whether something was a write, dug
//! `toolCall.locations[0].path` out to check it against a helper's allowance,
//! and matched on ACP's `allow_once` / `reject_once` option names to answer.
//!
//! That put the **safety gate** -- the code that decides whether an agent may
//! touch a file -- in the business of parsing one vendor's wire format. Adding
//! a second tool would have meant either teaching the gate a second dialect or
//! quietly having two gates. Neither is acceptable for the one piece of code
//! that must never be wrong.
//!
//! So each adapter now translates its own protocol into the types here, and
//! the run loop reads only these. A new tool is a new translator, and the gate
//! never changes.

use tokio::sync::oneshot;

use crate::agentdesk::policy::ToolCapability;

/// Something an agent told us, on its own initiative.
///
/// Deliberately small. Anything an adapter knows that is not in here is
/// something the run loop has decided it does not need -- if a variant grows a
/// `serde_json::Value`, that is the boundary leaking again.
#[derive(Debug)]
pub enum Incoming {
    /// A piece of the agent's prose, as it is produced.
    TextChunk(String),
    /// A tool the agent is running, described in one line.
    ///
    /// Only the title survives translation: the run loop shows it and counts
    /// it, and has never needed anything else.
    ToolCall { title: String },
    /// How full the session's context window is, and what it has cost.
    ///
    /// `used` is occupancy rather than spend and falls when the agent compacts
    /// its history, so it replaces the previous figure rather than adding to
    /// it. `cost_micro_usd` is cumulative for the session when a tool reports
    /// it at all, which most do not.
    ContextUsage {
        used: u32,
        size: u32,
        cost_micro_usd: Option<u32>,
    },
    /// The agent is asking whether it may do something.
    ///
    /// `respond` MUST be answered: the agent's turn is blocked until it is,
    /// and dropping the sender without a reply hangs the run.
    PermissionRequest(PermissionRequest),
}

/// One thing an agent wants permission to do, already classified.
///
/// The classification happens in the adapter, not here, because only the
/// adapter knows what its protocol calls a write. What reaches the gate is the
/// answer, not the evidence.
#[derive(Debug)]
pub struct PermissionRequest {
    /// What kind of thing this is, in GitWyrm's vocabulary.
    ///
    /// An adapter that cannot tell must say [`ToolCapability::EditFile`]: an
    /// unrecognised action is treated as a write, so a tool GitWyrm does not
    /// understand cannot slip a change past a read-only run.
    pub capability: ToolCapability,
    /// The file this would touch, when the tool said. `None` when it did not,
    /// which a path-scoped helper treats as a refusal rather than a pass.
    pub path: Option<String>,
    /// One line describing what is being asked, for the approval card.
    pub summary: String,
    /// How to answer. The adapter turns this back into whatever its protocol
    /// expects.
    pub respond: oneshot::Sender<PermissionDecision>,
}

/// Our answer to a permission request.
///
/// There is deliberately no "allow always". Protocols offer one; a remembered
/// approval is a decision made once and then applied to situations the person
/// never saw, which is exactly what the gate exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    AllowOnce,
    RejectOnce,
    /// The run is stopping. Distinct from a refusal: nothing is being judged.
    Cancelled,
}

/// Why a turn ended.
///
/// Named for what it means rather than for any one protocol's spelling, so an
/// adapter maps its own vocabulary in and the run loop reads one set of words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopReason {
    /// The agent finished what it was doing.
    EndTurn,
    /// It ran out of room in its reply.
    MaxTokens,
    /// It hit its own limit on how many steps it would take.
    MaxTurnRequests,
    /// It declined to continue.
    Refusal,
    /// We stopped it.
    Cancelled,
    /// It stopped for a reason this build does not recognise.
    #[default]
    Unknown,
}

impl StopReason {
    /// Whether this is a turn that did its work, as opposed to one cut short.
    pub fn is_success(self) -> bool {
        matches!(self, StopReason::EndTurn)
    }

    /// Plain-language cause, for the console to show when a run did not finish.
    pub fn plain_reason(self) -> &'static str {
        match self {
            StopReason::EndTurn => "finished",
            StopReason::MaxTokens => "the reply grew too long to continue",
            StopReason::MaxTurnRequests => "the AI reached its own step limit",
            StopReason::Refusal => "the AI declined to continue",
            StopReason::Cancelled => "you stopped it",
            StopReason::Unknown => "it stopped for a reason GitWyrm did not recognise",
        }
    }
}

/// What one turn produced: why it stopped, and what it cost.
#[derive(Debug, Clone, Default)]
pub struct TurnOutcome {
    pub stop_reason: StopReason,
    /// `None` when the tool reported no usage. Distinct from zero: most tools
    /// report nothing at all, and inventing a zero would make an unmeasured
    /// run look free.
    pub usage: Option<crate::agentdesk::model::TurnUsage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_finishing_counts_as_success() {
        assert!(StopReason::EndTurn.is_success());
        for other in [
            StopReason::MaxTokens,
            StopReason::MaxTurnRequests,
            StopReason::Refusal,
            StopReason::Cancelled,
            StopReason::Unknown,
        ] {
            assert!(!other.is_success(), "{other:?} was treated as a finished turn");
        }
    }

    #[test]
    fn every_stop_reason_explains_itself_without_blaming_gitwyrm() {
        for reason in [
            StopReason::EndTurn,
            StopReason::MaxTokens,
            StopReason::MaxTurnRequests,
            StopReason::Refusal,
            StopReason::Cancelled,
            StopReason::Unknown,
        ] {
            let text = reason.plain_reason();
            assert!(!text.is_empty());
            for blame in ["error", "failed", "crash"] {
                assert!(!text.contains(blame), "{text:?} reads as a GitWyrm fault");
            }
        }
    }

    #[test]
    fn an_unrecognised_stop_reason_is_the_default() {
        // A protocol that adds a value must not be read as a clean finish.
        assert_eq!(StopReason::default(), StopReason::Unknown);
        assert!(!StopReason::default().is_success());
    }

    #[test]
    fn there_is_no_remembered_approval() {
        // Guards the design, not the code: a variant meaning "and every time
        // after this" would apply one decision to situations nobody saw.
        let all = [
            PermissionDecision::AllowOnce,
            PermissionDecision::RejectOnce,
            PermissionDecision::Cancelled,
        ];
        assert_eq!(all.len(), 3);
    }
}
