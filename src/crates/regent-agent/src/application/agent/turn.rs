//! The turn loop: assemble context → (interruptible) model call → execute
//! tools → observe → check stop conditions. Stop conditions (budget,
//! interrupt) are checked here, never left to the model. Compression runs as a
//! child-session split, never by mutating history.

use super::Agent;
use futures::future::join_all;
use regent_kernel::{ChatMessage, RegentError};
use regent_providers::ChatRequest;
use std::sync::Arc;

impl Agent {
    /// Runs one user turn and records its outcome in the turns ledger. On
    /// success — or an interrupted turn that left a partial tool exchange — the
    /// background review fork (if configured) is spawned, fire-and-forget,
    /// never blocking the reply.
    pub async fn run_turn(&mut self, user_text: &str) -> Result<String, RegentError> {
        let started_at = regent_store::now_epoch();
        let result = self.run_turn_inner(user_text).await;
        // A successful turn is always review-worthy. A failed/interrupted turn is
        // only worth reviewing if it left a *partial tool exchange* (settling
        // pending tools produced rows) — a turn that reverted to pre-turn state
        // has nothing new to learn, and re-reviewing old history is wasteful.
        let mut review_worthy = result.is_ok();
        if result.is_err() {
            // A failed/interrupted turn can leave the transcript illegal for the
            // next turn in two ways:
            //  1. an assistant message with tool calls but no results (interrupted
            //     mid-dispatch) — settle them with synthetic results, persisted so
            //     a resumed session replaying the store stays legal;
            //  2. a trailing user message with no reply — drop it (the store keeps
            //     the user row; only live history is trimmed).
            let settled = self
                .transcript
                .settle_pending_tools("interrupted before completion");
            review_worthy = !settled.is_empty();
            for msg in settled {
                let _ = self.persist(msg, None, None).await;
            }
            self.transcript.drop_trailing_user();
        }
        self.record_turn_outcome(&result, started_at).await;
        if review_worthy {
            self.spawn_review_if_configured();
        }
        result
    }

    async fn run_turn_inner(&mut self, user_text: &str) -> Result<String, RegentError> {
        let user = ChatMessage::user(user_text);
        self.transcript.push(user.clone())?;
        self.persist(user, None, None).await?;

        // Built once per turn from the same catalog — byte-stable ordering.
        let definitions = self.catalog.definitions();
        self.turn_api_calls = 0;
        // Per-turn token spend, summed across model calls (W2.4 cost ceiling).
        let mut turn_tokens: u64 = 0;

        loop {
            if self.cancel.is_cancelled() {
                return Err(RegentError::Interrupted);
            }
            if self.turn_api_calls >= self.config.max_iterations {
                return Err(RegentError::BudgetExhausted(self.turn_api_calls));
            }
            // Per-turn token ceiling: halt before spending past the cap (the
            // call that crosses it completes; the next iteration stops here).
            if let Some(ceiling) = self.config.max_turn_tokens
                && turn_tokens >= u64::from(ceiling)
            {
                tracing::warn!(
                    turn_tokens,
                    ceiling,
                    api_calls = self.turn_api_calls,
                    "per-turn token ceiling reached — halting turn"
                );
                return Err(RegentError::BudgetExhausted(self.turn_api_calls));
            }
            self.maybe_compress().await?;

            let mut request = ChatRequest::new(
                self.system_prompt.clone(),
                self.transcript.messages().to_vec(),
            )
            .with_tools(definitions.clone());
            if let Some(budget) = self.config.thinking_budget {
                request = request.with_thinking(budget);
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
            self.turn_api_calls += 1;
            tracing::debug!(
                api_calls = self.turn_api_calls,
                model = self.provider.model(),
                "model call complete"
            );
            self.record_usage(
                i64::from(response.usage.prompt_tokens),
                i64::from(response.usage.completion_tokens),
            )
            .await?;
            turn_tokens += u64::from(response.usage.prompt_tokens)
                + u64::from(response.usage.completion_tokens);

            let assistant = response.message;
            let completion_tokens = i64::from(response.usage.completion_tokens);
            self.transcript.push(assistant.clone())?;
            self.persist(
                assistant.clone(),
                Some(completion_tokens),
                response.finish_reason.clone(),
            )
            .await?;

            if assistant.tool_calls.is_empty() {
                return Ok(assistant.content.unwrap_or_default());
            }

            // Parallel dispatch; results re-attached in original call order
            // (join_all preserves input order regardless of completion order).
            let dispatches = assistant.tool_calls.iter().map(|call| {
                let catalog = Arc::clone(&self.catalog);
                let ctx = self.tool_context.clone();
                let (name, arguments) = (call.name.clone(), call.arguments.clone());
                async move { catalog.dispatch(&name, &arguments, &ctx).await }
            });
            // Interruptible: a cancel drops the in-flight dispatch future, which
            // drops every tool — including delegated children (they run as
            // futures inside this tree) — so cancellation propagates downward.
            let results = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Err(RegentError::Interrupted),
                results = join_all(dispatches) => results,
            };
            for (call, result) in assistant.tool_calls.iter().zip(results) {
                let message = ChatMessage::tool_result(&call.id, &call.name, result);
                self.transcript.push(message.clone())?;
                self.persist(message, None, None).await?;
            }
        }
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

    pub(crate) async fn record_usage(&self, input: i64, output: i64) -> Result<(), RegentError> {
        let store = Arc::clone(&self.store);
        let session_id = self.session_id.clone();
        tokio::task::spawn_blocking(move || store.record_usage(&session_id, input, output))
            .await
            .map_err(|join_error| RegentError::Store(join_error.to_string()))??;
        Ok(())
    }
}
