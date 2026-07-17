use crate::domain::entities::{ChatRequest, ChatResponse};
use crate::domain::errors::ProviderError;
use async_trait::async_trait;

/// A callback that receives assistant-text deltas as they stream in. Used to
/// surface live output to a UI without waiting for the full turn.
pub type DeltaSink<'a> = &'a (dyn Fn(&str) + Send + Sync);

/// The chat contract with native (OpenAI-style) tool calling. Why this
/// exists instead of or-conduit's `ConduitProvider`: that contract is
/// text-only (tool use by parsing ReAct text); Regent requires structured
/// parallel `tool_calls` with `tool_call_id` plumbing (ADR-002).
#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError>;

    fn model(&self) -> &str;

    /// The active model's context window in tokens, when its family is known
    /// (`model_windows::window_for_model`); `None` for unrecognized models, so
    /// the caller falls back to its configured default. The default impl reads
    /// `self.model()`, so `FallbackChat` reports the ACTIVE chain member's
    /// window for free — compaction math follows a failover (ADR-038 P0a).
    fn context_window(&self) -> Option<u32> {
        crate::domain::model_windows::window_for_model(self.model())
    }

    /// Streaming variant: invoke `on_delta` for each assistant-text fragment
    /// as it arrives, returning the fully-accumulated response. The default
    /// is non-streaming — it calls `complete` and emits the whole reply once,
    /// so providers without a streaming path still satisfy the contract.
    async fn complete_streaming(
        &self,
        request: &ChatRequest,
        on_delta: DeltaSink<'_>,
    ) -> Result<ChatResponse, ProviderError> {
        let response = self.complete(request).await?;
        if let Some(text) = &response.message.content
            && !text.trim().is_empty()
        {
            on_delta(text);
        }
        Ok(response)
    }
}
