//! Talking to a language model. Two wire formats cover almost every provider: Anthropic's own
//! (Claude) and the "OpenAI-compatible" chat format (OpenAI, DeepSeek, Qwen, OpenRouter, Mistral,
//! Ollama and others). The rest of the app only sees the plain types below.
//!
//! The API key is read here from a `Secret`, goes into one request header and nowhere else: it is
//! not logged, not put in an error and not returned. Certificates are always checked.

use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, ErrorCode};
use crate::secret::Secret;

/// How long one request may take. Models can be slow to start answering.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
/// The most the model may write in one answer.
const MAX_ANSWER_TOKENS: u32 = 4096;
/// Longest piece of an error message from a provider that is passed on.
const MAX_ERROR_TEXT: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Claude, through Anthropic's own interface.
    Anthropic,
    /// Anything that speaks the OpenAI chat format.
    OpenAiCompatible,
}

/// One thing the model asked to be done.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// What came of one tool call, sent back to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

/// One turn of the conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    User { text: String },
    Assistant {
        #[serde(default)]
        text: String,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    ToolResults { results: Vec<ToolResult> },
}

/// A tool the model may use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema of the input.
    pub schema: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stop {
    /// The model finished its answer.
    Done,
    /// The model wants tools run.
    Tools,
    /// The answer was cut short by the length limit.
    Length,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reply {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop: Stop,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ---- Anthropic ---------------------------------------------------------------------------

pub fn anthropic_request(model: &str, system: &str, messages: &[Message], tools: &[ToolDef]) -> Value {
    let messages: Vec<Value> = messages
        .iter()
        .map(|m| match m {
            Message::User { text } => json!({"role": "user", "content": [{"type": "text", "text": text}]}),
            Message::Assistant { text, tool_calls } => {
                let mut blocks = Vec::new();
                if !text.is_empty() {
                    blocks.push(json!({"type": "text", "text": text}));
                }
                for call in tool_calls {
                    blocks.push(json!({"type": "tool_use", "id": call.id, "name": call.name, "input": call.input}));
                }
                json!({"role": "assistant", "content": blocks})
            }
            Message::ToolResults { results } => json!({
                "role": "user",
                "content": results.iter().map(|r| json!({
                    "type": "tool_result",
                    "tool_use_id": r.call_id,
                    "content": r.content,
                    "is_error": r.is_error,
                })).collect::<Vec<_>>(),
            }),
        })
        .collect();
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_ANSWER_TOKENS,
        "system": system,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = json!(
            tools
                .iter()
                .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.schema}))
                .collect::<Vec<_>>()
        );
    }
    body
}

pub fn parse_anthropic(value: &Value) -> Result<Reply, AppError> {
    let blocks = value
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(odd_answer)?;
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text.push_str(block.get("text").and_then(Value::as_str).unwrap_or("")),
            Some("tool_use") => tool_calls.push(ToolCall {
                id: block.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                name: block.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                input: block.get("input").cloned().unwrap_or(Value::Null),
            }),
            _ => {}
        }
    }
    let stop = match value.get("stop_reason").and_then(Value::as_str) {
        Some("tool_use") => Stop::Tools,
        Some("max_tokens") => Stop::Length,
        _ if !tool_calls.is_empty() => Stop::Tools,
        _ => Stop::Done,
    };
    Ok(Reply {
        text,
        tool_calls,
        stop,
        input_tokens: value.pointer("/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0),
        output_tokens: value.pointer("/usage/output_tokens").and_then(Value::as_u64).unwrap_or(0),
    })
}

// ---- OpenAI-compatible -------------------------------------------------------------------

pub fn openai_request(model: &str, system: &str, messages: &[Message], tools: &[ToolDef]) -> Value {
    let mut out = vec![json!({"role": "system", "content": system})];
    for m in messages {
        match m {
            Message::User { text } => out.push(json!({"role": "user", "content": text})),
            Message::Assistant { text, tool_calls } => {
                let mut message = json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) }});
                if !tool_calls.is_empty() {
                    message["tool_calls"] = json!(
                        tool_calls
                            .iter()
                            .map(|c| json!({
                                "id": c.id,
                                "type": "function",
                                "function": {"name": c.name, "arguments": c.input.to_string()},
                            }))
                            .collect::<Vec<_>>()
                    );
                }
                out.push(message);
            }
            Message::ToolResults { results } => {
                for r in results {
                    out.push(json!({"role": "tool", "tool_call_id": r.call_id, "content": r.content}));
                }
            }
        }
    }
    let mut body = json!({"model": model, "messages": out});
    if !tools.is_empty() {
        body["tools"] = json!(
            tools
                .iter()
                .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.schema}}))
                .collect::<Vec<_>>()
        );
    }
    body
}

pub fn parse_openai(value: &Value) -> Result<Reply, AppError> {
    let choice = value.pointer("/choices/0").ok_or_else(odd_answer)?;
    let message = choice.get("message").ok_or_else(odd_answer)?;
    let text = message.get("content").and_then(Value::as_str).unwrap_or("").to_string();
    let tool_calls: Vec<ToolCall> = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .map(|c| {
                    let arguments = c.pointer("/function/arguments");
                    // Arguments arrive as text holding JSON. Models sometimes get it wrong; that
                    // is reported to the model as a failed call, not hidden.
                    let input = match arguments {
                        Some(Value::String(s)) if s.trim().is_empty() => json!({}),
                        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                        Some(other) => other.clone(),
                        None => json!({}),
                    };
                    ToolCall {
                        id: c.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                        name: c.pointer("/function/name").and_then(Value::as_str).unwrap_or("").to_string(),
                        input,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let stop = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("length") => Stop::Length,
        Some("tool_calls") | Some("function_call") => Stop::Tools,
        _ if !tool_calls.is_empty() => Stop::Tools,
        _ => Stop::Done,
    };
    Ok(Reply {
        text,
        tool_calls,
        stop,
        input_tokens: value.pointer("/usage/prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
        output_tokens: value.pointer("/usage/completion_tokens").and_then(Value::as_u64).unwrap_or(0),
    })
}

fn odd_answer() -> AppError {
    AppError::new(
        ErrorCode::Internal,
        "The model's answer wasn't in the expected format. Check the provider type and address.",
    )
}

// ---- Sending -----------------------------------------------------------------------------

/// Where to send, and what for.
pub struct Endpoint<'a> {
    pub kind: ProviderKind,
    /// For example `https://api.openai.com/v1`, without a trailing slash.
    pub base_url: &'a str,
    pub model: &'a str,
}

/// Makes sure the HTTPS code has a crypto provider. Safe to call many times.
fn ensure_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn friendly(status: StatusCode, body: &Value) -> AppError {
    // Providers explain themselves in `error.message` (or `message`); that text comes from them,
    // never from the request, so it is safe to show once cleaned and cut.
    let said = body
        .pointer("/error/message")
        .or_else(|| body.get("message"))
        .and_then(Value::as_str)
        .map(|m| crate::docker::engine::clean(m))
        .map(|m| m.chars().take(MAX_ERROR_TEXT).collect::<String>())
        .filter(|m| !m.is_empty());
    let (code, base) = match status.as_u16() {
        401 | 403 => (ErrorCode::PermissionDenied, "The provider didn't accept the API key."),
        404 => (ErrorCode::NotFound, "The provider doesn't know that model or address."),
        408 | 504 => (ErrorCode::ConnectionFailed, "The provider took too long to answer."),
        429 => (ErrorCode::ConnectionFailed, "The provider says there have been too many requests, or the credit has run out."),
        500..=599 => (ErrorCode::ConnectionFailed, "The provider had a problem. Try again in a moment."),
        _ => (ErrorCode::InvalidInput, "The provider refused the request."),
    };
    AppError::new(
        code,
        match said {
            Some(said) => format!("{base} It said: {said}"),
            None => base.to_string(),
        },
    )
}

/// Sends one request and returns the model's answer. Stops early if `cancel` fires.
pub async fn complete(
    endpoint: &Endpoint<'_>,
    key: Option<&Secret>,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
    cancel: &CancellationToken,
) -> Result<Reply, AppError> {
    ensure_crypto();
    let base = endpoint.base_url.trim_end_matches('/');
    let (url, body) = match endpoint.kind {
        ProviderKind::Anthropic => (
            format!("{base}/v1/messages"),
            anthropic_request(endpoint.model, system, messages, tools),
        ),
        ProviderKind::OpenAiCompatible => (
            format!("{base}/chat/completions"),
            openai_request(endpoint.model, system, messages, tools),
        ),
    };
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        // An address that redirects could carry the key somewhere else.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't start the connection."))?;
    let mut request = client.post(url).json(&body);
    if let Some(key) = key {
        request = match endpoint.kind {
            ProviderKind::Anthropic => request
                .header("x-api-key", key.expose())
                .header("anthropic-version", "2023-06-01"),
            ProviderKind::OpenAiCompatible => request.bearer_auth(key.expose()),
        };
    }
    let sent = tokio::select! {
        _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Cancelled, "Stopped.")),
        sent = request.send() => sent,
    };
    let response = sent.map_err(|e| {
        if e.is_timeout() {
            AppError::new(ErrorCode::ConnectionFailed, "The provider took too long to answer.")
        } else {
            AppError::new(
                ErrorCode::ConnectionFailed,
                "Couldn't reach the provider. Check the address and your connection.",
            )
        }
    })?;
    let status = response.status();
    let value: Value = tokio::select! {
        _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Cancelled, "Stopped.")),
        read = response.json::<Value>() => read.unwrap_or(Value::Null),
    };
    if !status.is_success() {
        return Err(friendly(status, &value));
    }
    match endpoint.kind {
        ProviderKind::Anthropic => parse_anthropic(&value),
        ProviderKind::OpenAiCompatible => parse_openai(&value),
    }
}
