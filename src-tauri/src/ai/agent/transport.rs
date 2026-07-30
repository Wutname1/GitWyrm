//! One interface, several transports.
//!
//! The engine asks the default provider what it supports and uses that. It has
//! no preference of its own: a user who signed in with Copilot gets the Copilot
//! path, a user with an API key gets the API path, and neither is treated as
//! the "real" one.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Which way a provider is reached.
///
/// Not a ranking. Which variant applies is decided by what the user's default
/// provider actually offers, in [`super::select`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
  /// A documented HTTP API with the user's own key. Preferred where a key
  /// exists: a published API is a stable contract, where a CLI's output format
  /// is not.
  ApiKey,
  /// The user's own installed CLI, driven as a subprocess. Exists so a
  /// subscription-only user is not shut out. GitWyrm never reads the CLI's
  /// credential files -- it asks the tool and believes its answer.
  Cli,
  /// Any endpoint speaking the OpenAI dialect: a local opencode server,
  /// Ollama, LM Studio, or a self-hosted gateway.
  OpenAiCompatible,
}

impl fmt::Display for Transport {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      Transport::ApiKey => "API key",
      Transport::Cli => "command-line tool",
      Transport::OpenAiCompatible => "OpenAI-compatible endpoint",
    };
    f.write_str(s)
  }
}

/// A tool the model asked to use.
///
/// The bounded set lives in the loop, not here: a transport's job is to carry
/// the request faithfully, and refusing an unknown tool is the engine's call so
/// the rule holds identically across all three transports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
  /// Transport-assigned id, echoed back so a result can be matched to its call.
  pub id: String,
  pub name: String,
  /// Arguments as the model produced them. Parsed and validated by the tool
  /// itself, which is where a bad shape should be reported from.
  pub arguments: serde_json::Value,
}

/// What a tool produced, fed back into the next turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
  /// The [`ToolCall::id`] this answers.
  pub id: String,
  pub content: String,
  /// True when the tool refused or failed. The model is told either way -- a
  /// refusal it cannot see is a refusal it will retry.
  pub is_error: bool,
}

/// One request to the provider: the conversation so far, plus what tools exist.
pub struct TurnRequest<'a> {
  pub model: &'a str,
  pub system: &'a str,
  /// Full conversation. Transports that are stateless replay it; a CLI holding
  /// its own session may send only what is new.
  pub messages: &'a [Message],
  /// Tool schemas the model may call this turn.
  pub tools: &'a [ToolSchema],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
  User { content: String },
  Assistant { content: String, tool_calls: Vec<ToolCall> },
  Tool { result: ToolResult },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
  pub name: String,
  pub description: String,
  /// JSON Schema for the arguments.
  pub parameters: serde_json::Value,
}

/// What one turn produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
  /// Prose the model produced, if any. May be empty on a pure tool-call turn.
  pub text: String,
  /// Tools it wants run before the next turn. Empty means it is done talking.
  pub tool_calls: Vec<ToolCall>,
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
  TransportUnavailable { transport: Transport, detail: String },
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
    AgentError::Failed { detail: e.to_string() }
  }
}

/// A provider the engine can run turns against.
///
/// Implemented once per transport. Nothing provider-specific reaches the loop
/// or the console: a Copilot run and an Anthropic run differ only in which
/// implementation is behind this.
///
/// Uses native `async fn` in traits rather than `async_trait`, which keeps the
/// dependency list as it is. The cost is that the trait is not object-safe, so
/// the engine dispatches over an enum of implementations rather than a
/// `dyn ProviderAgent` -- see [`super::select`].
pub trait ProviderAgent: Send + Sync {
  /// Which transport this is, for the console to name when something is wrong.
  fn transport(&self) -> Transport;

  /// Whether this transport can currently run, and why not when it cannot.
  ///
  /// Asks the underlying tool or endpoint rather than checking for a
  /// credential file: a complete-looking credential whose scope is wrong looks
  /// ready on disk and fails at generation time, which is the spike's own
  /// failure case.
  fn check(&self) -> impl std::future::Future<Output = Result<(), AgentError>> + Send;

  /// Take one turn.
  ///
  /// Must remain cancellable throughout -- a turn is 10-20 seconds, so a Stop
  /// that waits for it to finish is a Stop that looks broken.
  fn turn(
    &self,
    req: TurnRequest<'_>,
  ) -> impl std::future::Future<Output = Result<AgentTurn, AgentError>> + Send;
}
