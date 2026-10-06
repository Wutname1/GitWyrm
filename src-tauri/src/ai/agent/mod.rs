//! Reaching a provider's agent: finding its CLI, and speaking ACP to it.
//!
//! Separate from [`crate::ai::client`], which does one-shot completions for
//! commit messages. An agent run is multi-turn and tool-using, which the
//! single-shot path deliberately cannot carry.
//!
//! GitWyrm does not run the loop itself. The provider's CLI plans, acts and
//! observes on its own, so a run hands it the whole task and watches -- see
//! [`crate::airun::cli_run`]. What lives here is the transport: discovering the
//! CLI, spawning it with the tools it may not use, and naming what went wrong.

pub mod acp;
pub mod claude;
pub mod cli_agent;
pub mod codex;
pub mod codex_models;
pub mod copilot_cli;
pub mod registry;
pub mod run;
pub mod select;
pub mod shell_path;
pub mod slash_commands;
pub mod tool_memory;
pub mod tool_updates;
pub mod transport;
pub mod wire;
