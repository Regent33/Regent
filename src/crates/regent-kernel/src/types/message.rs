use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Tool,
}

impl Role {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// One tool invocation requested by the model. `arguments` is the raw JSON
/// string exactly as the provider sent it (OpenAI wire format) — executors
/// parse it; the kernel does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Internal message format, OpenAI-style (the Hermes convention): one struct
/// for all roles, with role-specific fields optional. The system prompt is
/// deliberately NOT a transcript message — it travels separately on each
/// request so the cached prefix stays byte-stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Option<String>,
    /// Assistant-only: tool invocations requested this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Tool-only: which call this result answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool-only: tool name, kept for search/telemetry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Assistant-only: provider-exposed reasoning text, never replayed
    /// as instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Assistant-only: opaque signature for an extended-thinking block. Paired
    /// with `reasoning` (the thinking text), it lets the thinking block be
    /// replayed verbatim on the next request — required for multi-turn tool use
    /// with extended thinking. Not persisted: only the in-turn most-recent
    /// thinking block must be replayed, and that lives in memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_signature: Option<String>,
}

impl ChatMessage {
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(text.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
            reasoning: None,
            thinking_signature: None,
        }
    }

    #[must_use]
    pub fn assistant(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
            tool_name: None,
            reasoning: None,
            thinking_signature: None,
        }
    }

    #[must_use]
    pub fn tool_result(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        result_json: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: Some(result_json.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id.into()),
            tool_name: Some(tool_name.into()),
            reasoning: None,
            thinking_signature: None,
        }
    }
}
