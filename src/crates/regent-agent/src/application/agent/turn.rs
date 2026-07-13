//! The turn loop: assemble context → (interruptible) model call → execute
//! tools → observe → check stop conditions. Stop conditions (budget,
//! interrupt) are checked here, never left to the model. Compression runs as a
//! child-session split, never by mutating history.

use super::Agent;
use crate::domain::prompts::WRAP_UP_PROMPT;
use futures::future::join_all;
use regent_kernel::{ChatMessage, RegentError, tool_error_json};
use regent_providers::ChatRequest;
use std::sync::Arc;

/// Gap L1: synthetic tool result injected instead of dispatching the third
/// identical single-call batch in a row — the model gets steered, not looped.
const DOOM_LOOP_NUDGE: &str = "You have made this exact call 3 times in a row with identical \
arguments and identical results. Change your approach: use a different tool, different \
arguments, or explain to the user why you are stuck.";

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
        self.last_turn_budget_exhausted = false;
        self.last_turn_input_tokens = 0;
        self.last_turn_output_tokens = 0;
        self.last_turn_cache_read = None;
        self.last_turn_cache_write = None;
        // A routing-epoch provider swap (stamped by the deacon before the turn)
        // is the one reset cause that originates outside this loop — seed it now
        // so in-turn causes (pruning/compaction/failover) only override it if
        // higher priority (they aren't). `None` normally.
        self.last_cache_reset = self.pending_cache_reset.take();
        // Model the current provider answers as, captured before the loop: a
        // change mid-turn means the fallback chain failed over (SPL §3.2).
        let start_model = self.provider.model().to_owned();
        // Per-turn token spend, summed across model calls (W2.4 cost ceiling).
        let mut turn_tokens: u64 = 0;
        // Gap L1: the last two batches' shapes ((name, args) per call) —
        // enough to spot the third identical single-call in a row.
        let mut recent_batches: Vec<Vec<(String, String)>> = Vec::new();

        loop {
            if self.cancel.is_cancelled() {
                return Err(RegentError::Interrupted);
            }
            if self.turn_api_calls >= self.config.max_iterations {
                return self.budget_wrap_up().await;
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
                    "per-turn token ceiling reached — wrapping up turn"
                );
                return self.budget_wrap_up().await;
            }
            // History-side levers in tier order (§3.8 + gap C3): stub stale
            // tool results, then collapse older exchanges' fat arguments —
            // both push the compaction trigger below out further.
            self.maybe_prune();
            self.maybe_collapse();
            self.maybe_compress().await?;

            let mut request = ChatRequest::new(
                self.system_prompt.clone(),
                self.transcript.messages().to_vec(),
            )
            .with_tools(definitions.clone());
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
            self.last_turn_input_tokens = self
                .last_turn_input_tokens
                .saturating_add(response.usage.prompt_tokens);
            self.last_turn_output_tokens = self
                .last_turn_output_tokens
                .saturating_add(response.usage.completion_tokens);
            // SPL P2: sum provider-reported cache usage across the turn's calls.
            // Stays `None` until a call actually reports it (non-caching provider).
            if let Some(read) = response.usage.cache_read_tokens {
                self.last_turn_cache_read =
                    Some(self.last_turn_cache_read.unwrap_or(0).saturating_add(read));
            }
            if let Some(write) = response.usage.cache_write_tokens {
                self.last_turn_cache_write = Some(
                    self.last_turn_cache_write
                        .unwrap_or(0)
                        .saturating_add(write),
                );
            }
            // Sticky failover: the fallback chain swapped providers mid-turn, so
            // the new model's cache is cold — attribute this turn to failover.
            if self.provider.model() != start_model.as_str() {
                self.note_cache_reset("failover");
            }

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

            // Doom-loop guard (gap L1): the third identical single-call batch
            // in a row is not dispatched — a synthetic result steers the model
            // instead. The window stays saturated while it repeats, so every
            // further repeat gets the same nudge (a stubborn loop converges to
            // budget exhaustion, which wraps up gracefully above).
            let signature: Vec<(String, String)> = assistant
                .tool_calls
                .iter()
                .map(|c| (c.name.clone(), c.arguments.clone()))
                .collect();
            if signature.len() == 1
                && recent_batches.len() == 2
                && recent_batches.iter().all(|s| *s == signature)
            {
                tracing::warn!(
                    tool = signature[0].0,
                    "doom loop detected — skipping dispatch, nudging the model"
                );
                let call = &assistant.tool_calls[0];
                let message = ChatMessage::tool_result(
                    &call.id,
                    &call.name,
                    tool_error_json(DOOM_LOOP_NUDGE),
                );
                self.transcript.push(message.clone())?;
                self.persist(message, None, None).await?;
                continue;
            }
            recent_batches.push(signature);
            if recent_batches.len() > 2 {
                recent_batches.remove(0);
            }

            // Partitioned dispatch (gap L3): contiguous runs of read-only calls
            // execute in parallel; mutating calls execute serially, in call
            // order — two file_edits on the same file (or an edit racing the
            // build in `terminal`) must never interleave. Results re-attach in
            // original call order either way (runs execute in order; join_all
            // preserves input order within a run).
            let catalog = Arc::clone(&self.catalog);
            let ctx = self.tool_context.clone();
            let calls = &assistant.tool_calls;
            let dispatch_runs = async {
                let mut results: Vec<String> = Vec::with_capacity(calls.len());
                let mut start = 0;
                while start < calls.len() {
                    let read_only = regent_kernel::is_read_only_tool(&calls[start].name);
                    let mut end = start + 1;
                    while end < calls.len()
                        && regent_kernel::is_read_only_tool(&calls[end].name) == read_only
                    {
                        end += 1;
                    }
                    if read_only {
                        let dispatches = calls[start..end]
                            .iter()
                            .map(|call| catalog.dispatch(&call.name, &call.arguments, &ctx));
                        results.extend(join_all(dispatches).await);
                    } else {
                        for call in &calls[start..end] {
                            results.push(catalog.dispatch(&call.name, &call.arguments, &ctx).await);
                        }
                    }
                    start = end;
                }
                results
            };
            // Interruptible: a cancel drops the in-flight dispatch future, which
            // drops every tool — including delegated children (they run as
            // futures inside this tree) — so cancellation propagates downward.
            let results = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Err(RegentError::Interrupted),
                results = dispatch_runs => results,
            };
            for (call, result) in assistant.tool_calls.iter().zip(results) {
                let message = ChatMessage::tool_result(&call.id, &call.name, result);
                self.transcript.push(message.clone())?;
                self.persist(message, None, None).await?;
            }
        }
    }

    /// Gap L2: budget exhaustion ends the turn with a summary, not a hard
    /// error. One final model call — tool list EMPTY, so it cannot start new
    /// work and cannot recurse into the budget checks — asks for a wrap-up:
    /// done / remaining / where to resume. The turns ledger still records
    /// `budget_exhausted` (via the flag `record_turn_outcome` reads); if this
    /// last call itself fails, `run_turn`'s recovery drops the trailing
    /// wrap-up user message and the transcript stays legal.
    async fn budget_wrap_up(&mut self) -> Result<String, RegentError> {
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
