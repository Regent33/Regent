//! Session resume: rebuild an Agent over persisted history (with repair
//! of partial turns). Split from `agent/mod.rs` (file-size rule).

use super::*;

impl Agent {
    /// Resumes an existing session. The **stored** system prompt normally wins
    /// over `fallback_system_prompt` (byte-stability across resumes). A newer
    /// Regent prompt schema is rebased once so rebuilt apps do not keep running
    /// stale behavior instructions forever; custom/unversioned prompts remain
    /// frozen. History is
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
                let stored_schema = crate::domain::prompts::system_prompt_schema(&stored);
                let fallback_schema = crate::domain::prompts::system_prompt_schema(&fallback);
                if fallback_schema.is_some() && fallback_schema != stored_schema {
                    store.update_session_prompt(&session_id, &fallback)?;
                    tracing::info!(
                        session = %session_id,
                        from = ?stored_schema,
                        to = ?fallback_schema,
                        "rebased stored system prompt to current schema"
                    );
                    fallback
                } else {
                    if stored != fallback {
                        tracing::info!(session = %session_id, "using stored system prompt (differs from caller's)");
                    }
                    stored
                }
            }
            None => fallback,
        };
        let mut transcript = Transcript::new();
        for stored in store.get_conversation(&session_id)? {
            let message = stored.message;
            if transcript.push(message.clone()).is_err() {
                transcript.settle_pending_tools("interrupted before completion");
                transcript.close_trailing_user(regent_kernel::NO_REPLY);
                if transcript.push(message).is_err() {
                    tracing::warn!(session = %session_id, "resume: skipped a stored message that violates transcript order");
                }
            }
        }
        // A stored tail from a crashed turn would make the next user push
        // illegal — close it exactly like run_turn's live recovery does. Not
        // persisted: the note is re-derived from the same stored rows on every
        // resume, so writing it back would only duplicate it.
        transcript.settle_pending_tools("interrupted before completion");
        transcript.close_trailing_user(regent_kernel::NO_REPLY);
        // Restored history was already reviewed by the prior process — only
        // messages added after resume count toward the next review batch.
        let reviewed_len = store
            .session_reviewed_message_count(&session_id)?
            .min(transcript.messages().len());
        // Same adoption as `Agent::new`: a resumed session hands its interrupt
        // to the tools it dispatches, so delegated children stop with it.
        let cancel = tool_context.cancel_token().unwrap_or_default();
        let tool_context = tool_context.with_cancel(cancel.clone());
        Ok(Self {
            provider,
            catalog,
            store,
            tool_context,
            config,
            session_id,
            transcript,
            system_prompt,
            cancel,
            turn_api_calls: 0,
            last_turn_budget_exhausted: false,
            compression_broken: false,
            last_turn_input_tokens: 0,
            last_turn_output_tokens: 0,
            last_turn_usage_complete: true,
            last_request_input_tokens: None,
            last_turn_cache_read: None,
            last_turn_cache_write: None,
            graph: None,
            review: None,
            review_handle: None,
            reviewed_len: Arc::new(std::sync::atomic::AtomicUsize::new(reviewed_len)),
            review_scheduled_len: Arc::new(std::sync::atomic::AtomicUsize::new(reviewed_len)),
            review_gate: Arc::new(tokio::sync::Mutex::new(())),
            delta_sink: None,
            last_cache_reset: None,
            timings: crate::application::agent::TurnTimings::default(),
            pending_cache_reset: None,
        })
    }
}
