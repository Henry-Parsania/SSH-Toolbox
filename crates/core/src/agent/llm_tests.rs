//! The two provider formats, against a small stand-in web server on this computer.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::llm::*;
use crate::error::ErrorCode;
use crate::secret::Secret;

/// What the stand-in server was sent: the request line, the headers and the body.
#[derive(Default, Clone)]
struct Seen {
    head: Arc<Mutex<String>>,
    body: Arc<Mutex<String>>,
}

/// Answers every request with `status` and `body`. Returns its port.
async fn serve(status: &'static str, body: Value) -> (u16, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Seen::default();
    let record = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let record = record.clone();
            let body = body.to_string();
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&chunk[..n]);
                    if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&data[..end]).into_owned();
                        let length = head
                            .lines()
                            .find_map(|l| l.to_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                            .unwrap_or(0);
                        while data.len() < end + 4 + length {
                            let n = socket.read(&mut chunk).await.unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            data.extend_from_slice(&chunk[..n]);
                        }
                        *record.head.lock().unwrap() = head;
                        *record.body.lock().unwrap() = String::from_utf8_lossy(&data[end + 4..]).into_owned();
                        break;
                    }
                }
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    (port, seen)
}

fn tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "run_command".into(),
        description: "Run a command".into(),
        schema: json!({"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}),
    }]
}

fn history() -> Vec<Message> {
    vec![
        Message::User { text: "check the disk".into() },
        Message::Assistant {
            text: "Looking.".into(),
            tool_calls: vec![ToolCall { id: "t1".into(), name: "run_command".into(), input: json!({"command": "df -h"}) }],
        },
        Message::ToolResults { results: vec![ToolResult { call_id: "t1".into(), content: "Filesystem ...".into(), is_error: false }] },
    ]
}

#[test]
fn claude_requests_use_content_blocks_and_tool_results() {
    let body = anthropic_request("claude-x", "be careful", &history(), &tools());
    assert_eq!(body["system"], "be careful");
    assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
    assert_eq!(body["messages"][1]["content"][1]["type"], "tool_use");
    assert_eq!(body["messages"][1]["content"][1]["input"]["command"], "df -h");
    assert_eq!(body["messages"][2]["role"], "user");
    assert_eq!(body["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "t1");
}

#[test]
fn openai_requests_use_tool_calls_with_text_arguments_and_tool_messages() {
    let body = openai_request("gpt-x", "be careful", &history(), &tools());
    assert_eq!(body["messages"][0]["role"], "system");
    let call = &body["messages"][2]["tool_calls"][0];
    assert_eq!(call["function"]["name"], "run_command");
    // The arguments are JSON written out as text, as that format wants.
    let arguments: Value = serde_json::from_str(call["function"]["arguments"].as_str().unwrap()).unwrap();
    assert_eq!(arguments["command"], "df -h");
    assert_eq!(body["messages"][3]["role"], "tool");
    assert_eq!(body["messages"][3]["tool_call_id"], "t1");
    assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
}

#[test]
fn answers_with_tool_calls_are_read_in_both_formats() {
    let claude = parse_anthropic(&json!({
        "content": [{"type": "text", "text": "Checking"}, {"type": "tool_use", "id": "a", "name": "run_command", "input": {"command": "ls"}}],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    }))
    .unwrap();
    assert_eq!((claude.stop, claude.tool_calls.len(), claude.input_tokens), (Stop::Tools, 1, 10));
    assert_eq!(claude.tool_calls[0].input["command"], "ls");

    let openai = parse_openai(&json!({
        "choices": [{"finish_reason": "tool_calls", "message": {"content": null, "tool_calls": [
            {"id": "b", "type": "function", "function": {"name": "run_command", "arguments": "{\"command\":\"ls\"}"}}
        ]}}],
        "usage": {"prompt_tokens": 7, "completion_tokens": 3}
    }))
    .unwrap();
    assert_eq!((openai.stop, openai.tool_calls.len(), openai.output_tokens), (Stop::Tools, 1, 3));
    assert_eq!(openai.tool_calls[0].input["command"], "ls");
}

#[test]
fn broken_tool_arguments_are_kept_visible_not_guessed() {
    let reply = parse_openai(&json!({
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [
            {"id": "b", "function": {"name": "run_command", "arguments": "{not json"}}
        ]}}]
    }))
    .unwrap();
    assert!(reply.tool_calls[0].input.is_null());
}

#[tokio::test]
async fn a_claude_request_carries_the_key_in_the_header_only() {
    let (port, seen) = serve(
        "200 OK",
        json!({"content": [{"type": "text", "text": "All fine."}], "stop_reason": "end_turn", "usage": {"input_tokens": 1, "output_tokens": 2}}),
    )
    .await;
    let base = format!("http://127.0.0.1:{port}");
    let key = Secret::new("sk-test-secret-key");
    let reply = complete(
        &Endpoint { kind: ProviderKind::Anthropic, base_url: &base, model: "claude-x" },
        Some(&key),
        "sys",
        &[Message::User { text: "hi".into() }],
        &tools(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!((reply.text.as_str(), reply.stop), ("All fine.", Stop::Done));
    let head = seen.head.lock().unwrap().to_lowercase();
    assert!(head.starts_with("post /v1/messages"), "{head}");
    assert!(head.contains("x-api-key: sk-test-secret-key"));
    assert!(head.contains("anthropic-version"));
    assert!(!seen.body.lock().unwrap().contains("sk-test-secret-key"));
}

#[tokio::test]
async fn an_openai_style_request_uses_a_bearer_key_and_works_without_one() {
    let (port, seen) = serve(
        "200 OK",
        json!({"choices": [{"finish_reason": "stop", "message": {"content": "Done."}}], "usage": {"prompt_tokens": 1, "completion_tokens": 1}}),
    )
    .await;
    let base = format!("http://127.0.0.1:{port}/v1/");
    let endpoint = Endpoint { kind: ProviderKind::OpenAiCompatible, base_url: &base, model: "m" };
    let messages = [Message::User { text: "hi".into() }];
    let key = Secret::new("secret-abc");
    let reply = complete(&endpoint, Some(&key), "sys", &messages, &[], &CancellationToken::new()).await.unwrap();
    assert_eq!(reply.text, "Done.");
    let head = seen.head.lock().unwrap().to_lowercase();
    assert!(head.starts_with("post /v1/chat/completions"), "{head}");
    assert!(head.contains("authorization: bearer secret-abc"));
    // A local model needs no key at all.
    complete(&endpoint, None, "sys", &messages, &[], &CancellationToken::new()).await.unwrap();
    assert!(!seen.head.lock().unwrap().to_lowercase().contains("authorization"));
}

#[tokio::test]
async fn provider_errors_become_plain_messages_without_the_key() {
    for (status, code) in [("401 Unauthorized", ErrorCode::PermissionDenied), ("404 Not Found", ErrorCode::NotFound), ("429 Too Many Requests", ErrorCode::ConnectionFailed)] {
        let (port, _) = serve(status, json!({"error": {"message": "model gone\u{1b}[31m"}})).await;
        let base = format!("http://127.0.0.1:{port}");
        let key = Secret::new("secret-abc");
        let err = complete(
            &Endpoint { kind: ProviderKind::OpenAiCompatible, base_url: &base, model: "m" },
            Some(&key),
            "s",
            &[Message::User { text: "x".into() }],
            &[],
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, code, "{status}");
        assert!(err.message.contains("model gone"), "{}", err.message);
        assert!(!err.message.contains('\u{1b}'));
        assert!(!err.message.contains("secret-abc"));
    }
}

#[tokio::test]
async fn a_request_can_be_stopped_and_an_unreachable_provider_is_reported() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = complete(
        &Endpoint { kind: ProviderKind::Anthropic, base_url: "http://127.0.0.1:9", model: "m" },
        None,
        "s",
        &[Message::User { text: "x".into() }],
        &[],
        &cancel,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Cancelled);
    let err = complete(
        &Endpoint { kind: ProviderKind::Anthropic, base_url: "http://127.0.0.1:9", model: "m" },
        None,
        "s",
        &[Message::User { text: "x".into() }],
        &[],
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ConnectionFailed);
}
