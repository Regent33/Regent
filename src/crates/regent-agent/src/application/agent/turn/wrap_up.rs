//! Gap L2 budget wrap-up. Split from `turn.rs` (file-size rule).

use super::super::Agent;
use crate::domain::prompts::WRAP_UP_PROMPT;
use regent_kernel::{ChatMessage, RegentError};
use regent_providers::ChatRequest;
use std::sync::Arc;

impl Agent {
    /// Gap L2: budget exhaustion ends the turn with a summary, not a hard
    /// error. One final model call — tool list EMPTY, so it cannot start new
    /// work and cannot recurse into the budget checks — asks for a wrap-up:
    /// done / remaining / where to resume. The turns ledger still records
    /// `budget_exhausted` (via the flag `record_turn_outcome` reads); if this
    /// last call itself fails, `run_turn`'s recovery drops the trailing
    /// wrap-up user message and the transcript stays legal.
    pub(super) async fn budget_wrap_up(&mut self) -> Result<String, RegentError> {
        self.last_turn_budget_exhausted = true;
        let wrap_up = ChatMessage::user(WRAP_UP_PROMPT);
        self.transcript.push(wrap_up.clone())?;
        self.persist(wrap_up, None, None).await?;

        let request = ChatRequest::new(
            self.system_prompt.clone(),
            self.transcript.messages().to_vec(),
        );
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
        self.turn_api_calls += 1;
        self.record_usage(
            i64::from(response.usage.prompt_tokens),
            i64::from(response.usage.completion_tokens),
        )
        .await?;

        let text = response
            .message
            .content
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| {
                "Turn budget exhausted before a wrap-up summary could be produced.".to_owned()
            });
        // Stray tool calls are dropped (none were offered) — persisting them
        // would leave the transcript with forever-pending tools.
        let assistant = ChatMessage::assistant(Some(text.clone()), vec![]);
        let completion_tokens = i64::from(response.usage.completion_tokens);
        self.transcript.push(assistant.clone())?;
        self.persist(assistant, Some(completion_tokens), response.finish_reason)
            .await?;
        tracing::info!(
            api_calls = self.turn_api_calls,
            "turn budget exhausted — wrap-up summary returned"
        );
        Ok(text)
    }
}
