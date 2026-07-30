//! The API transports: a [`ProviderAgent`] over a documented HTTP API.
//!
//! Covers both remaining transports, because they differ only in where the
//! requests go. A provider from the catalog with the user's own key is
//! [`Transport::ApiKey`]; the same code pointed at a local opencode server,
//! Ollama, or LM Studio is [`Transport::OpenAiCompatible`]. Neither gets
//! special-cased in the loop.
//!
//! Separate from [`crate::ai::client`], which does one-shot completions for
//! commit messages and has no notion of tools. A turn here carries tool
//! schemas out and tool calls back, which is a different request shape in both
//! dialects.

use serde_json::{json, Value};

use super::transport::{
  AgentError, AgentTurn, Message, ProviderAgent, ToolCall, ToolSchema, Transport, TurnRequest,
};
use crate::ai::catalog::{CatalogProvider, Dialect};

/// A turn is a full model round-trip, measured at 10-20 seconds for a
/// realistic diff. Generous enough for a slow model, short enough that a hung
/// request does not look like a working one.
const TURN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

pub struct ApiAgent {
  provider: CatalogProvider,
  bearer: String,
  transport: Transport,
}

impl ApiAgent {
  /// A catalog provider reached with the user's own API key.
  pub fn with_key(provider: CatalogProvider, bearer: String) -> Self {
    Self { provider, bearer, transport: Transport::ApiKey }
  }

  /// Any endpoint speaking the OpenAI dialect.
  ///
  /// Same code path as [`Self::with_key`]; the distinct transport value exists
  /// so a failure can say "the endpoint you configured" rather than naming a
  /// provider the user never chose. Local servers often want no credential, so
  /// `bearer` may be empty.
  pub fn openai_compatible(base_url: String, bearer: String, model_id: String) -> Self {
    let provider = CatalogProvider {
      id: "openai-compatible".into(),
      name: "your endpoint".into(),
      base_url,
      dialect: Dialect::OpenAi,
      models: vec![crate::ai::catalog::CatalogModel {
        id: model_id.clone(),
        name: model_id,
        enabled: true,
      }],
    };
    Self { provider, bearer, transport: Transport::OpenAiCompatible }
  }
}

impl ProviderAgent for ApiAgent {
  fn transport(&self) -> Transport {
    self.transport
  }

  async fn check(&self) -> Result<(), AgentError> {
    // A local endpoint legitimately has no key; a hosted provider without one
    // will only fail later, at the first turn, with a less useful message.
    if self.bearer.trim().is_empty() && self.transport == Transport::ApiKey {
      return Err(AgentError::NeedsReconnect {
        detail: format!("{} needs an API key", self.provider.name),
      });
    }
    Ok(())
  }

  async fn turn(&self, req: TurnRequest<'_>) -> Result<AgentTurn, AgentError> {
    let base = self.provider.base_url.trim_end_matches('/');
    let client = reqwest::Client::new();

    let (url, builder, body) = match self.provider.dialect {
      Dialect::Anthropic => {
        let url = format!("{base}/v1/messages");
        let builder = client
          .post(&url)
          .header("x-api-key", &self.bearer)
          .header("anthropic-version", "2023-06-01");
        (url, builder, anthropic_body(&req))
      }
      Dialect::OpenAi => {
        let url = format!("{base}/chat/completions");
        let mut builder = client.post(&url);
        if !self.bearer.trim().is_empty() {
          builder = builder.bearer_auth(&self.bearer);
        }
        (url, builder, openai_body(&req))
      }
    };

    let res = crate::ai::client::extra_headers(&self.provider.id, builder)
      .timeout(TURN_TIMEOUT)
      .json(&body)
      .send()
      .await
      .map_err(|e| {
        if e.is_timeout() {
          AgentError::Failed {
            detail: "the AI took too long to reply. Nothing was changed.".into(),
          }
        } else if e.is_connect() {
          AgentError::TransportUnavailable {
            transport: self.transport,
            detail: format!("could not reach {url}"),
          }
        } else {
          AgentError::Failed { detail: format!("the request failed: {e}") }
        }
      })?;

    let status = res.status();
    let text = res
      .text()
      .await
      .map_err(|e| AgentError::Failed { detail: format!("could not read the reply: {e}") })?;

    if !status.is_success() {
      log::error!("agent turn rejected: provider={} status={}", self.provider.id, status);
      // 401/403 is a credential problem the user can fix by reconnecting;
      // everything else is the provider declining this particular request.
      return Err(if status.as_u16() == 401 || status.as_u16() == 403 {
        AgentError::NeedsReconnect {
          detail: format!("{} did not accept the credentials", self.provider.name),
        }
      } else {
        AgentError::Refused { detail: format!("{} said no ({status})", self.provider.name) }
      });
    }

    let parsed: Value = serde_json::from_str(&text)
      .map_err(|e| AgentError::Failed { detail: format!("the reply was not valid JSON: {e}") })?;

    match self.provider.dialect {
      Dialect::Anthropic => Ok(parse_anthropic(&parsed)),
      Dialect::OpenAi => Ok(parse_openai(&parsed)),
    }
  }
}

fn anthropic_body(req: &TurnRequest<'_>) -> Value {
  let messages: Vec<Value> = req.messages.iter().map(anthropic_message).collect();
  let mut body = json!({
    "model": req.model,
    "max_tokens": 4096,
    "system": req.system,
    "messages": messages,
  });
  if !req.tools.is_empty() {
    body["tools"] = Value::Array(req.tools.iter().map(anthropic_tool).collect());
  }
  body
}

fn anthropic_tool(t: &ToolSchema) -> Value {
  json!({ "name": t.name, "description": t.description, "input_schema": t.parameters })
}

fn anthropic_message(m: &Message) -> Value {
  match m {
    Message::User { content } => json!({ "role": "user", "content": content }),
    Message::Assistant { content, tool_calls } => {
      let mut blocks = Vec::new();
      if !content.is_empty() {
        blocks.push(json!({ "type": "text", "text": content }));
      }
      for c in tool_calls {
        blocks.push(json!({
          "type": "tool_use", "id": c.id, "name": c.name, "input": c.arguments
        }));
      }
      json!({ "role": "assistant", "content": blocks })
    }
    // Anthropic carries tool results as a user-role message, unlike OpenAI's
    // dedicated tool role.
    Message::Tool { result } => json!({
      "role": "user",
      "content": [{
        "type": "tool_result",
        "tool_use_id": result.id,
        "content": result.content,
        "is_error": result.is_error,
      }]
    }),
  }
}

fn openai_body(req: &TurnRequest<'_>) -> Value {
  let mut messages = vec![json!({ "role": "system", "content": req.system })];
  messages.extend(req.messages.iter().map(openai_message));
  let mut body = json!({ "model": req.model, "max_tokens": 4096, "messages": messages });
  if !req.tools.is_empty() {
    body["tools"] = Value::Array(req.tools.iter().map(openai_tool).collect());
  }
  body
}

fn openai_tool(t: &ToolSchema) -> Value {
  json!({
    "type": "function",
    "function": { "name": t.name, "description": t.description, "parameters": t.parameters }
  })
}

fn openai_message(m: &Message) -> Value {
  match m {
    Message::User { content } => json!({ "role": "user", "content": content }),
    Message::Assistant { content, tool_calls } => {
      let mut msg = json!({ "role": "assistant", "content": content });
      if !tool_calls.is_empty() {
        msg["tool_calls"] = Value::Array(
          tool_calls
            .iter()
            .map(|c| {
              json!({
                "id": c.id,
                "type": "function",
                // OpenAI takes the arguments as a JSON *string*, not an object.
                "function": { "name": c.name, "arguments": c.arguments.to_string() }
              })
            })
            .collect(),
        );
      }
      msg
    }
    Message::Tool { result } => json!({
      "role": "tool", "tool_call_id": result.id, "content": result.content
    }),
  }
}

fn parse_anthropic(parsed: &Value) -> AgentTurn {
  let mut text = String::new();
  let mut tool_calls = Vec::new();
  if let Some(blocks) = parsed.get("content").and_then(Value::as_array) {
    for block in blocks {
      match block.get("type").and_then(Value::as_str) {
        Some("text") => {
          if let Some(t) = block.get("text").and_then(Value::as_str) {
            text.push_str(t);
          }
        }
        Some("tool_use") => tool_calls.push(ToolCall {
          id: block.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
          name: block.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
          arguments: block.get("input").cloned().unwrap_or(Value::Null),
        }),
        _ => {}
      }
    }
  }
  AgentTurn { text, tool_calls }
}

fn parse_openai(parsed: &Value) -> AgentTurn {
  let message = &parsed["choices"][0]["message"];
  let text = message.get("content").and_then(Value::as_str).unwrap_or_default().to_string();
  let mut tool_calls = Vec::new();
  if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
    for c in calls {
      let f = &c["function"];
      // Arguments arrive as a JSON string. A model that emits a malformed one
      // should reach the tool as bad input, not crash the turn -- the tool
      // reports it and the model can correct on the next turn.
      let arguments = f
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
      tool_calls.push(ToolCall {
        id: c.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
        name: f.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        arguments,
      });
    }
  }
  AgentTurn { text, tool_calls }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ai::agent::transport::ToolResult;

  fn schema() -> ToolSchema {
    ToolSchema {
      name: "view".into(),
      description: "Read a file".into(),
      parameters: json!({ "type": "object", "properties": { "path": { "type": "string" } } }),
    }
  }

  #[test]
  fn openai_sends_tool_arguments_as_a_string() {
    // The dialects disagree here, and sending an object silently breaks tool
    // use rather than erroring.
    let msg = openai_message(&Message::Assistant {
      content: String::new(),
      tool_calls: vec![ToolCall {
        id: "c1".into(),
        name: "view".into(),
        arguments: json!({ "path": "a.txt" }),
      }],
    });
    let args = &msg["tool_calls"][0]["function"]["arguments"];
    assert!(args.is_string(), "OpenAI needs a JSON string, got {args}");
  }

  #[test]
  fn anthropic_sends_tool_arguments_as_an_object() {
    let msg = anthropic_message(&Message::Assistant {
      content: String::new(),
      tool_calls: vec![ToolCall {
        id: "c1".into(),
        name: "view".into(),
        arguments: json!({ "path": "a.txt" }),
      }],
    });
    assert!(msg["content"][0]["input"].is_object());
  }

  #[test]
  fn anthropic_tool_results_are_user_messages() {
    // Anthropic has no "tool" role; sending one is rejected by the API.
    let msg = anthropic_message(&Message::Tool {
      result: ToolResult { id: "c1".into(), content: "ok".into(), is_error: false },
    });
    assert_eq!(msg["role"], "user");
    assert_eq!(msg["content"][0]["type"], "tool_result");
  }

  #[test]
  fn openai_tool_results_use_the_tool_role() {
    let msg = openai_message(&Message::Tool {
      result: ToolResult { id: "c1".into(), content: "ok".into(), is_error: false },
    });
    assert_eq!(msg["role"], "tool");
    assert_eq!(msg["tool_call_id"], "c1");
  }

  #[test]
  fn parses_an_anthropic_turn_with_text_and_a_tool_call() {
    let res = json!({
      "content": [
        { "type": "text", "text": "Looking at it" },
        { "type": "tool_use", "id": "t1", "name": "view", "input": { "path": "a.txt" } }
      ]
    });
    let turn = parse_anthropic(&res);
    assert_eq!(turn.text, "Looking at it");
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].arguments["path"], "a.txt");
  }

  #[test]
  fn parses_an_openai_turn_and_decodes_string_arguments() {
    let res = json!({
      "choices": [{ "message": {
        "content": "Reading",
        "tool_calls": [{
          "id": "t1", "type": "function",
          "function": { "name": "view", "arguments": "{\"path\":\"a.txt\"}" }
        }]
      }}]
    });
    let turn = parse_openai(&res);
    assert_eq!(turn.text, "Reading");
    assert_eq!(turn.tool_calls[0].arguments["path"], "a.txt");
  }

  #[test]
  fn malformed_tool_arguments_do_not_lose_the_turn() {
    let res = json!({
      "choices": [{ "message": {
        "content": "",
        "tool_calls": [{
          "id": "t1", "function": { "name": "view", "arguments": "{not json" }
        }]
      }}]
    });
    let turn = parse_openai(&res);
    assert_eq!(turn.tool_calls.len(), 1, "the call should still surface");
    assert!(turn.tool_calls[0].arguments.is_null());
  }

  #[test]
  fn a_turn_with_no_tools_omits_the_tools_field() {
    // Some OpenAI-compatible servers reject an empty tools array.
    let req = TurnRequest { model: "m", system: "s", messages: &[], tools: &[] };
    assert!(openai_body(&req).get("tools").is_none());
    assert!(anthropic_body(&req).get("tools").is_none());
  }

  #[test]
  fn tools_are_shaped_per_dialect() {
    let t = schema();
    assert_eq!(openai_tool(&t)["function"]["parameters"]["type"], "object");
    assert_eq!(anthropic_tool(&t)["input_schema"]["type"], "object");
  }
}
