//! Session resume: rebuild an Agent over persisted history (with repair
//! of partial turns). Split from `agent/mod.rs` (file-size rule).

use super::*;

impl Agent {
    /// Resumes an existing session. The **stored** system prompt wins over
    /// `fallback_system_prompt` (byte-stability across resumes); history is
    /// replayed through the alternation-validating transcript. A crashed turn
    /// keeps its rows in the store (dangling user message, unanswered tool
    /// calls), so replay REPAIRS instead of failing: illegal rows get the same
    /// recovery `run_turn` applies live, and a repaired-but-still-illegal row
    /// is skipped — resume must never brick a session on old history.
    pub fn resume(
        provider: Arc<dyn ChatProvider>,
        catalog: Arc<ToolCatalog>,
        store: Arc<Store>,
        tool_context: ToolContext,
        fallback_system_prompt: impl Into<String>,
        config: AgentConfig,
        session_id: SessionId,
    ) -> Result<Self, RegentError> {
        let fallback = fallback_system_prompt.into();
        let system_prompt = match store.session_system_prompt(&session_id)? {
            Some(stored) => {
                if stored != fallback {
                    tracing::info!(session = %session_id, "using stored system prompt (differs from caller's)");
                }
                stored
            }
            None => fallback,
        };
        let mut transcript = Transcript::new();
        for stored in store.get_conversation(&session_id)? {
            let message = stored.message;
            if transcript.push(message.clone()).is_err() {
                transcript.settle_pending_tools("interrupted before completion");
                transcript.drop_trailing_user();
                if transcript.push(message).is_err() {
                    tracing::warn!(session = %session_id, "resume: skipped a stored message that violates transcript order");
                }
            }
        }
        // A stored tail from a crashed turn would make the next user push
        // illegal — trim it exactly like run_turn's live recovery does.
        transcript.settle_pending_tools("interrupted before completion");
        transcript.drop_trailing_user();
        // Restored history was already reviewed by the prior process — only
        // messages added after resume count toward the next review batch.
        let reviewed_len = transcript.messages().len();
        Ok(Self {
            provider,
            catalog,
            store,
            tool_context,
            config,
            session_id,
            transcript,
            system_prompt,
            cancel: CancellationToken::new(),
            turn_api_calls: 0,
            last_turn_budget_exhausted: false,
            compression_broken: false,
            last_turn_input_tokens: 0,
            last_turn_output_tokens: 0,
            last_turn_cache_read: None,
            last_turn_cache_write: None,
            graph: None,
            review: None,
            review_handle: None,
            reviewed_len,
            delta_sink: None,
            last_cache_reset: None,
            pending_cache_reset: None,
        })
    }
}
