//! Fallback-chain behavior. The shared `Flaky` provider harness lives here;
//! the tests are grouped by concern in the submodules.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

mod amplification;
mod empty_answers;
mod reroute;
mod streaming;

/// Provider that fails `fail_first` times with `error_factory`, then answers.
pub struct Flaky {
    name: &'static str,
    calls: AtomicU32,
    fail_always: bool,
    empty: bool,
    reasoning_only: bool,
    /// Stream severed before the terminal chunk: thinking arrived, the
    /// `finish_reason` never did.
    truncated: bool,
    error_factory: fn() -> ProviderError,
}

impl Flaky {
    pub fn failing_with(name: &'static str, error_factory: fn() -> ProviderError) -> Arc<Self> {
        Arc::new(Self {
            name,
            calls: AtomicU32::new(0),
            fail_always: true,
            empty: false,
            reasoning_only: false,
            truncated: false,
            error_factory,
        })
    }

    pub fn healthy(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            calls: AtomicU32::new(0),
            fail_always: false,
            empty: false,
            reasoning_only: false,
            truncated: false,
            error_factory: || ProviderError::Parse("unused".into()),
        })
    }

    /// Answers HTTP 200 with whitespace-only content and no tool calls — a
    /// flaky provider that "succeeds" but produces nothing (the nemotron case).
    pub fn empty(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            calls: AtomicU32::new(0),
            fail_always: false,
            empty: true,
            reasoning_only: false,
            truncated: false,
            error_factory: || ProviderError::Parse("unused".into()),
        })
    }

    pub fn reasoning_only(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            calls: AtomicU32::new(0),
            fail_always: false,
            empty: false,
            reasoning_only: true,
            truncated: false,
            error_factory: || ProviderError::Parse("unused".into()),
        })
    }

    /// Answers HTTP 200 with reasoning but NO `finish_reason` — an SSE stream
    /// cut mid-thought, which is indistinguishable from a finished one unless
    /// the terminal chunk is checked for.
    pub fn truncated(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            calls: AtomicU32::new(0),
            fail_always: false,
            empty: false,
            reasoning_only: true,
            truncated: true,
            error_factory: || ProviderError::Parse("unused".into()),
        })
    }

    pub fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ChatProvider for Flaky {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_always {
            return Err((self.error_factory)());
        }
        if self.reasoning_only {
            let mut message = ChatMessage::assistant(None, vec![]);
            message.reasoning =
                Some("I should call a tool, but I will only think about it.".into());
            return Ok(ChatResponse {
                message,
                usage: TokenUsage::default(),
                finish_reason: (!self.truncated).then(|| "stop".to_owned()),
            });
        }
        let content = if self.empty {
            "   ".to_owned() // whitespace-only = empty
        } else {
            format!("answer from {}", self.name)
        };
        Ok(ChatResponse {
            message: ChatMessage::assistant(Some(content), vec![]),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
        })
    }

    fn model(&self) -> &str {
        self.name
    }
}

pub fn request() -> ChatRequest {
    ChatRequest::new("system", vec![ChatMessage::user("hello")])
}
