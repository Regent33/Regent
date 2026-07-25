//! The turn loop: assemble context → (interruptible) model call → execute
//! tools → observe → check stop conditions. Stop conditions (budget,
//! interrupt) are checked here, never left to the model. Compression runs as a
//! child-session split, never by mutating history.
//! Split across: model_call.rs (request + usage), output_check.rs (repair
//! retries), dispatch.rs (doom-loop guard + tool execution).

use super::Agent;
use output_check::{OutputCheck, RetryState};
use regent_kernel::{ChatMessage, RegentError};

mod dispatch;
mod model_call;
mod output_check;
mod turn_support;
mod wrap_up;

impl Agent {
    /// Runs one user turn and records its outcome in the turns ledger. On
    /// success — or an interrupted turn that left a partial tool exchange — the
    /// background review fork (if configured) is spawned, fire-and-forget,
    /// never blocking the reply.
    pub async fn run_turn(&mut self, user_text: &str) -> Result<String, RegentError> {
        // A single message no window can hold fails HERE, with an actionable
        // message, instead of burning a provider call on a guaranteed 400 —
        // compaction can't shrink one message it must protect. Same chars/4
        // convention as the compression preflight.
        let input_est = user_text.len() as u64 / 4;
        let max = self.effective_max_context();
        if input_est > u64::from(max) {
            return Err(RegentError::Input(format!(
                "input is ~{}k tokens but {}'s context window is ~{}k — save it \
                 to a file and point me at it (I read documents in slices), or \
                 split it across messages",
                input_est / 1000,
                self.provider.model(),
                max / 1000
            )));
        }
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
        // `mut` only so the reasoning-only recovery can rebuild it after
        // revealing deferred tools (the rare stuck path; the happy path never
        // reassigns it, so the cached tool-list prefix is untouched).
        let mut definitions = self.catalog.definitions();
        self.turn_api_calls = 0;
        self.last_turn_budget_exhausted = false;
        self.last_turn_input_tokens = 0;
        self.last_turn_output_tokens = 0;
        self.last_turn_cache_read = None;
        self.last_turn_cache_write = None;
        // A routing-epoch provider swap (stamped by the deacon before the turn)
        // is the one reset cause that originates outside this loop — seed it now.
        // Only tiering may override it: a definitions reveal must remain visible
        // so the deacon can rebase its ledger. `None` normally.
        self.last_cache_reset = self.pending_cache_reset.take();
        // Model the current provider answers as, captured before the loop: a
        // change mid-turn means the fallback chain failed over (SPL §3.2).
        let start_model = self.provider.model().to_owned();
        // Per-turn token spend, summed across model calls (W2.4 cost ceiling).
        let mut turn_tokens: u64 = 0;
        // Gap L1: the last two batches' shapes ((name, args) per call) —
        // enough to spot the third identical single-call in a row.
        let mut recent_batches: Vec<Vec<(String, String)>> = Vec::new();
        let mut retry = RetryState::new();

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

            let response = self.call_model(&definitions, &retry, &start_model).await?;
            turn_tokens += u64::from(response.usage.prompt_tokens)
                + u64::from(response.usage.completion_tokens);

            let assistant = response.message;
            match self.check_model_output(&assistant, &mut definitions, &mut retry)? {
                OutputCheck::Retry => continue,
                OutputCheck::Proceed => {}
            }
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

            self.dispatch_tools(
                &assistant,
                &mut recent_batches,
                &mut definitions,
                &mut retry,
            )
            .await?;
        }
    }
}
