//! M3 learning-loop integration: the background review fork persists
//! memory without touching the main conversation, and an agent-created
//! skill survives into the next session's library (proposal M3 exit
//! criteria). Shared scripted-provider harness lives here; the tests are
//! grouped by concern in the submodules.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, ToolCall};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_tools::{DenyAll, ToolContext};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

mod review_gate;
mod review_targets;
mod skills;

pub struct Scripted {
    responses: Mutex<VecDeque<ChatResponse>>,
    /// Last-message content of every request, in call order — lets tests
    /// inspect the snapshot a review fork actually received.
    pub prompts: Mutex<Vec<String>>,
}

impl Scripted {
    pub fn new(responses: Vec<ChatResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
            prompts: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl ChatProvider for Scripted {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.prompts.lock().unwrap().push(
            request
                .messages
                .last()
                .and_then(|m| m.content.clone())
                .unwrap_or_default(),
        );
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

pub fn text(content: &str) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(Some(content.into()), vec![]),
        usage: TokenUsage::default(),
        finish_reason: Some("stop".into()),
    }
}

pub fn tool_call(name: &str, args: serde_json::Value) -> ChatResponse {
    let call = ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: args.to_string(),
    };
    ChatResponse {
        message: ChatMessage::assistant(None, vec![call]),
        usage: TokenUsage::default(),
        finish_reason: Some("tool_calls".into()),
    }
}

pub fn context() -> ToolContext {
    ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
}
