//! Turn lifecycle bookkeeping: the reproducibility ledger and the
//! compression session-split. Kept apart from the loop itself for focus.

use crate::application::agent::Agent;
use crate::domain::compression;
use regent_kernel::{ChatMessage, RegentError, SessionId};
use regent_providers::ChatRequest;
use regent_store::StoreError;
use std::sync::Arc;

impl Agent {
    /// Writes one row to the turns ledger. Recording failure is logged, not
    /// raised — it must never mask the turn's real result.
    pub(crate) async fn record_turn_outcome(
        &self,
        result: &Result<String, RegentError>,
        started_at: f64,
    ) {
        let (outcome, error) = match result {
            Ok(_) => ("ok", None),
            Err(RegentError::Interrupted) => ("interrupted", None),
            Err(RegentError::BudgetExhausted(_)) => ("budget_exhausted", None),
            Err(other) => ("error", Some(other.to_string())),
        };
        let store = Arc::clone(&self.store);
        let session_id = self.session_id.clone();
        let model = self.provider.model().to_owned();
        let api_calls = self.turn_api_calls;
        let recorded = tokio::task::spawn_blocking(move || {
            store.record_turn(&session_id, &model, api_calls, outcome, error.as_deref(), started_at)
        })
        .await;
        match recorded {
            Ok(Ok(())) => {}
            Ok(Err(store_error)) => tracing::warn!(%store_error, "failed to record turn outcome"),
            Err(join_error) => tracing::warn!(%join_error, "turn record task failed"),
        }
    }

    /// Preflight compression (Hermes semantics): when the estimated prompt
    /// crosses the threshold, summarize the head, keep the newest messages
    /// verbatim, and split into a **child session** (lineage) — the original
    /// session is ended with reason "compressed" and never mutated.
    pub(crate) async fn maybe_compress(&mut self) -> Result<(), RegentError> {
        let settings = &self.config.compression;
        if !settings.enabled {
            return Ok(());
        }
        let estimate = compression::estimate_tokens(&self.system_prompt, self.transcript.messages());
        let threshold =
            (self.config.max_context_tokens as f64 * f64::from(settings.trigger_fraction)) as u32;
        if estimate <= threshold {
            return Ok(());
        }
        let Some((head, tail)) =
            compression::split_for_compression(self.transcript.messages(), settings.protect_last_n)
        else {
            return Ok(());
        };
        tracing::info!(estimate, threshold, summarized = head.len(), kept = tail.len(),
                       "context compression triggered");

        // One summarizer call on the same provider chain (auxiliary models
        // arrive with the memory milestone).
        let summary_request = ChatRequest::new(
            compression::SUMMARIZER_SYSTEM,
            vec![ChatMessage::user(compression::render_for_summary(&head))],
        );
        let summary_response = self.provider.complete(&summary_request).await?;
        self.record_usage(
            i64::from(summary_response.usage.prompt_tokens),
            i64::from(summary_response.usage.completion_tokens),
        )
        .await?;
        let summary_text = summary_response
            .message
            .content
            .unwrap_or_else(|| "(summarizer returned no text)".to_owned());

        let new_transcript = compression::rebuild_transcript(&summary_text, tail)?;

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
            let episode = tokio::task::spawn_blocking(move || {
                graph.record_episode(old.as_str(), &summary)
            })
            .await;
            match episode {
                Ok(Ok(node_id)) => tracing::info!(node_id, "episode recorded"),
                Ok(Err(error)) => tracing::warn!(%error, "episode capture failed"),
                Err(join_error) => tracing::warn!(%join_error, "episode task failed"),
            }
        }

        tracing::info!(parent = %self.session_id, child = %child_id, "session split complete");
        self.session_id = child_id;
        self.transcript = new_transcript;
        Ok(())
    }
}
