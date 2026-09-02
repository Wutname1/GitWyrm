//! Driving Claude Code through its own newline-JSON stream.
//!
//! Claude Code does not speak ACP. In print mode it can keep a conversation
//! open on stdin and emit one JSON object per line on stdout, so GitWyrm talks
//! to that stream directly and translates it into [`super::wire`]. The UI
//! remains a conversation and activity feed; no terminal is exposed.

use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

use super::transport::AgentError;
use super::wire::{Incoming, StopReason, TurnOutcome};

type Pending = Arc<Mutex<Option<oneshot::Sender<Result<TurnOutcome, AgentError>>>>>;
type Complaints = Arc<Mutex<std::collections::VecDeque<String>>>;
const COMPLAINT_LINES: usize = 8;

pub struct ClaudeConnection {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    incoming: Option<mpsc::UnboundedReceiver<Incoming>>,
    cancelling: Arc<AtomicBool>,
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
        tokio::spawn(read_loop(stdout, pending.clone(), tx, cancelling.clone(), complaints.clone()));
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

        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            incoming: Some(rx),
            cancelling,
        })
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

        done_rx.await.map_err(|_| AgentError::Failed {
            detail: "Claude Code stopped before finishing the reply".into(),
        })?
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
        self.child
            .lock()
            .await
            .start_kill()
            .map_err(|e| AgentError::Failed {
                detail: format!("could not stop Claude Code: {e}"),
            })
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

fn launch_args(base: &[String], read_only: bool) -> Vec<String> {
    let mut args = base.to_vec();
    args.push("--permission-mode".into());
    args.push(if read_only { "plan" } else { "acceptEdits" }.into());
    // Keeps file access inside the run's isolated working directory and asks
    // Claude Code itself to guard Git/settings/tool-configuration files.
    args.push("--restricted".into());
    // Agent Desk supplies the task, policy and OpenSpec context itself. Local
    // hooks, plugins and MCP servers would be an unreviewed second execution
    // path around those gates, so do not load them for a managed run.
    args.push("--safe-mode".into());
    args.push("--strict-mcp-config".into());
    args.push("--mcp-config".into());
    // A real, empty configuration -- not `{}`. The CLI validates the document
    // and refuses to start on `{}` with "mcpServers: expected record, received
    // undefined", which killed every Claude run on first launch.
    args.push(r#"{"mcpServers":{}}"#.into());
    args
}

async fn read_loop(
    stdout: tokio::process::ChildStdout,
    pending: Pending,
    tx: mpsc::UnboundedSender<Incoming>,
    cancelling: Arc<AtomicBool>,
    complaints: Complaints,
) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        for incoming in classify(&message) {
            if tx.send(incoming).is_err() {
                return;
            }
        }

        if message.get("type").and_then(Value::as_str) == Some("result") {
            if let Some(done) = pending.lock().await.take() {
                let _ = done.send(result_of(&message));
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
        assert!(read_only.contains(&"--safe-mode".to_string()));
        assert!(read_only.contains(&"--strict-mcp-config".to_string()));
        let writing = launch_args(&base, false);
        assert!(writing
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "acceptEdits"]));
    }
}
