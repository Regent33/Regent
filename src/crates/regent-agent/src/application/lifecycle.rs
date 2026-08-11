//! Turn lifecycle bookkeeping: the reproducibility ledger and the
//! compression session-split. Kept apart from the loop itself for focus.

use crate::application::agent::Agent;
use crate::domain::compression;
use regent_kernel::{ChatMessage, RegentError, SessionId};
use regent_providers::ChatRequest;
use regent_store::StoreError;
use std::sync::Arc;

/// ADR-038 P3: the one-line identity re-anchor injected ahead of every
/// compaction summary (see `maybe_compress`). One sentence on purpose —
/// re-anchoring is a nudge back to the trained register, not a second
/// system prompt.
pub const IDENTITY_REANCHOR: &str = "[Identity re-anchor: you are Regent — continue in the \
     character, values, and voice your system prompt and constitution define.]";

impl Agent {
    /// Writes one row to the turns ledger. Recording failure is logged, not
    /// raised — it must never mask the turn's real result.
    pub(crate) async fn record_turn_outcome(
        &self,
        result: &Result<String, RegentError>,
        started_at: f64,
    ) {
        let (outcome, error) = match result {
            // Gap L2: a budget-exhausted turn returns Ok (the wrap-up summary)
            // but the ledger must still say what really happened.
            Ok(_) if self.last_turn_budget_exhausted => ("budget_exhausted", None),
            Ok(_) => ("ok", None),
            Err(RegentError::Interrupted) => ("interrupted", None),
            Err(RegentError::BudgetExhausted(_)) => ("budget_exhausted", None),
            Err(other) => ("error", Some(other.to_string())),
        };
        let store = Arc::clone(&self.store);
        let session_id = self.session_id.clone();
        let model = self.provider.model().to_owned();
        let api_calls = self.turn_api_calls;

        // The ledger has carried start/end since it shipped, but nothing ever
        // read it back, so "the app is slow" was undiagnosable from the logs.
        // On the owner's store, three days of turns put the median at 0.5s for
        // one model and 27.6s for another — a 55x spread that model choice
        // decides and no surface showed. One line at turn end makes the
        // dominant term visible where the complaint is made.
        let elapsed = (regent_store::now_epoch() - started_at).max(0.0);
        // The error goes in the LINE, not just the ledger. It was recorded in
        // `turns.error` from the start but omitted here, so a turn that died on
        // "provider failure: network error: request timed out" logged as a bare
        // `outcome="error"` and the reason could only be found by querying the
        // database — for a failure whose whole symptom is a chat that stops.
        // The breakdown rides this line rather than a `debug!` of its own: the
        // deacon's default filter is `info`, so a debug line would be absent
        // from regent.log exactly where the complaint gets made, and the fields
        // that give it context (session, model, api_calls, outcome) are here.
        let timings = self.timings;
        tracing::info!(
            session = %session_id,
            %model,
            api_calls,
            outcome,
            error = error.as_deref().unwrap_or(""),
            elapsed_ms = (elapsed * 1000.0) as u64,
            model_ms = timings.model_ms,
            tools_ms = timings.tools_ms,
            store_ms = timings.store_ms,
            compact_ms = timings.compact_ms,
            levers_ms = timings.levers_ms,
            "turn complete"
        );
        let recorded = tokio::task::spawn_blocking(move || {
            store.record_turn(
                &session_id,
                &model,
                api_calls,
                outcome,
                error.as_deref(),
                started_at,
            )
        })
        .await;
        match recorded {
            Ok(Ok(())) => {}
            Ok(Err(store_error)) => tracing::warn!(%store_error, "failed to record turn outcome"),
            Err(join_error) => tracing::warn!(%join_error, "turn record task failed"),
        }
    }

    /// Preflight compression: when the estimated prompt
    /// crosses the threshold, summarize the head, keep the newest messages
    /// verbatim, and split into a **child session** (lineage) — the original
    /// session is ended with reason "compressed" and never mutated.
    pub(crate) async fn maybe_compress(&mut self) -> Result<(), RegentError> {
        // `None` covers both "disabled by config" and Gap C4: once a pass failed
        // to shrink below threshold, another would just split sessions forever —
        // the breaker stays open. Same accessor the status meter reads, so the
        // marked threshold is always the enforced one.
        let Some(threshold) = self.compaction_threshold() else {
            return Ok(());
        };
        let settings = &self.config.compression;
        // The tool catalog is part of the request whether or not the transcript
        // knows it, so it counts toward the trigger: compaction has to leave
        // room for a fixed cost it can never shrink.
        let fixed = self.tool_schema_tokens();
        let estimate =
            compression::estimate_tokens(&self.system_prompt, self.transcript.messages()) + fixed;
        if estimate <= threshold {
            return Ok(());
        }
        // Nothing to summarize our way out of: the catalog alone does not fit.
        // Say that instead of spending a summarizer call to discover it and
        // then reporting "compaction ineffective", which names the wrong cause.
        if fixed >= threshold {
            self.compression_broken = true;
            tracing::warn!(
                tool_schema_tokens = fixed,
                threshold,
                window = self.effective_max_context(),
                "the tool catalog alone exceeds the compaction threshold — \
                 compaction cannot help; use a larger window or a smaller catalog"
            );
            return Ok(());
        }
        let Some((head, tail)) =
            compression::split_for_compression(self.transcript.messages(), settings.protect_last_n)
        else {
            return Ok(());
        };
        tracing::info!(
            estimate,
            threshold,
            summarized = head.len(),
            kept = tail.len(),
            "context compression triggered"
        );

        // One summarizer call on the same provider chain (auxiliary models
        // arrive with the memory milestone).
        let summary_request = ChatRequest::new(
            compression::SUMMARIZER_SYSTEM,
            vec![ChatMessage::user(compression::render_for_summary(&head))],
        );
        let summary_response = self.provider.complete(&summary_request).await?;
        self.account_usage(&summary_response.usage).await?;
        let summary_text = summary_response
            .message
            .content
            .unwrap_or_else(|| "(summarizer returned no text)".to_owned());

        // ADR-038 P3: one-line identity re-anchor at the compaction boundary.
        // A byte-stable identity block alone doesn't keep the model ATTENDING
        // to it as context grows (attention decay — the persona-drift
        // finding, plan §8.1 Q6); a single post-compaction re-anchor restores
        // the trained register. Token-free in cache terms: compaction resets
        // the cache here anyway.
        let summary_text = format!("{IDENTITY_REANCHOR}\n\n{summary_text}");

        let new_transcript = compression::rebuild_transcript(&summary_text, tail)?;

        // Gap C4 post-telemetry + circuit breaker: if the rewrite didn't get
        // us back under threshold (protected tail too fat, summary too long),
        // don't try again this session.
        let post_estimate =
            compression::estimate_tokens(&self.system_prompt, new_transcript.messages()) + fixed;
        if post_estimate > threshold {
            self.compression_broken = true;
            tracing::warn!(
                tokens_before = estimate,
                tokens_after = post_estimate,
                threshold,
                "compaction ineffective — circuit breaker open for this session"
            );
        }

        // Session split: child carries the same frozen prompt; parent points
        // back for lineage walks.
        let child_id = SessionId::generate();
        let store = Arc::clone(&self.store);
        let old_id = self.session_id.clone();
        let child_for_write = child_id.clone();
        let source = self.config.source.clone();
        let model = self.provider.model().to_owned();
        let system_prompt = self.system_prompt.clone();
        let messages = new_transcript.messages().to_vec();
        tokio::task::spawn_blocking(move || -> Result<(), StoreError> {
            store.create_session(
                &child_for_write,
                &source,
                Some(&model),
                Some(&system_prompt),
                Some(&old_id),
            )?;
            for message in &messages {
                store.append_message(&child_for_write, message, None, None)?;
            }
            store.end_session(&old_id, "compressed")
        })
        .await
        .map_err(|join_error| RegentError::Store(join_error.to_string()))??;

        // Episodic anchor: the summary that just left the context window
        // becomes a graph node tied to the parent session — recallable via
        // memory_search long after the transcript is gone.
        if let Some(graph) = self.graph.clone() {
            let old = self.session_id.clone();
            let summary = summary_text.clone();
            let episode =
                tokio::task::spawn_blocking(move || graph.record_episode(old.as_str(), &summary))
                    .await;
            match episode {
                Ok(Ok(node_id)) => tracing::info!(node_id, "episode recorded"),
                Ok(Err(error)) => tracing::warn!(%error, "episode capture failed"),
                Err(join_error) => tracing::warn!(%join_error, "episode task failed"),
            }
        }

        tracing::info!(
            parent = %self.session_id, child = %child_id,
            tokens_before = estimate, tokens_after = post_estimate,
            "session split complete"
        );
        // SPL cache-reset seam: compaction rewrites history wholesale — this turn
        // is full-price on the new child session. Overrides a pruning reason.
        self.note_cache_reset("compaction");
        self.session_id = child_id;
        self.transcript = new_transcript;
        Ok(())
    }
}
