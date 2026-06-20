use or_core::TokenUsage;
use regent_kernel::{ChatMessage, ToolDefinition};

/// One model call. `system` travels separately from the transcript so the
/// cached prefix stays byte-stable for the life of a conversation (the
/// Hermes prompt-caching invariant) — callers pass the same `system` string
/// every turn and append only transcript messages.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    /// Extended-thinking budget in tokens. `Some(n)` enables thinking (the
    /// model reasons before answering, up to `n` tokens); `None` disables it.
    /// Providers that don't support it ignore this.
    pub thinking_budget: Option<u32>,
}

impl ChatRequest {
    #[must_use]
    pub fn new(system: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            system: system.into(),
            messages,
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            thinking_budget: None,
        }
    }

    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Enables extended thinking with the given token budget.
    #[must_use]
    pub fn with_thinking(mut self, budget_tokens: u32) -> Self {
        self.thinking_budget = Some(budget_tokens);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// Always an assistant message; `tool_calls` non-empty when the model
    /// requested tools.
    pub message: ChatMessage,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
}

impl ChatResponse {
    #[must_use]
    pub fn wants_tools(&self) -> bool {
        !self.message.tool_calls.is_empty()
    }
}
