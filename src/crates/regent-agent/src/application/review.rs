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
        let Some(setup) = self.review.clone() else { return };
        // Nothing to learn from an empty/failed exchange.
        if self.transcript.messages().len() < 2 {
            return;
        }
        let snapshot = format!(
            "Conversation snapshot to review:\n\n{}\n\nReview per your instructions.",
            compression::render_for_summary(self.transcript.messages())
        );
        let provider = Arc::clone(&self.provider);
        let store = Arc::clone(&self.store);
        let tool_context = self.tool_context.clone();
        let parent_session = self.session_id.clone();

        self.review_handle = Some(tokio::spawn(async move {
            let config = AgentConfig {
                max_iterations: setup.max_iterations,
                source: "review".to_owned(),
                compression: CompressionConfig { enabled: false, ..CompressionConfig::default() },
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
