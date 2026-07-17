//! ADR-038 P0a: the effective context window (compaction preflight + context
//! meter) follows the ACTIVE model, and a provider swap changes it on the next
//! read — a failover / routing-epoch to a smaller model must not keep the
//! primary's (or the config default's) math.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{DenyAll, ToolCatalog, ToolContext};
use std::sync::Arc;

/// A provider that reports a fixed model id and never actually answers a turn —
/// these tests only read `context_usage`, which needs no model call.
struct FixedModel(&'static str);

#[async_trait]
impl ChatProvider for FixedModel {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Ok(ChatResponse {
            message: ChatMessage::assistant(Some("ok".into()), vec![]),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
        })
    }

    fn model(&self) -> &str {
        self.0
    }
}

fn agent_with(model: &'static str) -> Agent {
    let store = Arc::new(Store::open_in_memory().unwrap());
    Agent::new(
        Arc::new(FixedModel(model)),
        Arc::new(ToolCatalog::new()),
        store,
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)),
        "system",
        AgentConfig::default(),
    )
    .unwrap()
}

#[test]
fn context_usage_follows_a_known_model_not_the_config_default() {
    // claude family → 200k, distinct from the 128k config default.
    let agent = agent_with("claude-sonnet-4-5");
    assert_eq!(agent.context_usage().1, 200_000);
}

#[test]
fn unknown_model_falls_back_to_the_config_default() {
    let agent = agent_with("some-unlisted-model");
    assert_eq!(
        agent.context_usage().1,
        AgentConfig::default().max_context_tokens
    );
}

#[test]
fn config_override_beats_the_family_table() {
    // The user pinned this exact model id in `context.windows` — their number
    // wins over the built-in claude→200k family entry (the stale-table escape
    // hatch), and an override for a table-unknown local model works the same.
    let store = Arc::new(Store::open_in_memory().unwrap());
    let config = AgentConfig {
        context_windows: [
            ("claude-sonnet-4-5".to_owned(), 1_000_000),
            ("llama-3.1-70b".to_owned(), 32_000),
        ]
        .into(),
        ..AgentConfig::default()
    };
    let mut agent = Agent::new(
        Arc::new(FixedModel("claude-sonnet-4-5")),
        Arc::new(ToolCatalog::new()),
        store,
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)),
        "system",
        config,
    )
    .unwrap();
    assert_eq!(agent.context_usage().1, 1_000_000);

    // Failover-aware: the override map is keyed on the ACTIVE model per read.
    agent.set_provider(Arc::new(FixedModel("llama-3.1-70b")));
    assert_eq!(agent.context_usage().1, 32_000);
}

#[test]
fn provider_swap_changes_the_effective_window() {
    // Primary: a 2M-window model.
    let mut agent = agent_with("gemini-1.5-pro");
    assert_eq!(agent.context_usage().1, 2_000_000);

    // Routing-epoch / failover swaps in a smaller (200k) model — the window
    // must follow it on the next read, not stay at 2M or fall to the default.
    agent.set_provider(Arc::new(FixedModel("claude-3-haiku")));
    assert_eq!(agent.context_usage().1, 200_000);
}

// A single message no window can hold fails FAST with an actionable message
// (no provider call, no 400 round-trip); one that fits proceeds normally.
#[tokio::test]
async fn a_message_larger_than_the_window_fails_fast_with_guidance() {
    // Unknown model → the 128k config default is the effective window.
    let mut agent = agent_with("some-unlisted-model");
    let window = AgentConfig::default().max_context_tokens as usize;

    // Just over the window (chars/4 estimate): rejected before any API call.
    let giant = "x".repeat((window + 1) * 4);
    let err = agent.run_turn(&giant).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("context window"), "names the cause: {msg}");
    assert!(msg.contains("file"), "offers the file route: {msg}");

    // A normal-sized message on the same agent still completes.
    assert_eq!(agent.run_turn("hello").await.unwrap(), "ok");
}
