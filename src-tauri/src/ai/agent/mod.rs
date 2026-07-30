//! The agent engine: a bounded plan/act/observe loop over a provider.
//!
//! Separate from [`crate::ai::client`], which does one-shot completions for
//! commit messages. An agent run is multi-turn and tool-using, so it needs a
//! transport that can carry tool calls and results back and forth -- a shape
//! the single-shot path deliberately does not have.

pub mod acp;
pub mod api_agent;
pub mod cli_agent;
pub mod copilot_cli;
pub mod events;
pub mod exec;
pub mod guardrails;
pub mod run;
pub mod select;
pub mod session;
pub mod tools;
pub mod transport;

// Re-exported so callers outside this module use `ai::agent::X` rather than
// reaching through `transport`. Allowed to be unused while the engine has no
// caller yet; the run console is what will consume these.
#[allow(unused_imports)]
pub use transport::{
  AgentError, AgentTurn, Message, ProviderAgent, ToolCall, ToolResult, ToolSchema, Transport,
  TurnRequest,
};

/// Turns a run may take before it ends as "didn't finish".
///
/// Counted in turns rather than minutes so a task behaves the same way on a
/// slow provider as a fast one: a five-minute ceiling would give one provider
/// fifteen real steps and another four, making "didn't finish" mean different
/// things depending on who the user signed in with.
///
/// Twelve is a starting value, not a finding. The spike measured 10-20 seconds
/// per turn, putting a worst case around three to four minutes -- long enough
/// to be worth starting, short enough that a run gone wrong is not left
/// grinding. The setting exists because only real use reveals the right number.
pub const DEFAULT_TURN_BUDGET: u32 = 12;
