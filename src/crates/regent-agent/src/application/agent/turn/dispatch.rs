//! Tool dispatch for one assistant batch: the doom-loop guard, partitioned
//! (parallel-read / serial-write) execution, result persistence, and the
//! one-shot deferred-tool reveal after a failed call.

use super::output_check::RetryState;
use super::turn_support;
use crate::application::agent::Agent;
use regent_kernel::{ChatMessage, RegentError, ToolDefinition, tool_error_json};

/// Gap L1: synthetic tool result injected instead of dispatching the third
/// identical single-call batch in a row — the model gets steered, not looped.
const DOOM_LOOP_NUDGE: &str = "You have made this exact call 3 times in a row with identical \
arguments. Change your approach: use a different tool, different arguments, or explain to \
the user why you are stuck.";

impl Agent {
    /// Executes one assistant message's tool calls and records their results.
    /// The doom-loop guard (gap L1) answers the third identical single-call
    /// batch in a row with a synthetic nudge instead of dispatching it — the
    /// window stays saturated while it repeats, so every further repeat gets
    /// the same nudge (a stubborn loop converges to budget exhaustion, which
    /// wraps up gracefully in the caller).
    pub(super) async fn dispatch_tools(
        &mut self,
        assistant: &ChatMessage,
        recent_batches: &mut Vec<Vec<(String, String)>>,
        definitions: &mut Vec<ToolDefinition>,
        retry: &mut RetryState,
    ) -> Result<(), RegentError> {
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
            let message =
                ChatMessage::tool_result(&call.id, &call.name, tool_error_json(DOOM_LOOP_NUDGE));
            self.transcript.push(message.clone())?;
            self.persist(message, None, None).await?;
            return Ok(());
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
        let calls = &assistant.tool_calls;
        let dispatch_runs =
            turn_support::dispatch_partitioned(&self.catalog, &self.tool_context, calls);
        // Interruptible: a cancel drops the in-flight dispatch future, which
        // drops every tool — including delegated children (they run as
        // futures inside this tree) — so cancellation propagates downward.
        let results = tokio::select! {
            biased;
            () = self.cancel.cancelled() => return Err(RegentError::Interrupted),
            results = dispatch_runs => results,
        };
        let mut tool_error_seen = false;
        for (call, result) in assistant.tool_calls.iter().zip(results) {
            tool_error_seen |= serde_json::from_str::<serde_json::Value>(&result)
                .ok()
                .is_some_and(|value| value.get("error").is_some());
            let message = ChatMessage::tool_result(&call.id, &call.name, result);
            self.transcript.push(message.clone())?;
            self.persist(message, None, None).await?;
        }
        if tool_error_seen && !retry.error_recovery_attempted {
            retry.error_recovery_attempted = true;
            let revealed = self.catalog.reveal_all_deferred();
            if revealed > 0 {
                *definitions = self.catalog.definitions();
                // Same deliberate Tier-0 change as the reasoning-only reveal —
                // attribute it so the ledger baseline is rebased once.
                self.note_cache_reset("tiering");
                tracing::warn!(
                    model = self.provider.model(),
                    revealed,
                    "tool call failed — revealing deferred tools for the next model iteration"
                );
            }
        }
        Ok(())
    }
}
