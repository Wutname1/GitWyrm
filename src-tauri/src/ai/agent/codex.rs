//! Driving Codex through its own app-server, rather than a bridge.
//!
//! # Why not ACP
//!
//! Codex does not speak it. `codex --help` on 0.151.0 offers `app-server` and
//! `mcp-server` and no ACP mode at all, which is why a bridge package exists
//! for it. Asking someone to install a bridge for a tool already working in
//! their terminal is the wrong trade, so GitWyrm learns Codex's language
//! instead.
//!
//! # The protocol, as measured
//!
//! JSON-RPC on stdio, one message per line. Every shape below was taken from a
//! real 0.151.0 session rather than from documentation:
//!
//! - `initialize` answers with a `userAgent` and the Codex home directory.
//! - `thread/start` answers with `result.thread.id`. It accepts `cwd` and
//!   `sandbox`, both optional.
//! - `turn/start` takes `{ threadId, input: [{ type: "text", text }] }`.
//! - The agent's prose arrives as `item/agentMessage/delta` with a `delta`
//!   string, and `turn/completed` carries the finished turn.
//! - `thread/tokenUsage/updated` carries `tokenUsage.last` plus
//!   `modelContextWindow`.
//! - Approvals arrive as server-to-client REQUESTS -- `item/fileChange/
//!   requestApproval`, `item/commandExecution/requestApproval` and
//!   `item/permissions/requestApproval` -- which must be answered or the
//!   turn hangs. The answer vocabulary comes from the schema the binary
//!   generates (`codex app-server generate-json-schema`): `accept` /
//!   `decline` / `cancel`. An earlier build answered `approved` / `denied`,
//!   which Codex treated as a refusal, so every write a person allowed was
//!   still refused and the agent reported "write access was denied".
//! - A file-change approval names no paths itself; they arrive earlier on
//!   the `item/started` notification for the same `itemId`, so the read loop
//!   remembers them per item.
//!
//! # The trap that cost an hour
//!
//! Closing stdin kills the server mid-request. An early probe wrote both
//! messages and closed the pipe, and `thread/start` simply never answered --
//! which reads exactly like an unsupported method. `acp.rs` documents the same
//! trap for the same reason. Hold the pipe open.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

use super::transport::AgentError;
use super::wire::{Incoming, PermissionDecision, StopReason, TurnOutcome};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, AgentError>>>>>;
type Complaints = Arc<Mutex<std::collections::VecDeque<String>>>;
const COMPLAINT_LINES: usize = 8;

/// One model Codex says it offers.
///
/// Only the fields GitWyrm shows or sends. The response carries a dozen more
/// (`serviceTiers`, `inputModalities`, `multiAgentVersion`, ...) that nothing
/// here reads; leaving them out keeps this a statement of what GitWyrm uses
/// rather than a second copy of Codex's schema that would need maintaining.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexModel {
    /// Passed to Codex verbatim. Never shown as-is.
    pub id: String,
    /// What the user sees, as Codex writes it (e.g. "GPT-5.6-Sol").
    pub display_name: String,
    /// Codex's own one-line description, or empty when it gives none.
    pub description: String,
    /// The model Codex would pick for itself.
    pub is_default: bool,
    /// How hard this model can be asked to think, in the order Codex lists
    /// them (lowest first).
    ///
    /// Per model, not per tool: on 0.154.0 `gpt-5.5` accepts four levels while
    /// every other model accepts six. The registry row had no way to say that,
    /// which is why Codex's effort support was recorded as none at all.
    pub efforts: Vec<String>,
    /// The effort Codex would use if none is chosen, when it names one.
    pub default_effort: Option<String>,
}

impl CodexModel {
    /// Reads one entry, or `None` when it is hidden or has no usable id.
    ///
    /// Tolerant by design: a model missing a display name still renders under
    /// its id, and one missing efforts simply cannot be asked to think harder.
    /// Refusing the whole list over one unfamiliar row would turn a tool that
    /// added a field into a tool GitWyrm cannot read at all.
    fn parse(entry: &Value) -> Option<Self> {
        if entry.get("hidden").and_then(Value::as_bool).unwrap_or(false) {
            return None;
        }
        let id = entry
            .get("id")
            .or_else(|| entry.get("model"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())?
            .to_string();
        let display_name = entry
            .get("displayName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&id)
            .to_string();
        let efforts = entry
            .get("supportedReasoningEfforts")
            .and_then(Value::as_array)
            .map(|levels| {
                levels
                    .iter()
                    .filter_map(|l| {
                        l.get("reasoningEffort")
                            .or(Some(l))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(Self {
            id,
            display_name,
            description: entry
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
            is_default: entry
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            default_effort: entry
                .get("defaultReasoningEffort")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            efforts,
        })
    }
}

/// A live Codex app-server session.
pub struct CodexConnection {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: AtomicI64,
    pending: Pending,
    incoming: Option<mpsc::UnboundedReceiver<Incoming>>,
    thread_id: Option<String>,
}

impl CodexConnection {
    /// Starts `codex app-server` and begins reading it.
    ///
    /// `read_only` maps to the sandbox the thread is started with. Unlike a
    /// per-tool denial this covers the whole session, which is why Codex can
    /// be trusted with read-only work at all -- see `registry::Denial`.
    pub async fn spawn(program: &Path, cwd: &Path, args: &[String]) -> Result<Self, AgentError> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // The child must not outlive us: an orphan holds a subscription
            // slot and keeps writing to a repository nobody is watching.
            .kill_on_drop(true);
        // Same as every other child GitWyrm spawns: an AppImage's bundled
        // loader variables must not leak into a system binary.
        crate::process_env::scrub_bundled_env(&mut cmd);

        #[cfg(windows)]
        cmd.creation_flags(crate::git::shell::CREATE_NO_WINDOW);

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AgentError::TransportUnavailable {
                    transport: super::transport::Transport::Cli,
                    detail: "the Codex command-line tool is not installed".into(),
                }
            } else {
                AgentError::Failed {
                    detail: format!("could not start Codex: {e}"),
                }
            }
        })?;

        let stdout = child.stdout.take().ok_or_else(|| AgentError::Failed {
            detail: "Codex started without an output stream".into(),
        })?;
        let stdin = child.stdin.take().ok_or_else(|| AgentError::Failed {
            detail: "Codex started without an input stream".into(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| AgentError::Failed {
            detail: "Codex started without an error stream".into(),
        })?;

        // Keep the last few lines Codex wrote to stderr. When it dies before
        // answering, this is the only account of why -- a bad flag, a missing
        // login -- and without it the failure is just "stopped unexpectedly".
        let complaints: Complaints = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        {
            let complaints = complaints.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::debug!("Codex stderr: {line}");
                    let mut guard = complaints.lock().await;
                    if guard.len() >= COMPLAINT_LINES {
                        guard.pop_front();
                    }
                    guard.push_back(line);
                }
            });
        }

        let stdin = Arc::new(Mutex::new(stdin));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = mpsc::unbounded_channel();

        log::info!("Codex: started {} {}", program.display(), args.join(" "));
        tokio::spawn(read_loop(stdout, pending.clone(), stdin.clone(), tx, complaints, cwd.to_path_buf()));

        Ok(Self {
            child,
            stdin,
            next_id: AtomicI64::new(1),
            pending,
            incoming: Some(rx),
            thread_id: None,
        })
    }

    /// `initialize` then `thread/start`. Returns the thread id.
    pub async fn start_session(
        &mut self,
        cwd: &Path,
        read_only: bool,
    ) -> Result<String, AgentError> {
        self.request(
            "initialize",
            json!({ "clientInfo": { "name": "GitWyrm", "version": env!("CARGO_PKG_VERSION") } }),
        )
        .await?;

        let mut params = json!({ "cwd": cwd.to_string_lossy() });
        if read_only {
            // The whole-session bound. Set here rather than per turn so a
            // later turn cannot quietly widen it.
            params["sandbox"] = json!("read-only");
        }

        log::info!("Codex: starting a conversation in {}", cwd.display());
        let res = self.request("thread/start", params).await?;
        let id = res
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::Failed {
                detail: "Codex started without giving the conversation an id".into(),
            })?
            .to_string();
        self.thread_id = Some(id.clone());
        Ok(id)
    }

    /// Every model this Codex offers, asked of Codex rather than remembered.
    ///
    /// The registry's `choices` were written by hand against codex-cli 0.151.0
    /// and pinned two `gpt-5.4` entries. By 0.154.0 the tool offered six, none
    /// of them 5.4, and nothing noticed -- a compile-time list cannot tell when
    /// the thing it describes has moved on. `model/list` is the tool's own
    /// answer to the same question, on the connection GitWyrm already uses.
    ///
    /// Needs `initialize` and nothing else: no thread, no repository, no
    /// sandbox. Listing models is not a turn.
    ///
    /// `hidden` models are dropped. Codex marks its internal rows that way
    /// (`gpt-reserve`, `codex-auto-review` on 0.154.0) and its own picker does
    /// not show them, so neither does this.
    pub async fn list_models(&self) -> Result<Vec<CodexModel>, AgentError> {
        self.request(
            "initialize",
            json!({ "clientInfo": { "name": "GitWyrm", "version": env!("CARGO_PKG_VERSION") } }),
        )
        .await?;

        let mut models = Vec::new();
        let mut cursor: Option<String> = None;
        // Paginated even though 0.154.0 answers in one page: `nextCursor` is
        // in the protocol, so a build that starts using it must not silently
        // truncate the list.
        loop {
            let mut params = json!({});
            if let Some(c) = &cursor {
                params["cursor"] = json!(c);
            }
            let res = self.request("model/list", params).await?;
            let page = res.get("data").and_then(Value::as_array).ok_or_else(|| {
                AgentError::Failed {
                    detail: "Codex answered its model list without any models in it".into(),
                }
            })?;
            for entry in page {
                if let Some(model) = CodexModel::parse(entry) {
                    models.push(model);
                }
            }
            cursor = res
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            // A server that keeps handing back a cursor would loop forever;
            // an empty page with a cursor is the shape that would do it.
            if cursor.is_none() || page.is_empty() {
                break;
            }
        }
        Ok(models)
    }

    pub fn take_incoming(&mut self) -> Option<mpsc::UnboundedReceiver<Incoming>> {
        self.incoming.take()
    }

    /// Sends one turn and waits for it to end.
    ///
    /// `turn/start` answers immediately with the turn in `inProgress`; the
    /// finished turn arrives later as a `turn/completed` notification. So the
    /// wait is on that notification, not on the response.
    pub async fn prompt(&self, text: &str) -> Result<TurnOutcome, AgentError> {
        let thread_id = self.thread_id.as_ref().ok_or_else(|| AgentError::Failed {
            detail: "no conversation started".into(),
        })?;

        let (done_tx, done_rx) = oneshot::channel();
        {
            let mut guard = self.pending.lock().await;
            // Turn completion is keyed off a reserved id no request uses, so
            // the read loop can hand it back through the same map.
            guard.insert(TURN_DONE_KEY, done_tx);
        }

        self.request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "input": [{ "type": "text", "text": text }],
            }),
        )
        .await?;

        log::debug!("Codex: turn sent, waiting for it to finish");
        let completed = done_rx.await.map_err(|_| AgentError::Failed {
            detail: "Codex stopped before finishing the turn".into(),
        })??;

        Ok(TurnOutcome {
            stop_reason: stop_reason_of(&completed),
            // Spend arrives separately as `thread/tokenUsage/updated` and is
            // reported through `Incoming::ContextUsage`, so nothing to add.
            usage: None,
        })
    }

    /// Sends one prompt and returns everything the agent said.
    pub async fn ask(&mut self, text: &str) -> Result<String, AgentError> {
        let mut incoming = self.take_incoming().ok_or_else(|| AgentError::Failed {
            detail: "this connection is already being read somewhere else".into(),
        })?;

        let prompt = self.prompt(text);
        tokio::pin!(prompt);

        let mut said = String::new();
        loop {
            tokio::select! {
                result = &mut prompt => {
                    result?;
                    while let Ok(item) = incoming.try_recv() {
                        if let Incoming::TextChunk(chunk) = item {
                            said.push_str(&chunk);
                        }
                    }
                    return Ok(said);
                }
                Some(item) = incoming.recv() => {
                    if let Incoming::TextChunk(chunk) = item {
                        said.push_str(&chunk);
                    }
                }
            }
        }
    }

    pub async fn cancel(&self) -> Result<(), AgentError> {
        let Some(thread_id) = self.thread_id.as_ref() else {
            return Ok(());
        };
        self.notify("turn/interrupt", json!({ "threadId": thread_id }))
            .await
    }

    pub async fn shutdown(mut self) {
        {
            let mut stdin = self.stdin.lock().await;
            let _ = stdin.shutdown().await;
        }
        let killed =
            tokio::time::timeout(std::time::Duration::from_secs(3), self.child.wait()).await;
        if killed.is_err() {
            log::info!("Codex did not exit on its own; killing it");
            let _ = self.child.kill().await;
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, AgentError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write_line(json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }))
        .await?;
        rx.await.map_err(|_| AgentError::Failed {
            detail: "Codex closed before answering".into(),
        })?
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), AgentError> {
        self.write_line(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await
    }

    async fn write_line(&self, value: Value) -> Result<(), AgentError> {
        let mut line = value.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| AgentError::Failed {
                detail: format!("could not send to Codex: {e}"),
            })?;
        stdin.flush().await.map_err(|e| AgentError::Failed {
            detail: format!("could not send to Codex: {e}"),
        })
    }
}

/// The pending-map slot a finished turn is delivered through.
///
/// Negative so it can never collide with a real request id, which counts up
/// from 1.
const TURN_DONE_KEY: i64 = -1;

/// Reads Codex's output forever, routing replies and notifications.
async fn read_loop(
    stdout: tokio::process::ChildStdout,
    pending: Pending,
    stdin: Arc<Mutex<ChildStdin>>,
    tx: mpsc::UnboundedSender<Incoming>,
    complaints: Complaints,
    cwd: std::path::PathBuf,
) {
    let mut lines = BufReader::new(stdout).lines();
    // Paths per file-change item, learnt from `item/started`. The approval
    // request for the same item carries only its id.
    let mut file_change_paths: HashMap<String, Vec<String>> = HashMap::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        let method = msg.get("method").and_then(Value::as_str);
        let has_id = msg.get("id").is_some();
        if method == Some("item/started") {
            if let Some((id, paths)) = file_change_of(msg.get("params").unwrap_or(&Value::Null)) {
                // Codex names files by absolute path. The approval card is
                // read by a person who knows the project, so it shows the
                // path inside it.
                let paths = paths.into_iter().map(|p| relative_to(&cwd, &p)).collect();
                file_change_paths.insert(id, paths);
            }
        }

        match (has_id, method) {
            // A reply to something we asked.
            (true, None) => {
                let Some(id) = msg.get("id").and_then(Value::as_i64) else {
                    continue;
                };
                if let Some(sender) = pending.lock().await.remove(&id) {
                    let result = if let Some(err) = msg.get("error") {
                        Err(AgentError::Refused {
                            detail: err
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("Codex reported a problem")
                                .to_string(),
                        })
                    } else {
                        Ok(msg.get("result").cloned().unwrap_or(Value::Null))
                    };
                    let _ = sender.send(result);
                }
            }

            // An approval request. Must be answered or the turn hangs.
            (true, Some(m)) if m.ends_with("Approval") => {
                let id = msg.get("id").cloned().unwrap_or(Value::Null);
                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                let item_paths = params
                    .get("itemId")
                    .and_then(Value::as_str)
                    .and_then(|item| file_change_paths.get(item))
                    .cloned()
                    .unwrap_or_default();

                let (respond, answer) = oneshot::channel();
                let request = super::wire::PermissionRequest {
                    // A file change is a write; a command execution is one
                    // too, because it can do anything; a permission grant
                    // widens what the sandbox allows. Anything else this
                    // build does not recognise is a write as well.
                    capability: crate::agentdesk::policy::ToolCapability::EditFile,
                    // One path is a path. Several are not reduced to the
                    // first: a path-scoped helper must be judged on all of
                    // them, and `None` makes that check refuse.
                    path: match item_paths.as_slice() {
                        [only] => Some(only.clone()),
                        _ => None,
                    },
                    summary: approval_summary(m, &params, &item_paths),
                    respond,
                };
                if tx.send(Incoming::PermissionRequest(request)).is_err() {
                    break;
                }

                let stdin = stdin.clone();
                let method = m.to_string();
                tokio::spawn(async move {
                    let decision = answer.await.unwrap_or(PermissionDecision::Cancelled);
                    let reply = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": approval_reply(&method, &params, decision),
                    });
                    let mut line = reply.to_string();
                    line.push('\n');
                    let mut guard = stdin.lock().await;
                    let _ = guard.write_all(line.as_bytes()).await;
                    let _ = guard.flush().await;
                });
            }

            // Any other request FROM Codex. It is waiting for an answer, and a
            // request nobody answers hangs the turn forever with nothing in
            // the log to say why -- which reads, from the outside, as "I sent
            // a message and nothing happened". Refuse it explicitly so the
            // turn moves on and the refusal is visible.
            (true, Some(m)) => {
                log::info!("Codex asked something GitWyrm cannot answer ({m}); declining");
                let id = msg.get("id").cloned().unwrap_or(Value::Null);
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": "GitWyrm does not handle this request" },
                });
                let mut line = reply.to_string();
                line.push('\n');
                let mut guard = stdin.lock().await;
                let _ = guard.write_all(line.as_bytes()).await;
                let _ = guard.flush().await;
            }

            // A notification.
            (false, Some(m)) => {
                if m == "turn/completed" {
                    let turn = msg
                        .get("params")
                        .and_then(|p| p.get("turn"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    if let Some(sender) = pending.lock().await.remove(&TURN_DONE_KEY) {
                        let _ = sender.send(Ok(turn));
                    }
                    continue;
                }
                if let Some(item) = classify(m, msg.get("params").unwrap_or(&Value::Null)) {
                    if tx.send(item).is_err() {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    // The stream ended. Anything still waiting would hang forever otherwise.
    // Say why if Codex said why: its last words on stderr are the difference
    // between a fixable report and a shrug.
    let said = complaints.lock().await.iter().cloned().collect::<Vec<_>>().join(" / ");
    let detail = if said.trim().is_empty() {
        "Codex stopped unexpectedly".to_string()
    } else {
        format!("Codex stopped unexpectedly: {said}")
    };
    log::warn!("{detail}");
    let mut guard = pending.lock().await;
    for (_, sender) in guard.drain() {
        let _ = sender.send(Err(AgentError::Failed {
            detail: detail.clone(),
        }));
    }
}

/// Turns one Codex notification into something the run loop understands, or
/// nothing when it is noise.
fn classify(method: &str, params: &Value) -> Option<Incoming> {
    match method {
        // The agent's prose, one fragment at a time.
        "item/agentMessage/delta" => {
            let delta = params.get("delta").and_then(Value::as_str)?;
            Some(Incoming::TextChunk(delta.to_string()))
        }

        // Something the agent did. Its own messages are prose, not activity,
        // and are already carried by the deltas above.
        "item/started" => {
            let item = params.get("item")?;
            let kind = item.get("type").and_then(Value::as_str)?;
            if matches!(kind, "agentMessage" | "userMessage") {
                return None;
            }
            Some(Incoming::ToolCall {
                title: item
                    .get("title")
                    .or_else(|| item.get("command"))
                    .and_then(Value::as_str)
                    .unwrap_or(kind)
                    .to_string(),
            })
        }

        // How full the context window is. `last` is this turn; `total` is the
        // conversation. Occupancy is the conversation's, matching what the
        // window actually holds.
        "thread/tokenUsage/updated" => {
            let usage = params.get("tokenUsage")?;
            let used = usage
                .get("total")
                .and_then(|t| t.get("totalTokens"))
                .and_then(Value::as_u64)?;
            let size = params
                .get("tokenUsage")
                .and_then(|u| u.get("modelContextWindow"))
                .or_else(|| usage.get("modelContextWindow"))
                .and_then(Value::as_u64)?;
            Some(Incoming::ContextUsage {
                used: used.min(u32::MAX as u64) as u32,
                size: size.min(u32::MAX as u64) as u32,
                // Codex reports tokens, never money.
                cost_micro_usd: None,
            })
        }

        _ => None,
    }
}

/// One line describing what an approval is asking for.
fn approval_summary(method: &str, params: &Value, item_paths: &[String]) -> String {
    if method.contains("commandExecution") || method == "execCommandApproval" {
        let command = params
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("a command");
        return format!("Run {command}");
    }
    if method.contains("permissions") {
        let reason = params.get("reason").and_then(Value::as_str).unwrap_or("");
        let wants = permission_wants(params.get("permissions").unwrap_or(&Value::Null));
        return match (wants.is_empty(), reason.is_empty()) {
            (false, false) => format!("Allow {wants} ({reason})"),
            (false, true) => format!("Allow {wants}"),
            (true, false) => format!("Allow extra access ({reason})"),
            (true, true) => "Allow extra access".to_string(),
        };
    }
    match item_paths {
        [] => {
            // Older shapes and the legacy `applyPatchApproval` may still name
            // a path inline; otherwise the honest summary is the plain one.
            let path = params
                .get("fileChange")
                .and_then(|c| c.get("path"))
                .or_else(|| params.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("files");
            format!("Change {path}")
        }
        [only] => format!("Change {only}"),
        many => format!("Change {} files: {}", many.len(), many.join(", ")),
    }
}

/// The permissions a request asks for, in words: the paths it wants to
/// write and whether it wants the network.
fn permission_wants(profile: &Value) -> String {
    let mut parts = Vec::new();
    let fs = profile.get("fileSystem").unwrap_or(&Value::Null);
    let mut paths: Vec<String> = Vec::new();
    for key in ["write", "read"] {
        if let Some(list) = fs.get(key).and_then(Value::as_array) {
            paths.extend(list.iter().filter_map(Value::as_str).map(|p| format!("{key} {p}")));
        }
    }
    if let Some(entries) = fs.get("entries").and_then(Value::as_array) {
        for e in entries {
            let target = e
                .get("path")
                .and_then(|p| p.get("path").or_else(|| p.get("pattern")))
                .and_then(Value::as_str)
                .unwrap_or("a location");
            let access = e.get("access").and_then(Value::as_str).unwrap_or("access to");
            paths.push(format!("{access} {target}"));
        }
    }
    if !paths.is_empty() {
        parts.push(paths.join(", "));
    }
    if profile
        .get("network")
        .and_then(|n| n.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        parts.push("network access".to_string());
    }
    parts.join(" and ")
}

/// The `result` for one approval request, in the vocabulary that request
/// expects. Three shapes exist in 0.151.0 and they do not share words:
///
/// - the v2 item requests take `accept` / `decline` / `cancel`;
/// - the legacy `applyPatchApproval` / `execCommandApproval` take
///   `approved` / `denied` / `abort`;
/// - `item/permissions/requestApproval` takes no decision at all but a
///   granted profile: echoing the requested one back is "yes", an empty
///   one is "no". Grants are scoped to the turn, never the session: a
///   person approved one thing once.
fn approval_reply(method: &str, params: &Value, decision: PermissionDecision) -> Value {
    if method.contains("permissions") {
        return match decision {
            PermissionDecision::AllowOnce => json!({
                "permissions": params.get("permissions").cloned().unwrap_or_else(|| json!({})),
                "scope": "turn",
            }),
            _ => json!({ "permissions": {} }),
        };
    }
    let legacy = matches!(method, "applyPatchApproval" | "execCommandApproval");
    let word = match (decision, legacy) {
        (PermissionDecision::AllowOnce, false) => "accept",
        (PermissionDecision::RejectOnce, false) => "decline",
        (PermissionDecision::Cancelled, false) => "cancel",
        (PermissionDecision::AllowOnce, true) => "approved",
        (PermissionDecision::RejectOnce, true) => "denied",
        (PermissionDecision::Cancelled, true) => "abort",
    };
    json!({ "decision": word })
}

/// `path` inside `root` when it is inside it, otherwise unchanged. Compared
/// case-insensitively and separator-insensitively on Windows, where Codex and
/// the app can spell the same folder differently.
fn relative_to(root: &std::path::Path, path: &str) -> String {
    let sep = std::path::MAIN_SEPARATOR;
    let norm = |text: &str| {
        let slashed = text.replace(sep, "/");
        if cfg!(windows) { slashed.to_lowercase() } else { slashed }
    };
    let root_n = norm(&root.to_string_lossy()).trim_end_matches('/').to_string();
    let path_n = norm(path);
    match path_n.strip_prefix(&root_n) {
        Some(rest) if rest.starts_with('/') => {
            // Cut the ORIGINAL text at the same length so the file's own
            // casing survives; only the comparison was lowered.
            let original = path.replace(sep, "/");
            original[root_n.len() + 1..].to_string()
        }
        _ => path.to_string(),
    }
}

/// `(item id, paths)` when an `item/started` notification is a file change.
fn file_change_of(params: &Value) -> Option<(String, Vec<String>)> {
    let item = params.get("item")?;
    if item.get("type").and_then(Value::as_str) != Some("fileChange") {
        return None;
    }
    let id = item.get("id").and_then(Value::as_str)?.to_string();
    let paths = item
        .get("changes")
        .and_then(Value::as_array)
        .map(|changes| {
            changes
                .iter()
                .filter_map(|c| c.get("path").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some((id, paths))
}

/// Why a turn ended, from the completed turn Codex sent.
fn stop_reason_of(turn: &Value) -> StopReason {
    match turn.get("status").and_then(Value::as_str) {
        Some("completed") => StopReason::EndTurn,
        Some("interrupted") | Some("cancelled") => StopReason::Cancelled,
        Some("failed") => StopReason::Refusal,
        // Anything this build has not seen is not a clean finish.
        _ => StopReason::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole path a chat walks, against the Codex on this machine:
    /// spawn, handshake, start a conversation, send one turn, get prose
    /// back. Run it by hand when a Codex chat "does nothing":
    ///
    /// `cargo test --lib codex_answers_a_real_turn -- --ignored --nocapture`
    ///
    /// Ignored because it needs Codex installed and signed in, and spends a
    /// (tiny) amount of someone's quota.
    #[tokio::test]
    #[ignore]
    async fn codex_answers_a_real_turn() {
        let spec = super::super::registry::find("codex").expect("codex row");
        let found = super::super::copilot_cli::detect_agent(spec);
        let program = match found.state {
            super::super::copilot_cli::CliState::Ready { path, .. } => std::path::PathBuf::from(path),
            other => panic!("codex not usable on this machine: {other:?}"),
        };
        let cwd = std::env::temp_dir();

        let mut conn = CodexConnection::spawn(&program, &cwd, &spec.launch_args(&["write"]))
            .await
            .expect("spawn");
        let thread = conn.start_session(&cwd, true).await.expect("start_session");
        eprintln!("thread {thread}");

        let said = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            conn.ask("Reply with exactly the word PONG and nothing else."),
        )
        .await
        .expect("codex did not finish the turn within two minutes")
        .expect("turn failed");
        eprintln!("codex said: {said:?}");
        conn.shutdown().await;
        assert!(said.to_uppercase().contains("PONG"), "got {said:?}");
    }

    /// The words Codex 0.151.0's schema requires, per request family. A
    /// wrong word is not an error: Codex treats it as a refusal, which is how
    /// a person's Allow turned into "write access was denied" in a live run.
    #[test]
    fn approval_replies_use_each_requests_own_vocabulary() {
        let p = json!({});
        for method in ["item/fileChange/requestApproval", "item/commandExecution/requestApproval"] {
            assert_eq!(approval_reply(method, &p, PermissionDecision::AllowOnce)["decision"], "accept", "{method}");
            assert_eq!(approval_reply(method, &p, PermissionDecision::RejectOnce)["decision"], "decline", "{method}");
            assert_eq!(approval_reply(method, &p, PermissionDecision::Cancelled)["decision"], "cancel", "{method}");
        }
        for method in ["applyPatchApproval", "execCommandApproval"] {
            assert_eq!(approval_reply(method, &p, PermissionDecision::AllowOnce)["decision"], "approved", "{method}");
            assert_eq!(approval_reply(method, &p, PermissionDecision::RejectOnce)["decision"], "denied", "{method}");
            assert_eq!(approval_reply(method, &p, PermissionDecision::Cancelled)["decision"], "abort", "{method}");
        }
    }

    #[test]
    fn a_permission_grant_echoes_the_request_for_one_turn_and_grants_nothing_on_refusal() {
        let params = json!({
            "itemId": "i1",
            "reason": "write to the workspace",
            "permissions": { "fileSystem": { "write": ["C:/work"] }, "network": { "enabled": false } }
        });
        let yes = approval_reply("item/permissions/requestApproval", &params, PermissionDecision::AllowOnce);
        assert_eq!(yes["permissions"], params["permissions"]);
        assert_eq!(yes["scope"], "turn", "never a session-wide grant from one Allow");
        let no = approval_reply("item/permissions/requestApproval", &params, PermissionDecision::RejectOnce);
        assert_eq!(no["permissions"], json!({}));
        assert!(no.get("scope").is_none());

        let summary = approval_summary("item/permissions/requestApproval", &params, &[]);
        assert!(summary.contains("write C:/work"), "{summary}");
        assert!(summary.contains("write to the workspace"), "{summary}");
    }

    /// A file-change approval names no paths; they come from the earlier
    /// `item/started` for the same item.
    #[test]
    fn file_change_paths_are_learnt_from_item_started() {
        let started = json!({ "item": { "type": "fileChange", "id": "fc-1", "status": "inProgress",
            "changes": [ { "path": "src/a.rs", "kind": { "type": "update" }, "diff": "" },
                         { "path": "src/b.rs", "kind": { "type": "add" }, "diff": "" } ] } });
        let (id, paths) = file_change_of(&started).expect("a file change");
        assert_eq!(id, "fc-1");
        assert_eq!(paths, vec!["src/a.rs", "src/b.rs"]);
        assert!(file_change_of(&json!({ "item": { "type": "commandExecution", "id": "c" } })).is_none());

        let two = approval_summary("item/fileChange/requestApproval", &json!({ "itemId": "fc-1" }), &paths);
        assert_eq!(two, "Change 2 files: src/a.rs, src/b.rs");
        let one = approval_summary("item/fileChange/requestApproval", &json!({}), &paths[..1]);
        assert_eq!(one, "Change src/a.rs");
    }

    #[test]
    fn file_paths_are_shown_inside_the_project_when_they_are() {
        let s = std::path::MAIN_SEPARATOR;
        let root_text = format!("{s}work{s}repo");
        let root = std::path::Path::new(&root_text);
        assert_eq!(relative_to(root, &format!("{s}work{s}repo{s}src{s}Lib.rs")), "src/Lib.rs");
        assert_eq!(relative_to(root, &format!("{s}work{s}repo2{s}x.rs")), format!("{s}work{s}repo2{s}x.rs"));
        assert_eq!(relative_to(root, "elsewhere.rs"), "elsewhere.rs");
        if cfg!(windows) {
            assert_eq!(relative_to(root, &format!("{s}WORK{s}Repo{s}a.rs")), "a.rs", "case differs, same folder");
        }
    }

    #[test]
    fn agent_prose_arrives_as_text() {
        // Shape taken from a real 0.151.0 turn.
        let params = json!({ "delta": "PONG", "threadId": "t", "itemId": "m" });
        match classify("item/agentMessage/delta", &params) {
            Some(Incoming::TextChunk(t)) => assert_eq!(t, "PONG"),
            other => panic!("expected prose, got {other:?}"),
        }
    }

    #[test]
    fn the_agents_own_message_is_not_reported_as_activity() {
        // It is already carried by the deltas; reporting it again would show
        // every reply twice, once as prose and once as a tool row.
        let params = json!({ "item": { "type": "agentMessage", "id": "m" } });
        assert!(classify("item/started", &params).is_none());
        let user = json!({ "item": { "type": "userMessage", "id": "u" } });
        assert!(classify("item/started", &user).is_none());
    }

    #[test]
    fn a_real_tool_is_reported_as_activity() {
        let params = json!({ "item": { "type": "commandExecution", "command": "npm test" } });
        match classify("item/started", &params) {
            Some(Incoming::ToolCall { title }) => assert_eq!(title, "npm test"),
            other => panic!("expected activity, got {other:?}"),
        }
    }

    #[test]
    fn token_usage_becomes_context_occupancy() {
        // Verbatim from a measured session.
        let params = json!({
            "tokenUsage": {
                "total": { "totalTokens": 21892, "inputTokens": 21884, "outputTokens": 8 },
                "last": { "totalTokens": 21892 },
                "modelContextWindow": 258400
            }
        });
        match classify("thread/tokenUsage/updated", &params) {
            Some(Incoming::ContextUsage { used, size, cost_micro_usd }) => {
                assert_eq!(used, 21892);
                assert_eq!(size, 258400);
                assert_eq!(cost_micro_usd, None, "Codex reports tokens, not money");
            }
            other => panic!("expected context usage, got {other:?}"),
        }
    }

    #[test]
    fn usage_without_a_window_size_is_ignored_rather_than_guessed() {
        let params = json!({ "tokenUsage": { "total": { "totalTokens": 10 } } });
        assert!(classify("thread/tokenUsage/updated", &params).is_none());
    }

    #[test]
    fn a_completed_turn_is_a_finished_one() {
        assert_eq!(stop_reason_of(&json!({ "status": "completed" })), StopReason::EndTurn);
    }

    #[test]
    fn an_interrupted_turn_is_not_a_finish() {
        for status in ["interrupted", "cancelled"] {
            assert_eq!(stop_reason_of(&json!({ "status": status })), StopReason::Cancelled);
        }
        assert!(!stop_reason_of(&json!({ "status": "failed" })).is_success());
    }

    #[test]
    fn an_unknown_turn_status_is_never_read_as_success() {
        // A status this build has not seen must not let a run report itself
        // finished, or the work checker would audit a turn that never ran.
        for status in [json!({ "status": "something_new" }), json!({}), Value::Null] {
            assert!(!stop_reason_of(&status).is_success(), "{status}");
        }
    }

    #[test]
    fn an_approval_names_what_it_would_change() {
        let params = json!({ "fileChange": { "path": "src/main.rs" } });
        assert!(approval_summary("item/fileChange/requestApproval", &params, &[]).contains("src/main.rs"));
        let cmd = json!({ "command": "rm -rf /" });
        assert!(approval_summary("item/commandExecution/requestApproval", &cmd, &[]).contains("rm -rf /"));
    }

    #[test]
    fn an_approval_with_nothing_named_still_reads_as_a_sentence() {
        let s = approval_summary("item/fileChange/requestApproval", &json!({}), &[]);
        assert!(!s.is_empty());
        assert!(s.contains("file"));
    }

    #[test]
    fn the_turn_done_slot_cannot_collide_with_a_request() {
        // Request ids count up from 1, so a negative reserved key is safe.
        assert!(TURN_DONE_KEY < 1);
    }

    /// Shaped exactly like one entry from codex-cli 0.154.0's `model/list`.
    fn live_entry() -> Value {
        json!({
            "id": "gpt-5.6-sol",
            "model": "gpt-5.6-sol",
            "displayName": "GPT-5.6-Sol",
            "description": "Reliable agentic workhorse for everyday tasks.",
            "hidden": false,
            "isDefault": false,
            "defaultReasoningEffort": "low",
            "supportedReasoningEfforts": [
                { "reasoningEffort": "low", "description": "Fast responses with lighter reasoning" },
                { "reasoningEffort": "high", "description": "Greater reasoning depth for complex problems" }
            ],
            "inputModalities": ["text", "image"],
            "serviceTiers": [{ "id": "priority", "name": "Fast" }]
        })
    }

    #[test]
    fn a_live_model_entry_reads_the_fields_gitwyrm_shows() {
        let m = CodexModel::parse(&live_entry()).expect("a visible model");
        assert_eq!(m.id, "gpt-5.6-sol");
        assert_eq!(m.display_name, "GPT-5.6-Sol");
        assert_eq!(m.efforts, vec!["low", "high"]);
        assert_eq!(m.default_effort.as_deref(), Some("low"));
        assert!(!m.is_default);
    }

    /// Codex marks its internal rows hidden (`gpt-reserve`,
    /// `codex-auto-review` on 0.154.0) and its own picker does not show them.
    #[test]
    fn a_hidden_model_is_not_offered() {
        let mut entry = live_entry();
        entry["hidden"] = json!(true);
        assert!(CodexModel::parse(&entry).is_none());
    }

    #[test]
    fn the_default_model_is_carried_through() {
        let mut entry = live_entry();
        entry["isDefault"] = json!(true);
        assert!(CodexModel::parse(&entry).expect("model").is_default);
    }

    /// A tool that adds a field must not become a tool GitWyrm cannot read.
    /// An entry with nothing but an id still renders, under that id.
    #[test]
    fn a_sparse_entry_still_renders_rather_than_failing_the_whole_list() {
        let m = CodexModel::parse(&json!({ "id": "gpt-future" })).expect("model");
        assert_eq!(m.display_name, "gpt-future");
        assert!(m.description.is_empty());
        assert!(m.efforts.is_empty(), "no efforts means the control is not offered, not that it has none");
        assert!(!m.is_default);
    }

    /// An entry with no usable id cannot be sent to the tool, so it is dropped
    /// rather than shown as a choice that would fail at launch.
    #[test]
    fn an_entry_with_no_id_is_dropped() {
        assert!(CodexModel::parse(&json!({ "displayName": "Nameless" })).is_none());
        assert!(CodexModel::parse(&json!({ "id": "   " })).is_none());
    }

    /// Efforts are per model, not per tool: on 0.154.0 `gpt-5.5` accepts four
    /// levels while every other model accepts six. That is the fact the
    /// registry row had no way to record, which is why Codex's effort support
    /// was written down as none at all.
    #[test]
    fn two_models_can_offer_different_efforts() {
        let mut fewer = live_entry();
        fewer["id"] = json!("gpt-5.5");
        fewer["supportedReasoningEfforts"] = json!([{ "reasoningEffort": "low" }]);
        let a = CodexModel::parse(&live_entry()).expect("model");
        let b = CodexModel::parse(&fewer).expect("model");
        assert_ne!(a.efforts, b.efforts);
    }
}
