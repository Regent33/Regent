//! SPL P4/§3.1: reveal-on-stuck grows the tool catalog mid-session, which is a
//! deliberate Tier-0 definitions change — not a regression. It must therefore be
//! ATTRIBUTED (`cache_reset: "tiering"` on the turn that reveals) and the
//! stable-prefix baseline must be rebased, or `turn_prefix_hashes` reports the
//! same `cache_bust: tool_definitions` on every remaining turn of the session
//! (observed live on 2026-07-25, session 9720429d…: four identical WARNs).

use crate::helpers::{ScriptedProvider, make_session_manager};
use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// The reasoning-only dead-end that fires `reveal_all_deferred`: no text, no
/// tool call, only private reasoning.
fn reasoning_only() -> ChatResponse {
    let mut message = ChatMessage::assistant(None, vec![]);
    message.reasoning = Some("I should save the key, but that tool is hidden.".into());
    ChatResponse {
        message,
        usage: TokenUsage::default(),
        finish_reason: Some("stop".into()),
    }
}

#[tokio::test]
async fn revealing_deferred_tools_is_attributed_and_rebases_the_baseline() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![
        // Turn 1: stuck → reveal → retry answers.
        reasoning_only(),
        ScriptedProvider::text_reply("revealed and answered"),
        // Turns 2 and 3 run on the now-larger catalog.
        ScriptedProvider::text_reply("two"),
        ScriptedProvider::text_reply("three"),
    ]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());
    let sid = sm.create_session().await.unwrap();
    // A routing reset may already be pending when the weak model then reveals
    // tools. Tiering must win attribution, or the deacon cannot know to rebase.
    sm.set_model("routed-script");

    sm.run_turn(&sid, "save my groq key").await.unwrap();
    assert_eq!(
        sm.last_turn_cache_reset(&sid).await,
        Some("tiering"),
        "the turn that reveals deferred tools must explain its own full-price prompt"
    );

    // The reveal is now the baseline: this call runs the fail-open check, and a
    // rebased ledger reports no bust for the intentional definitions growth.
    let after_reveal = sm.turn_prefix_hashes(&sid).await.expect("known session");

    sm.run_turn(&sid, "two").await.unwrap();
    assert_eq!(
        sm.last_turn_cache_reset(&sid).await,
        None,
        "the reveal is one-way: no repeated attribution on later turns"
    );
    let later = sm.turn_prefix_hashes(&sid).await.expect("known session");
    assert_eq!(
        later, after_reveal,
        "post-reveal tier hashes are stable — the revealed catalog IS the new prefix"
    );
}

struct FallibleProvider {
    responses: Mutex<VecDeque<Result<ChatResponse, ProviderError>>>,
}

#[async_trait]
impl ChatProvider for FallibleProvider {
    async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("script exhausted")
    }

    fn model(&self) -> &str {
        "fallible-script"
    }
}

#[tokio::test]
async fn failed_reveal_turn_rebases_before_the_next_turn_resets_attribution() {
    let dir = TempDir::new().unwrap();
    let provider: Arc<dyn ChatProvider> = Arc::new(FallibleProvider {
        responses: Mutex::new(
            vec![
                Ok(reasoning_only()),
                Err(ProviderError::Network("offline".into())),
                Ok(ScriptedProvider::text_reply("recovered next turn")),
            ]
            .into(),
        ),
    });
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());
    let sid = sm.create_session().await.unwrap();
    let before = sm.turn_prefix_hashes(&sid).await.unwrap();

    assert!(sm.run_turn(&sid, "save my key").await.is_err());
    assert_eq!(sm.last_turn_cache_reset(&sid).await, Some("tiering"));

    // Starting the next turn clears last_cache_reset. Therefore the ledger must
    // already have been rebased by run_turn itself, not lazily by telemetry.
    sm.run_turn(&sid, "try again").await.unwrap();
    assert_eq!(sm.last_turn_cache_reset(&sid).await, None);
    let after = sm.turn_prefix_hashes(&sid).await.unwrap();
    assert_ne!(
        after, before,
        "the revealed definitions must become the baseline even when their turn failed"
    );
}
