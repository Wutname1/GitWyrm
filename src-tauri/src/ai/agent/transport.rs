//! How a provider is named, and why a turn failed.
//!
//! The turn-shuttling types that used to live here are gone with the loop:
//! the provider CLI carries its own conversation, so nothing in GitWyrm
//! assembles turns or tool calls any more. What remains is the vocabulary the
//! console needs to explain a failure.
//!
//! `Transport` had `ApiKey` and `OpenAiCompatible` variants for transports
//! that were planned but never built past the CLI subprocess path; they were
//! removed as dead code (2026-08-19) since nothing produced or consumed them
//! outside this module's own tests. Add a variant back only alongside the
//! driver that actually implements it.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Which way a provider is reached.
///
/// Still one variant, deliberately, even now that GitWyrm drives four
/// different tools. They are all the same transport: a command-line tool
/// spawned as a subprocess, spoken to over ACP on its stdin and stdout. What
/// differs between them -- the binary's name, the arguments that start its ACP
/// server, how it can be told to refuse a tool -- is data, and lives in
/// [`super::registry`] where a new tool is a new row rather than a new variant
/// here.
///
/// Adding a variant would mean a genuinely different way of reaching a
/// provider (an HTTP API, say), and the rule this module already stated stands:
/// add one only alongside the driver that implements it. Two earlier variants,
/// `ApiKey` and `OpenAiCompatible`, were removed in 2026-08 precisely because
/// they never had one.
///
/// Which tool a run uses is carried by the execution's policy
/// (`agentdesk::policy::ExecutionProvider`), not by this enum, so the two do
/// not have to be kept in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// The user's own installed command-line tool, driven as a subprocess.
    /// Exists so a subscription-only user is not shut out. GitWyrm never reads
    /// the tool's credential files -- it asks the tool and believes its answer.
    Cli,
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Transport::Cli => "command-line tool",
        };
        f.write_str(s)
    }
}

/// Why a turn could not be taken.
///
/// Separate from [`AppError`] so the console can say something useful about
/// each: a missing CLI and a refused key need different sentences, and
/// flattening them to one string would lose that.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentError {
    /// The transport itself is not available -- no CLI on PATH, or a version
    /// below the floor. Names what is missing and what to do.
    TransportUnavailable {
        transport: Transport,
        detail: String,
    },
    /// Credentials exist but the provider will not accept them. This is the
    /// stale-token and wrong-scope case: it needs a reconnect, not a retry.
    NeedsReconnect { detail: String },
    /// The provider was reached and refused the request (rate limit, quota, a
    /// model the plan does not include).
    Refused { detail: String },
    /// The user stopped the run.
    Cancelled,
    /// Anything else, including transport-level I/O failures.
    Failed { detail: String },
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::TransportUnavailable { transport, detail } => {
                write!(f, "{transport} is not available: {detail}")
            }
            AgentError::NeedsReconnect { detail } => write!(f, "sign in again: {detail}"),
            AgentError::Refused { detail } => write!(f, "the provider refused: {detail}"),
            AgentError::Cancelled => f.write_str("stopped"),
            AgentError::Failed { detail } => f.write_str(detail),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<AppError> for AgentError {
    fn from(e: AppError) -> Self {
        AgentError::Failed {
            detail: e.to_string(),
        }
    }
}
