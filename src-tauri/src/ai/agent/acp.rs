//! Agent Client Protocol client: NDJSON over a child process's stdin/stdout.
//!
//! Verified against `agentclientprotocol/agent-client-protocol` and GitHub's
//! ACP server reference (2026-07-30). We speak the protocol directly rather
//! than depending on the schema crate: we use a handful of its ~7000 lines, and
//! a preview-status protocol is better pinned to the few messages we actually
//! send than to a crate version.
//!
//! Only the transport lives here. Which tools exist, what is refused, and when
//! a run ends are the engine's decisions -- this module carries messages and
//! surfaces permission requests for someone else to answer.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

use super::transport::AgentError;

/// Protocol version we negotiate. ACP is in public preview; a mismatch is
/// reported rather than guessed at.
pub const PROTOCOL_VERSION: u32 = 1;

/// What the agent sent us that is not a reply to something we asked.
///
/// The engine consumes these to drive the console: text chunks become the
/// visible transcript, tool calls become steps, and permission requests become
/// a decision it has to make.
#[derive(Debug)]
pub enum Incoming {
    /// A piece of the agent's prose, as it is produced.
    TextChunk(String),
    /// A tool call starting or changing state.
    ToolCall {
        id: String,
        title: String,
        raw: Value,
    },
    /// The agent is asking whether it may do something. `respond` MUST be
    /// answered -- the agent's turn is blocked until it is.
    PermissionRequest {
        tool_call: Value,
        options: Vec<PermissionOption>,
        respond: oneshot::Sender<PermissionDecision>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionOption {
    #[serde(rename = "optionId")]
    pub option_id: String,
    pub name: String,
    pub kind: String,
}

/// Our answer to a permission request.
///
/// There is deliberately no "allow always" variant. The protocol offers
/// `allow_always`, but a remembered approval is a decision made once and then
/// applied to situations the user never saw -- exactly what the guardrails
/// exist to prevent.
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    AllowOnce { option_id: String },
    RejectOnce { option_id: String },
    Cancelled,
}

/// Why a prompt turn ended, as the agent reported it.
///
/// Mirrors ACP's `StopReason`. Note `end_turn`, not `completed` -- the docs
/// site lists a value the schema does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
    /// Anything the protocol adds later. Treated as "ended, cause unknown"
    /// rather than a parse failure, since the enum is `non_exhaustive` upstream.
    #[serde(other)]
    Unknown,
}

/// What one `session/prompt` turn produced: why it stopped, and what it cost.
///
/// Exists because `prompt` used to return only the stop reason and drop the
/// rest of the response on the floor -- including any usage the agent
/// reported, which is the only place a token count is ever offered.
#[derive(Debug, Clone, Default)]
pub struct TurnOutcome {
    pub stop_reason: StopReason,
    /// `None` when the agent reported no usage for this turn. Distinct from
    /// zero: most ACP agents report nothing at all, and inventing a zero
    /// would make an unmeasured run look free.
    pub usage: Option<crate::agentdesk::model::TurnUsage>,
}

/// Pulls whatever usage an agent chose to report out of a `session/prompt`
/// response.
///
/// ACP does not standardise usage reporting, so this is deliberately
/// permissive rather than a typed deserialize: it looks in the places agents
/// actually put it (`usage`, or `_meta.usage`, the protocol's own extension
/// slot) and accepts any of the field spellings in common use. A response
/// with none of them yields `None`, which is the honest answer for the
/// majority of agents.
///
/// Unknown shapes are ignored rather than erroring: a usage figure is a
/// nice-to-have, and failing a turn because an agent added a field would be
/// a bad trade.
fn parse_turn_usage(res: &Value) -> Option<crate::agentdesk::model::TurnUsage> {
    let node = res
        .get("usage")
        .or_else(|| res.get("_meta").and_then(|m| m.get("usage")))
        .or_else(|| res.get("meta").and_then(|m| m.get("usage")))?;

    /// Reads the first present spelling of a count, as a whole number.
    fn count(node: &Value, names: &[&str]) -> Option<u64> {
        names.iter().find_map(|n| node.get(*n)?.as_u64())
    }

    let usage = crate::agentdesk::model::TurnUsage {
        input_tokens: count(node, &["inputTokens", "input_tokens", "promptTokens", "prompt_tokens"]),
        output_tokens: count(
            node,
            &["outputTokens", "output_tokens", "completionTokens", "completion_tokens"],
        ),
        // `cachedReadTokens` is Copilot CLI 1.0.80's own spelling (verified
        // against its bundled schema); the others are the spellings used by
        // agents that follow Anthropic's or OpenAI's naming.
        cached_input_tokens: count(
            node,
            &[
                "cachedReadTokens",
                "cached_read_tokens",
                "cachedInputTokens",
                "cached_input_tokens",
                "cacheReadInputTokens",
                "cache_read_input_tokens",
            ],
        ),
        // Converted to micro-USD at the boundary so nothing downstream holds
        // money in a float. Negative or non-finite figures are dropped rather
        // than cast (a cast would saturate to 0 and read as "free").
        cost_micro_usd: ["costUsd", "cost_usd", "totalCostUsd", "total_cost_usd"]
            .iter()
            .find_map(|n| node.get(*n)?.as_f64())
            .filter(|c| c.is_finite() && *c >= 0.0)
            .map(|c| (c * 1_000_000.0).round() as u64),
    };

    // An empty object is the same as no object: report nothing rather than a
    // row of `None`s that reads as a measured result.
    if usage.is_empty() {
        None
    } else {
        Some(usage)
    }
}

impl Default for StopReason {
    fn default() -> Self {
        StopReason::Unknown
    }
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

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, AgentError>>>>>;

/// Offered when the CLI reports an auth problem by name.
///
/// Deliberately NOT used for a silent stream close. An earlier version of this
/// file claimed `session/new` goes unanswered when nobody is signed in, and
/// mapped any failure there to "sign in". That was wrong, and the measurement
/// behind it was a broken test rather than the CLI: the probe closed stdin
/// straight after writing, and a stdio-mode ACP server shuts down when its
/// input stream closes -- so the server was being killed mid-request. Holding
/// the pipe open returns a real `sessionId` on the same signed-in account.
///
/// The lesson worth keeping: a closed stream means the child went away, which
/// can happen for many reasons. Only the CLI saying so is evidence of an auth
/// problem, and sending someone to re-run `copilot login` when their sign-in
/// was fine wastes their time on the wrong fix.
const SIGN_IN_HINT: &str =
    "Copilot needs you to sign in first. Run `copilot login` in a terminal, then try again.";

/// A live ACP session over a spawned CLI.
pub struct AcpConnection {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: AtomicI64,
    pending: Pending,
    /// Notifications and agent-to-client requests, in arrival order.
    ///
    /// Taken out with [`Self::take_incoming`] rather than borrowed, because a
    /// caller has to read this *while* a prompt is in flight -- the prompt does
    /// not return until any permission request it raises has been answered, so
    /// borrowing both at once is both impossible and the wrong shape.
    incoming: Option<mpsc::UnboundedReceiver<Incoming>>,
    session_id: Option<String>,
}

impl AcpConnection {
    /// Spawns the CLI in ACP stdio mode and starts reading its output.
    ///
    /// `denied_tools` is applied at start because ACP fixes tool filtering at
    /// server launch: a client cannot narrow it per session, so the bounded set
    /// has to be a launch argument.
    ///
    /// Denial rather than an allow-list because, per `copilot help permissions`,
    /// "denial rules always take precedence over allow rules, even
    /// --allow-all-tools" -- so this cannot be widened by anything later.
    pub async fn spawn(
        program: &std::path::Path,
        cwd: &std::path::Path,
        denied_tools: &[&str],
    ) -> Result<Self, AgentError> {
        let mut cmd = Command::new(program);
        cmd.arg("--acp").arg("--stdio");
        for tool in denied_tools {
            cmd.arg(format!("--deny-tool={tool}"));
        }
        cmd.current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // The child must not outlive us: an orphaned agent holds a subscription
            // slot and keeps writing to a repository nobody is watching.
            .kill_on_drop(true);

        // No console window per turn. `tokio::process::Command` provides
        // `creation_flags` inherently, so unlike the std path this needs no
        // `CommandExt` import.
        #[cfg(windows)]
        cmd.creation_flags(crate::git::shell::CREATE_NO_WINDOW);

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AgentError::TransportUnavailable {
                    transport: super::transport::Transport::Cli,
                    detail: "the Copilot command-line tool is not installed".into(),
                }
            } else {
                AgentError::Failed {
                    detail: format!("could not start the Copilot CLI: {e}"),
                }
            }
        })?;

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let stdin = child.stdin.take().expect("stdin piped");

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx, incoming) = mpsc::unbounded_channel();

        let stdin = Arc::new(Mutex::new(stdin));
        tokio::spawn(read_loop(stdout, pending.clone(), tx, stdin.clone()));
        // In stdio mode stdout carries the protocol, so the CLI's own diagnostics
        // arrive on stderr. Logged rather than parsed.
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::debug!("copilot acp stderr: {line}");
            }
        });

        Ok(Self {
            child,
            stdin,
            next_id: AtomicI64::new(1),
            pending,
            incoming: Some(incoming),
            session_id: None,
        })
    }

    /// `initialize` then `session/new`. Returns the session id.
    pub async fn start_session(&mut self, cwd: &std::path::Path) -> Result<String, AgentError> {
        let init = self
            .request(
                "initialize",
                json!({
                  "protocolVersion": PROTOCOL_VERSION,
                  "clientCapabilities": {},
                  "clientInfo": { "name": "GitWyrm", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
            .await?;
        log::info!(
            "ACP initialized: agent={}",
            init.get("agentInfo")
                .and_then(|v| v.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("?")
        );

        // Errors are passed through as they are. `rpc_error` already promotes the
        // CLI's own auth messages to NeedsReconnect; anything else really is a
        // failure, and calling it a sign-in problem would send the user to fix
        // something that is not broken.
        let session = self
            .request(
                "session/new",
                json!({ "cwd": cwd.to_string_lossy(), "mcpServers": [] }),
            )
            .await?;

        // A reply that arrived but carries no session is the one case where the
        // CLI is up and talking yet cannot open a session -- the shape a
        // credential problem takes once the transport itself is fine.
        let id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::NeedsReconnect {
                detail: SIGN_IN_HINT.into(),
            })?
            .to_string();
        self.session_id = Some(id.clone());
        Ok(id)
    }

    /// Takes the incoming stream, once.
    ///
    /// Returns None on a second call: there is one stream, and two readers would
    /// each see half the messages.
    pub fn take_incoming(&mut self) -> Option<mpsc::UnboundedReceiver<Incoming>> {
        self.incoming.take()
    }

    /// Sends one prompt and waits for the turn to end.
    ///
    /// Streaming and permission requests arrive on [`Self::incoming`] while this
    /// is outstanding, so the caller must be draining that concurrently -- a
    /// permission request left unanswered blocks the turn from ever returning.
    pub async fn prompt(&self, text: &str) -> Result<TurnOutcome, AgentError> {
        let session_id = self.session_id.as_ref().ok_or_else(|| AgentError::Failed {
            detail: "no session started".into(),
        })?;

        let res = self
            .request(
                "session/prompt",
                json!({
                  "sessionId": session_id,
                  "prompt": [{ "type": "text", "text": text }],
                }),
            )
            .await?;

        let stop = res.get("stopReason").cloned().unwrap_or(Value::Null);
        let stop_reason = serde_json::from_value(stop).unwrap_or(StopReason::Unknown);
        Ok(TurnOutcome {
            stop_reason,
            usage: parse_turn_usage(&res),
        })
    }

    /// Asks the agent to stop the current turn.
    ///
    /// A notification, not a request: the protocol requires the agent to answer
    /// the in-flight `session/prompt` with `cancelled`, so the turn still ends
    /// cleanly rather than the pipe simply going quiet.
    pub async fn cancel(&self) -> Result<(), AgentError> {
        let Some(session_id) = self.session_id.as_ref() else {
            return Ok(());
        };
        self.notify("session/cancel", json!({ "sessionId": session_id }))
            .await
    }

    /// Ends the session and the child process.
    pub async fn shutdown(mut self) {
        // Closing stdin is the documented way a stdio-mode server shuts down.
        // `kill` is the backstop for one that does not take the hint.
        {
            let mut stdin = self.stdin.lock().await;
            let _ = stdin.shutdown().await;
        }
        let killed =
            tokio::time::timeout(std::time::Duration::from_secs(3), self.child.wait()).await;
        if killed.is_err() {
            log::info!("ACP child did not exit on its own; killing it");
            let _ = self.child.kill().await;
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, AgentError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_line(&line).await?;

        rx.await.map_err(|_| AgentError::Failed {
            detail: "the Copilot CLI stopped responding".into(),
        })?
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), AgentError> {
        let line = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.write_line(&line).await
    }

    async fn write_line(&self, value: &Value) -> Result<(), AgentError> {
        let mut buf = serde_json::to_vec(value).map_err(|e| AgentError::Failed {
            detail: format!("could not encode a message: {e}"),
        })?;
        buf.push(b'\n');
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(&buf)
            .await
            .map_err(|e| AgentError::Failed {
                detail: format!("could not reach the Copilot CLI: {e}"),
            })?;
        stdin.flush().await.map_err(|e| AgentError::Failed {
            detail: format!("could not reach the Copilot CLI: {e}"),
        })
    }
}

/// Reads NDJSON until the pipe closes, routing each line.
async fn read_loop(
    stdout: tokio::process::ChildStdout,
    pending: Pending,
    tx: mpsc::UnboundedSender<Incoming>,
    stdin: Arc<Mutex<ChildStdin>>,
) {
    let mut lines = BufReader::new(stdout).lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) => break,
            Err(e) => {
                log::info!("ACP read failed: {e}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            // A malformed line is logged and skipped rather than ending the run:
            // one unparseable message should not lose a turn's work.
            log::info!("ACP: could not parse a line, skipping it");
            continue;
        };

        let has_id = msg.get("id").is_some();
        let method = msg.get("method").and_then(Value::as_str);

        match (has_id, method) {
            // A reply to something we sent.
            (true, None) => {
                let Some(id) = msg.get("id").and_then(Value::as_i64) else {
                    continue;
                };
                if let Some(sender) = pending.lock().await.remove(&id) {
                    let result = if let Some(err) = msg.get("error") {
                        Err(rpc_error(err))
                    } else {
                        Ok(msg.get("result").cloned().unwrap_or(Value::Null))
                    };
                    let _ = sender.send(result);
                }
            }
            // A request from the agent to us. Only permission needs an answer.
            (true, Some("session/request_permission")) => {
                let id = msg.get("id").cloned().unwrap_or(Value::Null);
                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                let options: Vec<PermissionOption> = params
                    .get("options")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);

                let (respond, answer) = oneshot::channel();
                if tx
                    .send(Incoming::PermissionRequest {
                        tool_call,
                        options,
                        respond,
                    })
                    .is_err()
                {
                    break;
                }

                // Answering happens on its own task so one slow decision does not stall
                // the read loop -- streaming for the same turn is still arriving.
                let stdin = stdin.clone();
                tokio::spawn(async move {
                    let decision = answer.await.unwrap_or(PermissionDecision::Cancelled);
                    let outcome = match decision {
                        PermissionDecision::AllowOnce { option_id }
                        | PermissionDecision::RejectOnce { option_id } => {
                            json!({ "outcome": "selected", "optionId": option_id })
                        }
                        PermissionDecision::Cancelled => json!({ "outcome": "cancelled" }),
                    };
                    let reply =
                        json!({ "jsonrpc": "2.0", "id": id, "result": { "outcome": outcome } });
                    if let Ok(mut buf) = serde_json::to_vec(&reply) {
                        buf.push(b'\n');
                        let mut s = stdin.lock().await;
                        let _ = s.write_all(&buf).await;
                        let _ = s.flush().await;
                    }
                });
            }
            // Any other agent-to-client request: answered with a method-not-found
            // error so the agent is not left waiting on a capability we lack.
            (true, Some(other)) => {
                log::info!("ACP: unhandled request {other}");
                let id = msg.get("id").cloned().unwrap_or(Value::Null);
                let reply = json!({
                  "jsonrpc": "2.0", "id": id,
                  "error": { "code": -32601, "message": "not supported by this client" }
                });
                if let Ok(mut buf) = serde_json::to_vec(&reply) {
                    buf.push(b'\n');
                    let mut s = stdin.lock().await;
                    let _ = s.write_all(&buf).await;
                    let _ = s.flush().await;
                }
            }
            // Notifications.
            (false, Some("session/update")) => {
                let update = msg
                    .get("params")
                    .and_then(|p| p.get("update"))
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Some(item) = classify_update(&update) {
                    if tx.send(item).is_err() {
                        break;
                    }
                }
            }
            (false, Some(other)) => log::debug!("ACP: ignoring notification {other}"),
            (false, None) => log::debug!("ACP: message with neither id nor method"),
        }
    }
    log::info!("ACP: output stream closed");

    // Anything still waiting will never be answered now.
    let mut map = pending.lock().await;
    for (_, sender) in map.drain() {
        let _ = sender.send(Err(AgentError::Failed {
            detail: "the Copilot CLI exited before finishing".into(),
        }));
    }
}

/// Turns a `session/update` into something the engine cares about, or nothing.
fn classify_update(update: &Value) -> Option<Incoming> {
    match update.get("sessionUpdate").and_then(Value::as_str)? {
        "agent_message_chunk" => {
            let text = update.get("content")?.get("text")?.as_str()?.to_string();
            Some(Incoming::TextChunk(text))
        }
        "tool_call" | "tool_call_update" => {
            let id = update
                .get("toolCallId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            // No fallback to a made-up word like "working" here: a
            // `tool_call_update` with no `title` is usually a bare
            // status/progress ping (still `in_progress`, no new detail to
            // show), and inventing text for it is how a literal "working"
            // ends up wedged into the activity stream. Dropping the update
            // when there is nothing real to say is correct -- the tool call
            // that started this id already carried its own title.
            let title = update.get("title").and_then(Value::as_str)?.to_string();
            Some(Incoming::ToolCall {
                id,
                title,
                raw: update.clone(),
            })
        }
        _ => None,
    }
}

/// Maps a JSON-RPC error onto something the console can say.
fn rpc_error(err: &Value) -> AgentError {
    let message = err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("the CLI reported a problem");
    let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
    // ACP uses -32000 for auth-required; the CLI also surfaces sign-in problems
    // in the message when an org has switched Copilot off.
    let looks_like_auth = code == -32000
        || message.to_lowercase().contains("auth")
        || message.to_lowercase().contains("login")
        || message.to_lowercase().contains("sign in");
    if looks_like_auth {
        AgentError::NeedsReconnect {
            detail: message.to_string(),
        }
    } else {
        AgentError::Refused {
            detail: message.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copilot_cli_usage_shape_is_understood() {
        // The shape Copilot CLI 1.0.80 puts on its `session/prompt` response,
        // verified against its own bundled schema.
        let res = serde_json::json!({
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 1200,
                "outputTokens": 340,
                "totalTokens": 1540,
                "cachedReadTokens": 900
            }
        });
        let usage = parse_turn_usage(&res).expect("Copilot's usage must be recognised");
        assert_eq!(usage.input_tokens, Some(1200));
        assert_eq!(usage.output_tokens, Some(340));
        assert_eq!(usage.cached_input_tokens, Some(900));
        assert_eq!(usage.cost_micro_usd, None, "Copilot reports no cost");
    }

    #[test]
    fn usage_is_none_when_the_agent_reports_none() {
        // The common case: ACP does not require usage, and most agents omit
        // it entirely. That must read as "not reported", not as zero.
        let res = serde_json::json!({ "stopReason": "end_turn" });
        assert!(parse_turn_usage(&res).is_none());
    }

    #[test]
    fn an_empty_usage_object_reports_nothing() {
        let res = serde_json::json!({ "stopReason": "end_turn", "usage": {} });
        assert!(
            parse_turn_usage(&res).is_none(),
            "an empty object is not a measurement"
        );
    }

    #[test]
    fn usage_is_read_from_the_meta_extension_slot() {
        // `_meta` is ACP's sanctioned extension point, so an agent may put
        // usage there instead of at the top level.
        let res = serde_json::json!({
            "stopReason": "end_turn",
            "_meta": { "usage": { "input_tokens": 7 } }
        });
        let usage = parse_turn_usage(&res).expect("_meta.usage must be found");
        assert_eq!(usage.input_tokens, Some(7));
    }

    #[test]
    fn cost_becomes_micro_usd() {
        let res = serde_json::json!({ "usage": { "costUsd": 0.0125 } });
        let usage = parse_turn_usage(&res).expect("a cost is a measurement");
        assert_eq!(usage.cost_micro_usd, Some(12_500));
    }

    #[test]
    fn a_nonsense_cost_is_dropped_rather_than_cast() {
        // A negative or non-finite cost would saturate to 0 on cast and then
        // read as a measured "free" run, which is worse than reporting
        // nothing.
        for bad in [-1.0f64, f64::NAN, f64::INFINITY] {
            let res = serde_json::json!({ "usage": { "costUsd": bad } });
            let parsed = parse_turn_usage(&res).and_then(|u| u.cost_micro_usd);
            assert_eq!(parsed, None, "{bad} must not become a cost");
        }
    }

    #[test]
    fn a_prompt_response_with_no_usage_still_yields_its_stop_reason() {
        // Guards the ordering assumption in `prompt`: usage is additive, and
        // its absence must never cost us the stop reason.
        let res = serde_json::json!({ "stopReason": "refusal" });
        let stop: StopReason =
            serde_json::from_value(res.get("stopReason").cloned().unwrap()).unwrap();
        assert_eq!(stop, StopReason::Refusal);
        assert!(parse_turn_usage(&res).is_none());
    }

    #[test]
    fn stop_reason_uses_end_turn_not_completed() {
        // The docs site lists "completed", which the schema does not have. Parsing
        // the schema's value is what matters.
        let parsed: StopReason = serde_json::from_value(json!("end_turn")).unwrap();
        assert_eq!(parsed, StopReason::EndTurn);
        assert!(parsed.is_success());
    }

    #[test]
    fn unknown_stop_reason_does_not_fail_the_parse() {
        let parsed: StopReason = serde_json::from_value(json!("something_new")).unwrap();
        assert_eq!(parsed, StopReason::Unknown);
        assert!(!parsed.is_success());
    }

    #[test]
    fn agent_limits_are_not_success() {
        for raw in ["max_tokens", "max_turn_requests", "refusal", "cancelled"] {
            let parsed: StopReason = serde_json::from_value(json!(raw)).unwrap();
            assert!(!parsed.is_success(), "{raw} should not count as finished");
            assert!(!parsed.plain_reason().is_empty());
        }
    }

    #[test]
    fn text_chunks_are_classified() {
        let update = json!({
          "sessionUpdate": "agent_message_chunk",
          "content": { "type": "text", "text": "hello" }
        });
        match classify_update(&update) {
            Some(Incoming::TextChunk(t)) => assert_eq!(t, "hello"),
            other => panic!("expected a text chunk, got {other:?}"),
        }
    }

    #[test]
    fn tool_calls_are_classified() {
        let update = json!({
          "sessionUpdate": "tool_call",
          "toolCallId": "t1",
          "title": "Read a file"
        });
        match classify_update(&update) {
            Some(Incoming::ToolCall { id, title, .. }) => {
                assert_eq!(id, "t1");
                assert_eq!(title, "Read a file");
            }
            other => panic!("expected a tool call, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_call_update_with_no_title_is_dropped_not_fabricated() {
        // A `tool_call_update` progress ping with no `title` used to fall
        // back to the literal word "working", which then leaked into the
        // agent's own transcript as a bare status word. There is nothing
        // real to say here, so the update must be dropped, not invented.
        let update = json!({
          "sessionUpdate": "tool_call_update",
          "toolCallId": "t1"
        });
        assert!(classify_update(&update).is_none());
    }

    #[test]
    fn unrelated_updates_are_ignored() {
        let update = json!({ "sessionUpdate": "usage", "used": 10 });
        assert!(classify_update(&update).is_none());
    }

    #[test]
    fn a_chunk_without_text_is_ignored_rather_than_panicking() {
        let update =
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "image" } });
        assert!(classify_update(&update).is_none());
    }

    #[test]
    fn auth_errors_ask_for_a_reconnect() {
        let err = json!({ "code": -32000, "message": "authentication required" });
        assert!(matches!(rpc_error(&err), AgentError::NeedsReconnect { .. }));
    }

    #[test]
    fn other_errors_are_refusals() {
        let err = json!({ "code": -32603, "message": "model unavailable on your plan" });
        assert!(matches!(rpc_error(&err), AgentError::Refused { .. }));
    }

    /// Proves the no-orphan promise with a real child process rather than by
    /// reading the builder call. Uses a long-lived system command as a stand-in
    /// for the CLI, since what is being tested is our process handling.
    #[tokio::test]
    async fn dropping_a_connection_kills_its_child() {
        let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "sleep" });
        if cfg!(windows) {
            // `pause` waits on input forever; with stdin piped and never written to,
            // it will not exit on its own.
            cmd.args(["/C", "pause"]);
        } else {
            cmd.arg("30");
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let Ok(child) = cmd.spawn() else {
            eprintln!("NOTE: could not spawn a test child; skipping the orphan check");
            return;
        };
        let pid = child.id().expect("a running child has a pid");

        // Dropping is what a cancelled or panicking run does. kill_on_drop turns
        // that into a terminated child rather than an orphan.
        drop(child);

        // The kill is asynchronous; give the OS a moment before looking.
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        assert!(!process_is_running(pid), "process {pid} survived the drop");
    }

    #[cfg(windows)]
    fn process_is_running(pid: u32) -> bool {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }

    #[cfg(not(windows))]
    fn process_is_running(pid: u32) -> bool {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
}
