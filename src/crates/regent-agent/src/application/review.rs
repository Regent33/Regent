//! The post-turn background review fork (the self-improvement loop):
//! after a successful turn, a whitelisted sub-agent replays a conversation
//! snapshot and persists learning through memory/skill tools. The main
//! conversation, its transcript, and its prompt cache are never touched;
//! the reviewer cannot recurse (it is built without a review setup).

use crate::application::agent::Agent;
use crate::domain::compression;
use crate::domain::config::{AgentConfig, CompressionConfig};
use regent_tools::ToolCatalog;
use std::sync::Arc;

pub struct ReviewSetup {
    /// Whitelist catalog — memory + skill tools only, by construction.
    pub catalog: Arc<ToolCatalog>,
    /// Reviewer system prompt (see `regent_skills::REVIEW_SYSTEM_PROMPT`).
    pub system_prompt: String,
    /// Reviews are bounded tighter than user turns.
    pub max_iterations: u32,
    /// Batch gate: a review spawns only once this many UNREVIEWED messages
    /// have accumulated, and it sees only that unreviewed slice. Without it,
    /// every turn forked a review replaying the whole transcript — the
    /// "review-session flood" (800 sessions / 30M tokens in 2 weeks).
    pub min_new_messages: usize,
}

impl Agent {
    /// Enables the learning loop. The caller composes the whitelist catalog
    /// and prompt (composition root owns DI).
    #[must_use]
    pub fn with_background_review(mut self, setup: ReviewSetup) -> Self {
        self.review = Some(Arc::new(setup));
        self
    }

    /// Last spawned review task — await it for graceful shutdown or tests.
    pub fn take_review_handle(&mut self) -> Option<tokio::task::JoinHandle<()>> {
        self.review_handle.take()
    }

    /// Called by `run_turn` after a successful turn. Fire-and-forget; the
    /// handle is kept so callers can await completion.
    pub(crate) fn spawn_review_if_configured(&mut self) {
        let Some(setup) = self.review.clone() else {
            return;
        };
        let messages = self.transcript.messages();
        // Clamp: a compression split can shrink the transcript below the mark.
        let start = self.reviewed_len.min(messages.len());
        let unreviewed = &messages[start..];
        // Batch gate (floor 2: nothing to learn from an empty exchange). The
        // below-threshold tail just waits for the next review-worthy turn.
        // ponytail: a tail smaller than the threshold at session end is never
        // reviewed — add a shutdown flush only if that loss ever matters.
        if unreviewed.len() < setup.min_new_messages.max(2) {
            return;
        }
        let snapshot = format!(
            "Conversation snapshot to review:\n\n{}\n\nReview per your instructions.",
            compression::render_for_summary(unreviewed)
        );
        self.reviewed_len = messages.len();
        let provider = Arc::clone(&self.provider);
        let store = Arc::clone(&self.store);
        let tool_context = self.tool_context.clone();
        let parent_session = self.session_id.clone();

        self.review_handle = Some(tokio::spawn(async move {
            let config = AgentConfig {
                max_iterations: setup.max_iterations,
                source: "review".to_owned(),
                compression: CompressionConfig {
                    enabled: false,
                    ..CompressionConfig::default()
                },
                ..AgentConfig::default()
            };
            let reviewer = Agent::new(
                provider,
                Arc::clone(&setup.catalog),
                store,
                tool_context,
                setup.system_prompt.clone(),
                config,
            );
            match reviewer {
                Ok(mut agent) => match agent.run_turn(&snapshot).await {
                    Ok(outcome) => tracing::info!(
                        parent = %parent_session, review = %agent.session_id(),
                        outcome = outcome.trim(), "background review complete"
                    ),
                    Err(error) => tracing::warn!(
                        parent = %parent_session, %error, "background review turn failed"
                    ),
                },
                Err(error) => {
                    tracing::warn!(parent = %parent_session, %error, "background review setup failed");
                }
            }
        }));
    }
}
