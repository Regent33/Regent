//! Shared test doubles: scripted/runaway/slow providers, the echo tool, and
//! response builders.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, RegentError, ToolCall, ToolDefinition};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_tools::{DenyAll, ToolCatalog, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct ScriptedProvider {
    responses: Mutex<VecDeque<ChatResponse>>,
    repeat_tool_calls_forever: bool,
    delay: Option<std::time::Duration>,
}

impl ScriptedProvider {
    pub fn scripted(responses: Vec<ChatResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
            repeat_tool_calls_forever: false,
            delay: None,
        })
    }

    pub fn runaway() -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(VecDeque::new()),
            repeat_tool_calls_forever: true,
            delay: None,
        })
    }

    pub fn slow(delay: std::time::Duration) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(VecDeque::new()),
            repeat_tool_calls_forever: true,
            delay: Some(delay),
        })
    }
}

#[async_trait]
impl ChatProvider for ScriptedProvider {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        if self.repeat_tool_calls_forever {
            return Ok(tool_call_response(vec![call(
                "loop",
                "echo",
                json!({"text": "again"}),
            )]));
        }
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Parse("script exhausted".into()))
    }

    fn model(&self) -> &str {
        "scripted-model"
    }
}

struct EchoTool;

#[async_trait]
impl ToolExecutor for EchoTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        Ok(json!({"echo": args["text"]}).to_string())
    }
}

pub fn call(id: &str, name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: args.to_string(),
    }
}

pub fn tool_call_response(calls: Vec<ToolCall>) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(None, calls),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        },
        finish_reason: Some("tool_calls".into()),
    }
}

pub fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(Some(text.into()), vec![]),
        usage: TokenUsage {
            prompt_tokens: 20,
            completion_tokens: 8,
            total_tokens: 28,
        },
        finish_reason: Some("stop".into()),
    }
}

pub fn echo_catalog() -> Arc<ToolCatalog> {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(
            ToolDefinition {
                name: "echo".into(),
                description: "echo back".into(),
                parameters: json!({"type": "object"}),
                toolset: "test".into(),
            },
            Arc::new(EchoTool),
        )
        .unwrap();
    Arc::new(catalog)
}

pub fn test_context() -> ToolContext {
    ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
}
