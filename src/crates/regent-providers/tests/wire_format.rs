//! Wire-format contract tests: payload shape sent to the API and parsing of
//! real-shaped responses, including parallel tool calls. No network.

use regent_kernel::{ChatMessage, ToolCall, ToolDefinition};
use regent_providers::infra::adapters::{build_payload, parse_response};
use regent_providers::{ChatRequest, ProviderError};
use serde_json::json;

fn tool_def() -> ToolDefinition {
    ToolDefinition {
        name: "terminal".into(),
        description: "Run a shell command".into(),
        parameters: json!({"type": "object", "properties": {"command": {"type": "string"}},
                           "required": ["command"]}),
        toolset: "terminal".into(),
    }
}

#[test]
fn payload_carries_system_tools_and_tool_round_trip() {
    let call = ToolCall {
        id: "c1".into(),
        name: "terminal".into(),
        arguments: "{}".into(),
    };
    let messages = vec![
        ChatMessage::user("list files"),
        ChatMessage::assistant(None, vec![call]),
        ChatMessage::tool_result("c1", "terminal", r#"{"stdout":"a.txt"}"#),
    ];
    let request = ChatRequest::new("You are Regent.", messages).with_tools(vec![tool_def()]);
    let payload = build_payload("test-model", &request);

    assert_eq!(payload["model"], "test-model");
    assert_eq!(payload["messages"][0]["role"], "system");
    assert_eq!(payload["messages"][0]["content"], "You are Regent.");
    assert_eq!(payload["messages"][2]["tool_calls"][0]["id"], "c1");
    assert_eq!(payload["messages"][2]["tool_calls"][0]["type"], "function");
    assert_eq!(payload["messages"][3]["role"], "tool");
    assert_eq!(payload["messages"][3]["tool_call_id"], "c1");
    assert_eq!(payload["tools"][0]["function"]["name"], "terminal");
    // Optional knobs stay absent unless set (stable payloads cache better).
    assert!(payload.get("temperature").is_none());
}

#[test]
fn parses_parallel_tool_calls_and_usage() {
    let body = json!({
        "choices": [{
            "finish_reason": "tool_calls",
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "a", "type": "function",
                     "function": {"name": "read_file", "arguments": "{\"path\":\"x\"}"}},
                    {"id": "b", "type": "function",
                     "function": {"name": "terminal", "arguments": {"command": "ls"}}}
                ]
            }
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });
    let response = parse_response(&body).unwrap();
    assert!(response.wants_tools());
    assert_eq!(response.message.tool_calls.len(), 2);
    assert_eq!(response.message.tool_calls[0].name, "read_file");
    // object-form arguments are normalized to a JSON string
    assert_eq!(
        response.message.tool_calls[1].arguments,
        r#"{"command":"ls"}"#
    );
    assert_eq!(response.usage.total_tokens, 15);
    assert_eq!(response.finish_reason.as_deref(), Some("tool_calls"));
}

#[test]
fn parses_text_response_with_reasoning() {
    let body = json!({
        "choices": [{
            "finish_reason": "stop",
            "message": {"role": "assistant", "content": "done",
                        "reasoning_content": "thought about it"}
        }]
    });
    let response = parse_response(&body).unwrap();
    assert!(!response.wants_tools());
    assert_eq!(response.message.content.as_deref(), Some("done"));
    assert_eq!(
        response.message.reasoning.as_deref(),
        Some("thought about it")
    );
    assert_eq!(response.usage.total_tokens, 0);
}

#[test]
fn malformed_response_is_a_typed_parse_error() {
    let err = parse_response(&json!({"choices": []})).unwrap_err();
    assert!(matches!(err, ProviderError::Parse(_)));
}

#[test]
fn retryability_classification() {
    assert!(ProviderError::RateLimited.is_retryable());
    assert!(
        ProviderError::Api {
            status: 503,
            body: String::new()
        }
        .is_retryable()
    );
    assert!(ProviderError::Network("reset".into()).is_retryable());
    assert!(!ProviderError::Auth { status: 401 }.is_retryable());
    assert!(
        !ProviderError::Api {
            status: 400,
            body: String::new()
        }
        .is_retryable()
    );
}
