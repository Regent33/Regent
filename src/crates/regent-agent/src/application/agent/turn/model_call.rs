//! One model call: build the request from the live transcript (plus any
//! request-local repair messages), run it interruptibly, and account usage.

use super::output_check::{
    PROMISED_WORK_REPAIR, PSEUDO_TOOL_REPAIR, REASONING_ONLY_REPAIR, RetryState,
};
use crate::application::agent::Agent;
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, RegentError, ToolDefinition};
use regent_providers::{ChatRequest, ChatResponse};
use std::sync::Arc;

impl Agent {
    /// Builds and runs one (interruptible) completion, then folds its token
    /// usage into the per-turn counters and notes a mid-turn failover.
    pub(super) async fn call_model(
        &mut self,
        definitions: &[ToolDefinition],
        retry: &RetryState,
        start_model: &str,
    ) -> Result<ChatResponse, RegentError> {
        let mut request_messages = self.transcript.messages().to_vec();
        if retry.repair_reasoning_only {
            request_messages.push(ChatMessage::user(REASONING_ONLY_REPAIR));
        }
        if retry.repair_pseudo_tool {
            request_messages.push(ChatMessage::user(PSEUDO_TOOL_REPAIR));
        }
        if retry.repair_promised_work {
            request_messages.push(ChatMessage::user(PROMISED_WORK_REPAIR));
        }
        let mut request = ChatRequest::new(self.system_prompt.clone(), request_messages)
            .with_tools(definitions.to_vec());
        if let Some(budget) = self.config.thinking_budget {
            request = request.with_thinking(budget);
        }
        // SPL P2: opt into explicit prompt-cache breakpoints when the
        // session's cadence policy asks for them (deacon sets it per source).
        // `None` = today's request, no breakpoints; non-Anthropic providers
        // ignore it either way.
        if let Some(policy) = self.config.cache_policy {
            request = request.with_cache(policy);
        }
        // Bounded completions for internal turns (see `max_output_tokens`).
        // Unset for chat, so a long answer is never clipped.
        if let Some(limit) = self.config.max_output_tokens {
            request.max_tokens = Some(limit);
        }

        let response = match &self.delta_sink {
            Some(sink) => {
                let sink = Arc::clone(sink);
                let on_delta = move |fragment: &str| sink(fragment);
                tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => return Err(RegentError::Interrupted),
                    result = self.provider.complete_streaming(&request, &on_delta) => result?,
                }
            }
            None => tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Err(RegentError::Interrupted),
                result = self.provider.complete(&request) => result?,
            },
        };
        self.account_usage(&response.usage).await?;
        tracing::debug!(
            api_calls = self.turn_api_calls,
            model = self.provider.model(),
            "model call complete"
        );
        // Sticky failover: the fallback chain swapped providers mid-turn, so
        // the new model's cache is cold — attribute this turn to failover.
        if self.provider.model() != start_model {
            self.note_cache_reset("failover");
        }
        Ok(response)
    }

    /// Store writes are blocking SQLite calls — bridged off the runtime in
    /// exactly one place.
    pub(crate) async fn persist(
        &self,
        message: ChatMessage,
        token_count: Option<i64>,
        finish_reason: Option<String>,
    ) -> Result<(), RegentError> {
        let store = Arc::clone(&self.store);
        let session_id = self.session_id.clone();
        tokio::task::spawn_blocking(move || {
            store.append_message(&session_id, &message, token_count, finish_reason.as_deref())
        })
        .await
        .map_err(|join_error| RegentError::Store(join_error.to_string()))??;
        Ok(())
    }

    /// The one accounting boundary for every successful provider response,
    /// including ordinary, compaction, and budget-wrap-up calls.
    pub(crate) async fn account_usage(&mut self, usage: &TokenUsage) -> Result<(), RegentError> {
        // A real request necessarily has prompt tokens. Some adapters can
        // produce total/cache-shaped zero values when the provider omitted the
        // usage object, so those fields cannot prove input/output reporting.
        let reported = usage.prompt_tokens > 0;
        self.turn_api_calls = self.turn_api_calls.saturating_add(1);
        self.record_usage(
            i64::from(usage.prompt_tokens),
            i64::from(usage.completion_tokens),
            reported,
        )
        .await?;
        self.last_turn_usage_complete &= reported;
        self.last_request_input_tokens = reported.then_some(usage.prompt_tokens);
        self.last_turn_input_tokens = self
            .last_turn_input_tokens
            .saturating_add(usage.prompt_tokens);
        self.last_turn_output_tokens = self
            .last_turn_output_tokens
            .saturating_add(usage.completion_tokens);
        if let Some(read) = usage.cache_read_tokens {
            self.last_turn_cache_read =
                Some(self.last_turn_cache_read.unwrap_or(0).saturating_add(read));
        }
        if let Some(write) = usage.cache_write_tokens {
            self.last_turn_cache_write = Some(
                self.last_turn_cache_write
                    .unwrap_or(0)
                    .saturating_add(write),
            );
        }
        Ok(())
    }

    async fn record_usage(
        &self,
        input: i64,
        output: i64,
        reported: bool,
    ) -> Result<(), RegentError> {
        let store = Arc::clone(&self.store);
        let session_id = self.session_id.clone();
        tokio::task::spawn_blocking(move || {
            store.record_usage(&session_id, input, output, reported)
        })
        .await
        .map_err(|join_error| RegentError::Store(join_error.to_string()))??;
        Ok(())
    }
}
