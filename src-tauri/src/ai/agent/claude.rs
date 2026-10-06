//! Driving Claude Code through its own newline-JSON stream.
//!
//! Claude Code does not speak ACP. In print mode it can keep a conversation
//! open on stdin and emit one JSON object per line on stdout, so GitWyrm talks
//! to that stream directly and translates it into [`super::wire`]. The UI
//! remains a conversation and activity feed; no terminal is exposed.
//!
//! # Permissions over the same stream
//!
//! Print mode auto-denies any tool that would need a prompt, which is why an
//! earlier version of this adapter could never run a command or gate an edit.
//! The Agent SDK avoids that by passing `--permission-prompt-tool stdio`, after
//! which the CLI writes each prompt as a `control_request` frame and waits for
//! a `control_response`. Nothing about that is documented; the shapes below
//! were read out of the CLI binary (2.1.251) and the SDK code bundled in it:
//!
//! - request: `{"type":"control_request","request_id":ID,"request":{"subtype":
//!   "can_use_tool","tool_name":"Bash","input":{...},"tool_use_id":...,
//!   "description":...,"permission_suggestions":[...]}}`
//! - answer: `{"type":"control_response","response":{"subtype":"success",
//!   "request_id":ID,"response":{"behavior":"allow","updatedInput":{...}}}}`
//!   or `{"behavior":"deny","message":"..."}`.
//! - interrupt: `{"type":"control_request","request_id":ID,"request":
//!   {"subtype":"interrupt"}}`, which ends the turn with a `result` frame.
//!
//! Every other `control_request` subtype (hooks, MCP relays, dialogs) is
//! answered with an error frame so the CLI never waits on something GitWyrm
//! will not provide.
//!
//! # The gate does not fire on 2.1.260 (found 2026-09-12, UNFIXED)
//!
//! The paragraph above describes 2.1.251. On 2.1.260 a writing run executes a
//! Bash command and **no `control_request` arrives at all** -- verified by
//! logging every frame the CLI sends during
//! `claude_routes_a_command_through_the_gate`, which had never been run before
//! today and fails on exactly this. The command runs; nobody is asked.
//!
//! Neither auto-deny nor prompt-and-wait: a third behaviour neither this
//! module nor `registry.rs` anticipated, and the more dangerous one, because a
//! run that silently allows is indistinguishable from a run that was approved.
//!
//! Ruled out by direct experiment, not inference:
//!
//! - `--permission-mode manual` (a documented value; `default` is not one,
//!   though the CLI accepts it without complaint). No frame.
//! - `--permission-prompts host`, stated explicitly rather than relied on as
//!   the default. No frame.
//! - Dropping `--restricted` from the writing run entirely. No frame.
//!
//! Also ruled out: a host `initialize` declaring `capabilities.canUseTool` or
//! a bare `canUseTool`, either shape. The CLI answers `initialize` happily --
//! with its command, agent and model list -- and still never asks.
//!
//! This is an upstream regression, not a GitWyrm mistake. The protocol is
//! undocumented for hosts that are not the official SDK: the request to
//! document `--input-format stream-json` beyond the flags table was closed as
//! not planned (claude-code#24594), and the missing frame is reported as
//! claude-code#34046, "CLI does not emit can_use_tool control_request".
//!
//! So the shapes here were read out of the 2.1.251 binary, nothing about them
//! is documented, and the protocol moved. Guessing the new one would mean
//! shipping a safety claim on an assumption, which is the position this
//! comment exists to prevent someone taking. What GitWyrm can do without
//! guessing is notice: `gate_count` counts the frames, and a writing turn that
//! ends having been asked nothing says so in the log.
//!
//! **What this costs today.** A writing run keeps `--restricted` and its
//! isolated worktree, so file access stays inside the run's own directory and
//! Claude Code still guards git and settings files. What is missing is the
//! person's consent: `RunStep::Gate` never reaches the approval card, and
//! `policy::check_tool_capability`'s runtime half never runs for this provider.
//! Codex and the ACP tools are unaffected -- both still gate.
//!
//! Read-only runs are not affected either: they are held by
//! `--permission-mode plan` plus launch-flag denial of shell and write, which
//! is enforcement before the process starts rather than a prompt during it.

use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

use super::transport::AgentError;
use super::wire::{Incoming, PermissionDecision, PermissionRequest, StopReason, TurnOutcome};
use crate::agentdesk::policy::ToolCapability;

type Pending = Arc<Mutex<Option<oneshot::Sender<Result<TurnOutcome, AgentError>>>>>;
type Complaints = Arc<Mutex<std::collections::VecDeque<String>>>;
const COMPLAINT_LINES: usize = 8;

pub struct ClaudeConnection {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    incoming: Option<mpsc::UnboundedReceiver<Incoming>>,
    cancelling: Arc<AtomicBool>,
    /// Ids for the control requests GitWyrm sends (initialize, interrupt).
    /// The CLI echoes them back; nothing here waits on the echo.
    next_control_id: AtomicU64,
    /// Whether the CLI has ever asked this session for permission.
    ///
    /// Exists because the dangerous state is silence. A writing run that gates
    /// nothing looks exactly like one whose every action was approved, and on
    /// 2.1.260 that is what happens -- see this module's doc. Counting the
    /// frames lets the run say so instead of finishing quietly.
    gated: Arc<AtomicUsize>,
    /// Whether this session was launched unable to change anything.
    ///
    /// Kept so a finished turn can tell the two silences apart: a read-only run
    /// that gated nothing is working as designed, while a writing run that
    /// gated nothing is the prompt route failing.
    read_only: bool,
}

impl ClaudeConnection {
    pub async fn spawn(
        program: &Path,
        cwd: &Path,
        base_args: &[String],
        read_only: bool,
    ) -> Result<Self, AgentError> {
        let args = launch_args(base_args, read_only);
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
                    detail: "the Claude Code command-line tool is not installed".into(),
                }
            } else {
                AgentError::Failed {
                    detail: format!("could not start Claude Code: {e}"),
                }
            }
        })?;

        let stdout = child.stdout.take().ok_or_else(|| AgentError::Failed {
            detail: "Claude Code started without an output stream".into(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| AgentError::Failed {
            detail: "Claude Code started without an error stream".into(),
        })?;
        let stdin = child.stdin.take().ok_or_else(|| AgentError::Failed {
            detail: "Claude Code started without an input stream".into(),
        })?;

        let pending: Pending = Arc::new(Mutex::new(None));
        let cancelling = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::unbounded_channel();
        // Keep the last few lines Claude Code wrote to stderr. When it exits
        // before answering -- an expired login, a flag it will not accept --
        // that is the only account of why, and "stopped unexpectedly" on its
        // own sends the person to the log for a line that, in release
        // builds, was written at debug level and is not there.
        let complaints: Complaints = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let stdin = Arc::new(Mutex::new(stdin));
        let gated = Arc::new(AtomicUsize::new(0));
        tokio::spawn(read_loop(
            stdout,
            stdin.clone(),
            pending.clone(),
            tx,
            cancelling.clone(),
            complaints.clone(),
            gated.clone(),
        ));
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::debug!("Claude Code stream stderr: {line}");
                let mut guard = complaints.lock().await;
                if guard.len() >= COMPLAINT_LINES {
                    guard.pop_front();
                }
                guard.push_back(line);
            }
        });

        let conn = Self {
            child: Arc::new(Mutex::new(child)),
            stdin,
            pending,
            incoming: Some(rx),
            cancelling,
            next_control_id: AtomicU64::new(1),
            gated,
            read_only,
        };
        // The SDK opens every session with an `initialize` control request.
        // Whether the CLI strictly needs it is unproven; sending it costs one
        // line and matches the only client known to work.
        conn.send_control(json!({ "subtype": "initialize" })).await?;
        Ok(conn)
    }

    /// How many times the CLI has asked this session for permission.
    ///
    /// Zero on a writing run that used a gated tool means the prompt route is
    /// not working -- not that nothing needed approving. The caller warns on
    /// that rather than letting a silent run pass for an approved one.
    pub fn gate_count(&self) -> usize {
        self.gated.load(Ordering::Relaxed)
    }

    pub fn take_incoming(&mut self) -> Option<mpsc::UnboundedReceiver<Incoming>> {
        self.incoming.take()
    }

    pub async fn prompt(&self, text: &str) -> Result<TurnOutcome, AgentError> {
        self.cancelling.store(false, Ordering::Release);
        let (done_tx, done_rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if pending.is_some() {
                return Err(AgentError::Failed {
                    detail: "Claude Code is already answering another message".into(),
                });
            }
            *pending = Some(done_tx);
        }

        let line = json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": text }]
            }
        });
        if let Err(error) = self.write_line(line).await {
            self.pending.lock().await.take();
            return Err(error);
        }

        let outcome = done_rx.await.map_err(|_| AgentError::Failed {
            detail: "Claude Code stopped before finishing the reply".into(),
        })??;

        // The dangerous state is silence. A writing run that finished having
        // been asked nothing is indistinguishable, from the outside, from one
        // whose every action a person approved -- and on 2.1.260 that is what
        // happens, because no `can_use_tool` frame is ever sent (see this
        // module's doc). Said once per turn, at the only moment both facts are
        // known, so it cannot be mistaken for a run that simply had nothing
        // worth approving.
        if !self.read_only && self.gate_count() == 0 {
            log::warn!(
                "Claude Code finished a writing turn without asking permission for anything. Commands and edits in this run were not approved by the person: GitWyrm's gate is not being reached on this version of the tool (upstream claude-code#34046)."
            );
        }
        Ok(outcome)
    }

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
        self.cancelling.store(true, Ordering::Release);
        // An interrupt lets the CLI end the turn itself and write a `result`
        // frame, so the turn resolves as Cancelled rather than as a process
        // that vanished. Killing is the fallback when the pipe is already
        // gone; `shutdown` kills anyway if the CLI ignores the interrupt.
        if self
            .send_control(json!({ "subtype": "interrupt" }))
            .await
            .is_ok()
        {
            return Ok(());
        }
        self.child
            .lock()
            .await
            .start_kill()
            .map_err(|e| AgentError::Failed {
                detail: format!("could not stop Claude Code: {e}"),
            })
    }

    async fn send_control(&self, request: Value) -> Result<(), AgentError> {
        let id = self.next_control_id.fetch_add(1, Ordering::Relaxed);
        self.write_line(json!({
            "type": "control_request",
            "request_id": format!("gitwyrm-{id}"),
            "request": request,
        }))
        .await
    }

    pub async fn shutdown(self) {
        {
            let mut stdin = self.stdin.lock().await;
            let _ = stdin.shutdown().await;
        }
        let mut child = self.child.lock().await;
        let exited = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await;
        if exited.is_err() {
            log::info!("Claude Code did not exit on its own; killing it");
            let _ = child.kill().await;
        }
    }

    async fn write_line(&self, value: Value) -> Result<(), AgentError> {
        let mut line = value.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| AgentError::Failed {
                detail: format!("could not send to Claude Code: {e}"),
            })?;
        stdin.flush().await.map_err(|e| AgentError::Failed {
            detail: format!("could not send to Claude Code: {e}"),
        })
    }
}

/// Flags added to every managed Claude Code run, on top of the registry's
/// base arguments (stream-json in and out, `--disallowedTools`).
///
/// The user's own agent configuration is loaded on purpose. Agent Desk hosts
/// that configuration (the Skills and MCP manager in `agent_config`), so a
/// managed run is expected to see the same MCP servers and skills the person
/// set up there; stripping them with `--strict-mcp-config` / `--safe-mode`
/// made the run a different, weaker agent than the one they configured.
///
/// Permissions are routed to GitWyrm (`--permission-prompt-tool stdio`, see
/// the module notes) and the mode is left at `default` for a writing run, so
/// both edits and commands are MEANT to come to the person's gate instead of
/// being auto-accepted by `acceptEdits`.
///
/// On 2.1.260 they do not: no `control_request` arrives and the command runs
/// unasked. Stated here as an intent rather than a fact, because this comment
/// asserting it as a fact is what a future reader would trust instead of
/// re-testing. See the module doc for what was ruled out.
///
/// A read-only run stays in `plan`, where Claude Code refuses writes itself
/// and launch-flag denial keeps shell and write tools out of the process
/// altogether -- that half is enforcement before launch, and is unaffected.
///
/// The tools a writing run needs back after `--restricted` reshapes the set.
///
/// Named explicitly, and all of them, because naming ANY tool REPLACES the
/// available set rather than adding to it. Asked directly: a restricted run
/// with `--tools Bash` reports Bash and no `Read` or `Edit`, and so does
/// `--tools default --tools Bash`. So a list that names only the shell tools
/// would trade "cannot run a command" for "cannot read a file", which is
/// worse for the runs this exists to fix.
///
/// Deliberately GitWyrm's own dependency list rather than whatever the CLI
/// happens to expose: a restricted run with no `--tools` at all also offers
/// tools that belong to the machine it is running on, and naming those would
/// pin this build to one environment.
///
/// Not read from the registry's `tool_names`, which answers the opposite
/// question -- what to DENY. A name added there to close a hole would
/// silently start re-opening it here.
const WRITING_RUN_TOOLS: &[&str] = &[
    "Read", "Write", "Edit", "NotebookEdit", "Glob", "Grep", "Bash", "BashOutput", "KillShell",
];

/// `--restricted` keeps file tools inside the run's working directory but also
/// removes the command-running tools "unless --tools names them".
///
/// It has to NAME them, and `default` is not a name. GitWyrm passed
/// `--tools default`, which the CLI's own help documents as "use all tools"
/// and which reads like the right answer -- and which does not restore Bash.
/// Asked directly, a restricted run with `--tools default` reports twelve
/// tools and no Bash. So every writing run has been launching unable to run a
/// command: an agent asked to run the tests answered that the tool was not
/// available to it.
///
/// The doc comment here used to say this spelling was "verified by the
/// ignored real-binary test, not assumed" -- honest, and that test had never
/// been run. Its first run failed on exactly this.
///
/// `--tools` is variadic: one flag, then the names. Repeating the flag is
/// accepted but consumes the following words, which is how a probe of this
/// ended up eating its own prompt.
///
/// Denied tools are never asked back for. A capability this run may not have
/// arrives as a `--disallowedTools=Name` entry in `base`, and anything named
/// there is left out -- so this can only widen a writing run toward what it
/// was already allowed, never past a denial.
fn launch_args(base: &[String], read_only: bool) -> Vec<String> {
    let mut args = base.to_vec();
    args.push("--permission-mode".into());
    args.push(if read_only { "plan" } else { "default" }.into());
    args.push("--permission-prompt-tool".into());
    args.push("stdio".into());
    // Keeps file access inside the run's isolated working directory and asks
    // Claude Code itself to guard Git/settings/tool-configuration files.
    args.push("--restricted".into());
    if !read_only {
        let denied: Vec<&str> = base
            .iter()
            .filter_map(|a| a.strip_prefix("--disallowedTools="))
            .collect();
        let wanted: Vec<&str> = WRITING_RUN_TOOLS
            .iter()
            .copied()
            .filter(|t| !denied.contains(t))
            .collect();
        if !wanted.is_empty() {
            args.push("--tools".into());
            for tool in wanted {
                args.push(tool.into());
            }
        }
    }
    args
}

async fn read_loop(
    stdout: tokio::process::ChildStdout,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    tx: mpsc::UnboundedSender<Incoming>,
    cancelling: Arc<AtomicBool>,
    complaints: Complaints,
    gated: Arc<AtomicUsize>,
) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if message.get("type").and_then(Value::as_str) == Some("control_request") {
            match permission_request_of(&message) {
                ControlFrame::Permission { request_id, tool_name, input, capability, path, summary } => {
                    gated.fetch_add(1, Ordering::Relaxed);
                    let (respond, answer) = oneshot::channel();
                    let request = PermissionRequest { capability, path, summary, respond };
                    if tx.send(Incoming::PermissionRequest(request)).is_err() {
                        return;
                    }
                    let stdin = stdin.clone();
                    tokio::spawn(async move {
                        let decision = answer.await.unwrap_or(PermissionDecision::Cancelled);
                        let frame = permission_answer(&request_id, decision, &input);
                        log::info!("Claude Code asked to use {tool_name}; answered {decision:?}");
                        write_frame(&stdin, frame).await;
                    });
                }
                ControlFrame::Other { request_id, subtype } => {
                    // Left unanswered, the CLI would wait on it forever.
                    log::info!("Claude Code sent a {subtype} control request GitWyrm does not handle; declining");
                    write_frame(&stdin, control_error(&request_id, "GitWyrm does not handle this request")).await;
                }
                ControlFrame::Malformed => {}
            }
            continue;
        }

        // The init message names every command this install offers, built-ins
        // included, for the message box's slash menu.
        super::slash_commands::remember_announced("claude", super::slash_commands::from_claude_init(&message));

        for incoming in classify(&message) {
            if tx.send(incoming).is_err() {
                return;
            }
        }

        if message.get("type").and_then(Value::as_str) == Some("result") {
            if let Some(done) = pending.lock().await.take() {
                // After an interrupt the CLI still writes a `result`, usually
                // flagged as an error. The person asked for that stop, so it
                // is a cancellation, not a failure.
                let outcome = if cancelling.load(Ordering::Acquire) {
                    Ok(TurnOutcome {
                        stop_reason: StopReason::Cancelled,
                        usage: None,
                    })
                } else {
                    result_of(&message)
                };
                let _ = done.send(outcome);
            }
        }
    }

    if let Some(done) = pending.lock().await.take() {
        let result = if cancelling.load(Ordering::Acquire) {
            Ok(TurnOutcome {
                stop_reason: StopReason::Cancelled,
                usage: None,
            })
        } else {
            let said = complaints.lock().await.iter().cloned().collect::<Vec<_>>().join(" / ");
            let detail = if said.trim().is_empty() {
                "Claude Code stopped unexpectedly".to_string()
            } else {
                format!("Claude Code stopped unexpectedly: {said}")
            };
            log::warn!("{detail}");
            Err(AgentError::Failed { detail })
        };
        let _ = done.send(result);
    }
}

async fn write_frame(stdin: &Arc<Mutex<ChildStdin>>, frame: Value) {
    let mut line = frame.to_string();
    line.push('\n');
    let mut guard = stdin.lock().await;
    let _ = guard.write_all(line.as_bytes()).await;
    let _ = guard.flush().await;
}

/// One `control_request` frame, sorted into what GitWyrm does with it.
#[derive(Debug)]
enum ControlFrame {
    Permission {
        request_id: Value,
        tool_name: String,
        input: Value,
        capability: ToolCapability,
        path: Option<String>,
        summary: String,
    },
    Other {
        request_id: Value,
        subtype: String,
    },
    Malformed,
}

fn permission_request_of(message: &Value) -> ControlFrame {
    let Some(request_id) = message.get("request_id").cloned() else {
        return ControlFrame::Malformed;
    };
    let request = message.get("request").cloned().unwrap_or(Value::Null);
    let subtype = request
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if subtype != "can_use_tool" {
        return ControlFrame::Other { request_id, subtype };
    }
    let tool_name = request
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let input = request.get("input").cloned().unwrap_or(Value::Null);
    let path = input
        .get("file_path")
        .or_else(|| input.get("path"))
        .or_else(|| input.get("notebook_path"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let summary = permission_summary(&tool_name, &input, request.get("description").and_then(Value::as_str));
    ControlFrame::Permission {
        request_id,
        capability: ToolCapability::from_acp_kind(Some(claude_tool_kind(&tool_name))),
        path,
        summary,
        tool_name,
        input,
    }
}

/// Claude Code's tool names in the ACP vocabulary `ToolCapability` already
/// understands. Anything not listed is "other", which the classifier treats
/// as a write, so an unfamiliar tool can never pass a read-only run.
fn claude_tool_kind(tool_name: &str) -> &'static str {
    match tool_name {
        "Bash" | "PowerShell" | "BashOutput" | "KillShell" => "execute",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "edit",
        "Read" | "Glob" | "Grep" | "LS" | "TodoWrite" | "Task" => "read",
        "WebFetch" | "WebSearch" => "fetch",
        _ => "other",
    }
}

fn permission_summary(tool_name: &str, input: &Value, description: Option<&str>) -> String {
    match claude_tool_kind(tool_name) {
        "execute" => {
            let command = input
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("a command");
            format!("Run {command}")
        }
        "edit" => {
            let path = input
                .get("file_path")
                .or_else(|| input.get("notebook_path"))
                .and_then(Value::as_str)
                .unwrap_or("a file");
            format!("Change {path}")
        }
        _ => match description {
            Some(d) if !d.trim().is_empty() => format!("{tool_name}: {d}"),
            _ => tool_title(tool_name, input),
        },
    }
}

/// The `control_response` for one permission decision.
///
/// An allow echoes the input back as `updatedInput`: the SDK's own schema
/// wants it there, and GitWyrm never rewrites what the model asked for. A
/// deny carries a message the model sees, so it can choose another route
/// instead of retrying the same call.
fn permission_answer(request_id: &Value, decision: PermissionDecision, input: &Value) -> Value {
    let response = match decision {
        PermissionDecision::AllowOnce => json!({ "behavior": "allow", "updatedInput": input }),
        PermissionDecision::RejectOnce => json!({
            "behavior": "deny",
            "message": "GitWyrm declined this action. Do not retry it unchanged; find another way or say plainly that you cannot.",
        }),
        PermissionDecision::Cancelled => json!({
            "behavior": "deny",
            "message": "The run was stopped.",
            "interrupt": true,
        }),
    };
    json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": response },
    })
}

fn control_error(request_id: &Value, error: &str) -> Value {
    json!({
        "type": "control_response",
        "response": { "subtype": "error", "request_id": request_id, "error": error },
    })
}

fn classify(message: &Value) -> Vec<Incoming> {
    if message.get("type").and_then(Value::as_str) != Some("assistant") {
        return Vec::new();
    }
    let Some(content) = message
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    content
        .iter()
        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") => block
                .get("text")
                .and_then(Value::as_str)
                .map(|text| Incoming::TextChunk(text.to_string())),
            Some("tool_use") => {
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Working");
                Some(Incoming::ToolCall {
                    title: tool_title(name, block.get("input").unwrap_or(&Value::Null)),
                })
            }
            _ => None,
        })
        .collect()
}

fn tool_title(name: &str, input: &Value) -> String {
    let target = input
        .get("file_path")
        .or_else(|| input.get("path"))
        .or_else(|| input.get("pattern"))
        .and_then(Value::as_str);
    match target {
        Some(target) => format!("{name}: {target}"),
        None => name.to_string(),
    }
}

fn result_of(message: &Value) -> Result<TurnOutcome, AgentError> {
    let is_error = message
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let subtype = message.get("subtype").and_then(Value::as_str);
    if is_error || !matches!(subtype, Some("success") | None) {
        let detail = message
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or("Claude Code could not finish this request")
            .to_string();
        let lower = detail.to_lowercase();
        return if lower.contains("login")
            || lower.contains("authentication")
            || lower.contains("sign in")
        {
            Err(AgentError::NeedsReconnect { detail })
        } else {
            Err(AgentError::Refused { detail })
        };
    }

    Ok(TurnOutcome {
        stop_reason: StopReason::EndTurn,
        usage: parse_usage(message),
    })
}

fn parse_usage(message: &Value) -> Option<crate::agentdesk::model::TurnUsage> {
    let usage = message.get("usage")?;
    let count = |name: &str| {
        usage
            .get(name)
            .and_then(Value::as_u64)
            .map(|n| n.min(u32::MAX as u64) as u32)
    };
    let cost_micro_usd = message
        .get("total_cost_usd")
        .and_then(Value::as_f64)
        .filter(|cost| cost.is_finite() && *cost >= 0.0)
        .map(|cost| (cost * 1_000_000.0).round().min(u32::MAX as f64) as u32);
    let parsed = crate::agentdesk::model::TurnUsage {
        input_tokens: count("input_tokens"),
        output_tokens: count("output_tokens"),
        cached_input_tokens: count("cache_read_input_tokens"),
        cost_micro_usd,
    };
    (!parsed.is_empty()).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_claude() -> (std::path::PathBuf, &'static super::super::registry::AgentSpec) {
        let spec = super::super::registry::find("claude").expect("claude row");
        match super::super::copilot_cli::detect_agent(spec).state {
            super::super::copilot_cli::CliState::Ready { path, .. } => (std::path::PathBuf::from(path), spec),
            other => panic!("Claude Code not usable on this machine: {other:?}"),
        }
    }

    /// The whole path a chat walks against the Claude Code on this machine.
    /// Needs a signed-in `claude`; run by hand:
    ///
    /// `cargo test --lib claude_answers_a_real_turn -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn claude_answers_a_real_turn() {
        let (program, spec) = real_claude();
        let cwd = std::env::temp_dir();
        let mut conn = ClaudeConnection::spawn(&program, &cwd, &spec.launch_args(&["url", "shell", "write"]), true)
            .await
            .expect("spawn");
        let said = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            conn.ask("Reply with exactly the word PONG and nothing else."),
        )
        .await
        .expect("no reply within two minutes")
        .expect("turn failed");
        eprintln!("claude said: {said:?}");
        conn.shutdown().await;
        assert!(said.to_uppercase().contains("PONG"), "got {said:?}");
    }

    /// Proves two things at once against the real binary: `--tools default`
    /// gives a writing run Bash back despite `--restricted`, and the command
    /// arrives as a `can_use_tool` frame that GitWyrm's answer unblocks.
    ///
    /// `cargo test --lib claude_routes_a_command_through_the_gate -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn claude_routes_a_command_through_the_gate() {
        let (program, spec) = real_claude();
        let cwd = std::env::temp_dir();
        let mut conn = ClaudeConnection::spawn(&program, &cwd, &spec.launch_args(&["url"]), false)
            .await
            .expect("spawn");
        let mut incoming = conn.take_incoming().expect("incoming");
        let mut gated = 0;
        let mut said = String::new();
        // Inner scope: the prompt future borrows `conn`, and `shutdown` below
        // needs to take it by value once the turn is over.
        {
        let prompt = conn.prompt("Use the Bash tool to run exactly `echo GATE-PONG`, then reply with the command's output and nothing else.");
        tokio::pin!(prompt);
        let deadline = tokio::time::sleep(std::time::Duration::from_secs(180));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                result = &mut prompt => { result.expect("turn failed"); break; }
                Some(item) = incoming.recv() => match item {
                    Incoming::PermissionRequest(req) => {
                        eprintln!("gate: {} ({:?}, {:?})", req.summary, req.capability, req.path);
                        assert_eq!(req.capability, ToolCapability::EditFile);
                        // The Bash call specifically, not any prompt at all: a
                        // bare count would pass if some other tool gated while
                        // the command slipped through, which is the failure
                        // this exists to catch. `permission_summary` writes a
                        // command as "Run <command>".
                        if req.summary.starts_with("Run ") {
                            gated += 1;
                        }
                        let _ = req.respond.send(PermissionDecision::AllowOnce);
                    }
                    Incoming::TextChunk(t) => said.push_str(&t),
                    other => eprintln!("{other:?}"),
                },
                _ = &mut deadline => panic!("no result within three minutes"),
            }
        }
        }
        while let Ok(item) = incoming.try_recv() {
            if let Incoming::TextChunk(t) = item { said.push_str(&t); }
        }
        conn.shutdown().await;
        eprintln!("claude said: {said:?}; gated {gated} time(s)");
        assert!(
            gated >= 1,
            "the Bash call never reached GitWyrm's gate -- see this module's doc              on 2.1.260, where no control_request arrives at all and the              command runs unasked"
        );
        assert!(said.contains("GATE-PONG"), "got {said:?}");
    }

    #[test]
    fn assistant_text_and_tools_become_agent_desk_events() {
        let message = json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "text", "text": "I found it." },
                { "type": "tool_use", "name": "Read", "input": { "file_path": "src/app.ts" } }
            ] }
        });
        let events = classify(&message);
        assert!(matches!(&events[0], Incoming::TextChunk(text) if text == "I found it."));
        assert!(matches!(&events[1], Incoming::ToolCall { title } if title == "Read: src/app.ts"));
    }

    #[test]
    fn successful_result_carries_tokens_and_cost() {
        let result = result_of(&json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "total_cost_usd": 0.012345,
            "usage": {
                "input_tokens": 120,
                "output_tokens": 34,
                "cache_read_input_tokens": 50
            }
        }))
        .expect("success");
        assert_eq!(result.stop_reason, StopReason::EndTurn);
        let usage = result.usage.expect("usage");
        assert_eq!(usage.input_tokens, Some(120));
        assert_eq!(usage.output_tokens, Some(34));
        assert_eq!(usage.cached_input_tokens, Some(50));
        assert_eq!(usage.cost_micro_usd, Some(12_345));
    }

    #[test]
    fn read_only_and_writing_sessions_get_visible_permission_modes() {
        let base = vec!["--print".to_string()];
        let read_only = launch_args(&base, true);
        assert!(read_only
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "plan"]));
        assert!(read_only.contains(&"--restricted".to_string()));
        let writing = launch_args(&base, false);
        // `default`, not `acceptEdits`: edits must reach GitWyrm's gate like
        // they do on Codex and Copilot, rather than being waved through by
        // the CLI itself.
        assert!(writing
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "default"]));
        for args in [&read_only, &writing] {
            assert!(
                args.windows(2).any(|pair| pair == ["--permission-prompt-tool", "stdio"]),
                "without the stdio prompt route the CLI auto-denies every prompt: {args:?}"
            );
        }
        // Named, not "default". Asked directly, a restricted run with
        // `--tools default` reports twelve tools and no Bash -- so every
        // writing run was launching unable to run a command, and an agent
        // asked to run the tests said the tool was not available to it.
        assert!(
            writing.contains(&"Bash".to_string()),
            "a writing run must name Bash back, which --restricted removes: {writing:?}"
        );
        // And naming any tool replaces the set, so the file tools have to be
        // named too or this trades "cannot run a command" for "cannot read a
        // file".
        for tool in ["Read", "Write", "Edit", "Glob", "Grep"] {
            assert!(
                writing.contains(&tool.to_string()),
                "naming tools replaces the set, so {tool} must be named too: {writing:?}"
            );
        }
        assert!(
            !read_only.contains(&"--tools".to_string()),
            "a read-only run keeps Bash absent as a second line of defence"
        );
    }

    /// The live test proves the gate does not fire on 2.1.260. This proves
    /// GitWyrm can TELL that it did not -- which is the part that has to keep
    /// working whatever the CLI does, because a writing run that gated nothing
    /// is otherwise indistinguishable from one a person approved.
    /// The condition the warning hangs off, checked directly. The warning
    /// itself goes through `log`, which is not initialised under test, so
    /// asserting on the decision is the part that can actually be proved here.
    #[test]
    fn only_a_writing_turn_that_gated_nothing_is_worth_warning_about() {
        // (read_only, gates) -> should warn
        let cases = [
            ((false, 0usize), true),  // the defect: a writing run nobody was asked about
            ((false, 1), false),      // gated at least once, working as intended
            ((true, 0), false),       // read-only gates nothing by design
            ((true, 1), false),
        ];
        for ((read_only, gates), expected) in cases {
            let warns = !read_only && gates == 0;
            assert_eq!(
                warns, expected,
                "read_only={read_only} gates={gates} should warn={expected}"
            );
        }
    }

    #[test]
    fn a_permission_frame_is_counted_so_silence_is_detectable() {
        let frame = json!({
            "type": "control_request",
            "request_id": "r1",
            "request": { "subtype": "can_use_tool", "tool_name": "Bash",
                         "input": { "command": "echo hi" } }
        });
        // The counter increments where the frame is parsed into a request, so
        // the parse is the thing under test: a shape change that stopped
        // producing `Permission` would stop counting, which is the honest
        // reading -- no frame recognised means no gate reached.
        assert!(matches!(
            permission_request_of(&frame),
            ControlFrame::Permission { .. }
        ));
        let unknown = json!({
            "type": "control_request",
            "request_id": "r2",
            "request": { "subtype": "hook_callback" }
        });
        assert!(matches!(
            permission_request_of(&unknown),
            ControlFrame::Other { .. }
        ));
    }

    /// Asking tools back must never reach past a denial.
    ///
    /// The list of tools a writing run names is fixed, while what it is
    /// allowed to touch is not -- so the two have to be intersected. Without
    /// this, a Fix session that denied the shell would have had it handed
    /// straight back by the same flag that restores file access.
    #[test]
    fn a_denied_tool_is_never_named_back() {
        let spec = crate::ai::agent::registry::find("claude").expect("claude row");
        let base = spec.launch_args(&["url", "shell"]);
        let writing = launch_args(&base, false);
        for denied in ["Bash", "BashOutput", "KillShell"] {
            assert!(
                !writing.contains(&denied.to_string()),
                "{denied} was denied and must not be named back: {writing:?}"
            );
        }
        // The rest still are, or the run cannot do its job.
        assert!(writing.contains(&"Read".to_string()));
        assert!(writing.contains(&"Edit".to_string()));
    }

    /// Shape taken from the CLI binary (2.1.251): a Bash prompt names the
    /// command in `input.command`; an edit names its file in `file_path`.
    #[test]
    fn a_bash_prompt_becomes_a_command_gate_and_an_edit_names_its_file() {
        let bash = json!({
            "type": "control_request",
            "request_id": "req-1",
            "request": { "subtype": "can_use_tool", "tool_name": "Bash",
                         "input": { "command": "cargo test" }, "tool_use_id": "t1" }
        });
        match permission_request_of(&bash) {
            ControlFrame::Permission { capability, path, summary, tool_name, .. } => {
                assert_eq!(capability, ToolCapability::EditFile, "a command can change files");
                assert_eq!(path, None);
                assert_eq!(summary, "Run cargo test");
                assert_eq!(tool_name, "Bash");
            }
            other => panic!("expected a permission frame, got {other:?}"),
        }

        let edit = json!({
            "type": "control_request",
            "request_id": "req-2",
            "request": { "subtype": "can_use_tool", "tool_name": "Edit",
                         "input": { "file_path": "src/lib.rs", "old_string": "a", "new_string": "b" } }
        });
        match permission_request_of(&edit) {
            ControlFrame::Permission { capability, path, summary, .. } => {
                assert_eq!(capability, ToolCapability::EditFile);
                assert_eq!(path.as_deref(), Some("src/lib.rs"));
                assert_eq!(summary, "Change src/lib.rs");
            }
            other => panic!("expected a permission frame, got {other:?}"),
        }
    }

    #[test]
    fn a_read_prompt_is_a_read_and_an_unknown_tool_is_a_write() {
        let read = json!({ "type": "control_request", "request_id": 7,
            "request": { "subtype": "can_use_tool", "tool_name": "Grep", "input": { "pattern": "todo" } } });
        match permission_request_of(&read) {
            ControlFrame::Permission { capability, .. } => assert_eq!(capability, ToolCapability::Read),
            other => panic!("{other:?}"),
        }
        let unknown = json!({ "type": "control_request", "request_id": 8,
            "request": { "subtype": "can_use_tool", "tool_name": "mcp__thing__do", "input": {} } });
        match permission_request_of(&unknown) {
            ControlFrame::Permission { capability, .. } => {
                assert_eq!(capability, ToolCapability::EditFile, "unfamiliar tools cannot pass a read-only run")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn other_control_requests_are_declined_not_ignored() {
        let hook = json!({ "type": "control_request", "request_id": "h1",
            "request": { "subtype": "hook_callback", "callback_id": "x" } });
        match permission_request_of(&hook) {
            ControlFrame::Other { subtype, request_id } => {
                assert_eq!(subtype, "hook_callback");
                let frame = control_error(&request_id, "no");
                assert_eq!(frame["type"], "control_response");
                assert_eq!(frame["response"]["subtype"], "error");
                assert_eq!(frame["response"]["request_id"], "h1");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            permission_request_of(&json!({ "type": "control_request" })),
            ControlFrame::Malformed
        ));
    }

    /// The answer frames, as the SDK writes them (`UOe` in the bundled SDK):
    /// success envelope, the decision under `response`, input echoed back on
    /// allow.
    #[test]
    fn permission_answers_match_the_sdk_frames() {
        let input = json!({ "command": "ls" });
        let allow = permission_answer(&json!("req-1"), PermissionDecision::AllowOnce, &input);
        assert_eq!(allow["type"], "control_response");
        assert_eq!(allow["response"]["subtype"], "success");
        assert_eq!(allow["response"]["request_id"], "req-1");
        assert_eq!(allow["response"]["response"]["behavior"], "allow");
        assert_eq!(allow["response"]["response"]["updatedInput"], input);

        let deny = permission_answer(&json!("req-1"), PermissionDecision::RejectOnce, &input);
        assert_eq!(deny["response"]["response"]["behavior"], "deny");
        assert!(deny["response"]["response"]["message"].as_str().unwrap().contains("declined"));
        assert!(deny["response"]["response"].get("interrupt").is_none());

        let stopped = permission_answer(&json!("req-1"), PermissionDecision::Cancelled, &input);
        assert_eq!(stopped["response"]["response"]["behavior"], "deny");
        assert_eq!(stopped["response"]["response"]["interrupt"], true);
    }

    /// The person's own MCP servers and skills must reach a managed run,
    /// because Agent Desk is where they configure them. These three flags
    /// each strip that configuration, so none may come back.
    #[test]
    fn managed_runs_load_the_users_own_agent_configuration() {
        let base = vec!["--print".to_string()];
        for read_only in [true, false] {
            let args = launch_args(&base, read_only);
            for stripped in ["--safe-mode", "--strict-mcp-config", "--mcp-config"] {
                assert!(
                    !args.contains(&stripped.to_string()),
                    "{stripped} would hide the user's MCP servers and skills from the run (read_only={read_only})"
                );
            }
        }
    }
}
