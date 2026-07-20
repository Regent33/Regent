//! Streaming failover semantics and per-member context-window delegation.

use crate::{Flaky, request};
use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::ChatMessage;
use regent_providers::domain::contracts::DeltaSink;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, FallbackChat, ProviderError};
use std::sync::{Arc, Mutex};

/// Emits one delta, then fails mid-stream — models a provider that dropped
/// after text already reached the user.
struct MidStreamFail {
    name: &'static str,
}

#[async_trait]
impl ChatProvider for MidStreamFail {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Err(ProviderError::Network(
            "mid-stream provider has no unary path".into(),
        ))
    }

    async fn complete_streaming(
        &self,
        _request: &ChatRequest,
        on_delta: DeltaSink<'_>,
    ) -> Result<ChatResponse, ProviderError> {
        on_delta("partial ");
        Err(ProviderError::Network("dropped mid-stream".into()))
    }

    fn model(&self) -> &str {
        self.name
    }
}

#[tokio::test]
async fn streaming_empty_response_fails_over_before_any_delta() {
    // Empty streamed answer (nothing emitted) reroutes exactly like the unary
    // path; the fallback's real reply streams through.
    let empty = Flaky::empty("nemotron");
    let healthy = Flaky::healthy("glm");
    let chain = FallbackChat::new(vec![empty.clone(), healthy.clone()]).unwrap();

    let seen = Mutex::new(String::new());
    let sink = |fragment: &str| seen.lock().unwrap().push_str(fragment);
    let response = chain.complete_streaming(&request(), &sink).await.unwrap();

    assert!(response.message.content.unwrap().contains("glm"));
    assert!(seen.lock().unwrap().contains("glm"), "fallback streamed");
    assert_eq!(empty.calls(), 1);
}

#[tokio::test]
async fn streaming_fails_over_before_any_delta_is_emitted() {
    // Primary fails before streaming a single fragment → safe to reroute; the
    // fallback's whole reply streams through (default streaming emits it once).
    let primary = Flaky::failing_with("primary", || ProviderError::RateLimited {
        retry_after_ms: None,
    });
    let secondary = Flaky::healthy("secondary");
    let chain = FallbackChat::new(vec![primary.clone(), secondary.clone()]).unwrap();

    let seen = Mutex::new(String::new());
    let sink = |fragment: &str| seen.lock().unwrap().push_str(fragment);
    let response = chain.complete_streaming(&request(), &sink).await.unwrap();

    assert!(response.message.content.unwrap().contains("secondary"));
    assert!(
        seen.lock().unwrap().contains("secondary"),
        "fallback streamed"
    );
    assert_eq!(secondary.calls(), 1);
}

#[tokio::test]
async fn streaming_does_not_fail_over_once_a_delta_was_emitted() {
    // Primary streams a fragment THEN fails → re-running on the fallback would
    // duplicate the already-delivered text, so the error surfaces instead.
    let primary = Arc::new(MidStreamFail { name: "primary" });
    let secondary = Flaky::healthy("secondary");
    let chain = FallbackChat::new(vec![primary, secondary.clone()]).unwrap();

    let seen = Mutex::new(String::new());
    let sink = |fragment: &str| seen.lock().unwrap().push_str(fragment);
    let error = chain
        .complete_streaming(&request(), &sink)
        .await
        .unwrap_err();

    assert!(matches!(error, ProviderError::Network(_)));
    assert_eq!(
        seen.lock().unwrap().as_str(),
        "partial ",
        "the pre-failure delta reached the sink"
    );
    assert_eq!(secondary.calls(), 0, "no failover once a delta was emitted",);
}

#[tokio::test]
async fn context_window_follows_the_active_chain_member() {
    /// Provider with its own `context_window` — proves the chain delegates to
    /// the ACTIVE member instead of the trait default's static-table lookup
    /// (which would ignore a member's discovered/override window entirely).
    struct Windowed {
        name: &'static str,
        window: u32,
        fail: bool,
    }

    #[async_trait]
    impl ChatProvider for Windowed {
        async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            if self.fail {
                return Err(ProviderError::Exhausted {
                    attempts: 1,
                    last: "down".into(),
                });
            }
            Ok(ChatResponse {
                message: ChatMessage::assistant(Some("ok".into()), vec![]),
                usage: TokenUsage::default(),
                finish_reason: Some("stop".into()),
            })
        }

        fn model(&self) -> &str {
            self.name
        }

        fn context_window(&self) -> Option<u32> {
            Some(self.window)
        }
    }

    let chain = FallbackChat::new(vec![
        Arc::new(Windowed {
            name: "big-primary",
            window: 1_000_000,
            fail: true,
        }),
        Arc::new(Windowed {
            name: "small-local",
            window: 32_000,
            fail: false,
        }),
    ])
    .unwrap();

    // Fresh chain: the primary is active, so its window is reported.
    assert_eq!(chain.context_window(), Some(1_000_000));

    // Primary dies, the chain reroutes — compaction math must follow the
    // 32k survivor on the very next read, not keep 1M math (ADR-038 P0a).
    chain.complete(&request()).await.unwrap();
    assert_eq!(chain.context_window(), Some(32_000));
}
