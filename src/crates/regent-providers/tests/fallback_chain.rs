use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::ChatMessage;
use regent_providers::domain::contracts::DeltaSink;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, FallbackChat, ProviderError};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};

/// Provider that fails `fail_first` times with `error_factory`, then answers.
struct Flaky {
    name: &'static str,
    calls: AtomicU32,
    fail_always: bool,
    error_factory: fn() -> ProviderError,
}

impl Flaky {
    fn failing_with(name: &'static str, error_factory: fn() -> ProviderError) -> Arc<Self> {
        Arc::new(Self {
            name,
            calls: AtomicU32::new(0),
            fail_always: true,
            error_factory,
        })
    }

    fn healthy(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            calls: AtomicU32::new(0),
            fail_always: false,
            error_factory: || ProviderError::Parse("unused".into()),
        })
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ChatProvider for Flaky {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_always {
            return Err((self.error_factory)());
        }
        Ok(ChatResponse {
            message: ChatMessage::assistant(Some(format!("answer from {}", self.name)), vec![]),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
        })
    }

    fn model(&self) -> &str {
        self.name
    }
}

fn request() -> ChatRequest {
    ChatRequest::new("system", vec![ChatMessage::user("hello")])
}

#[tokio::test]
async fn reroutes_when_primary_down_and_retries_it_every_call_to_recover() {
    let primary = Flaky::failing_with("primary", || ProviderError::Exhausted {
        attempts: 3,
        last: "503".into(),
    });
    let secondary = Flaky::healthy("secondary");
    let chain = FallbackChat::new(vec![primary.clone(), secondary.clone()]).unwrap();

    // Call 1: primary is down → reroute to the secondary.
    let first = chain.complete(&request()).await.unwrap();
    assert!(first.message.content.unwrap().contains("secondary"));

    // Call 2: RECOVERING (not sticky) — the primary is tried AGAIN first, so the
    // moment it comes back the chain returns to it. A sticky chain would have
    // pinned to the secondary and never retried the primary.
    chain.complete(&request()).await.unwrap();
    assert_eq!(primary.calls(), 2, "primary retried every call (recovering)");
    assert_eq!(secondary.calls(), 2);
}

#[tokio::test]
async fn rate_limited_primary_completes_on_fallback() {
    // 429 on the primary is transient → fail over and complete on the fallback.
    let primary = Flaky::failing_with("primary", || ProviderError::RateLimited);
    let secondary = Flaky::healthy("secondary");
    let chain = FallbackChat::new(vec![primary.clone(), secondary.clone()]).unwrap();

    let response = chain.complete(&request()).await.unwrap();
    assert!(response.message.content.unwrap().contains("secondary"));
    assert_eq!(primary.calls(), 1, "primary attempted once");
    assert_eq!(secondary.calls(), 1, "fallback served the answer");
}

#[tokio::test]
async fn auth_errors_fail_over_but_client_errors_do_not() {
    let bad_key = Flaky::failing_with("bad-key", || ProviderError::Auth { status: 401 });
    let healthy = Flaky::healthy("backup");
    let chain = FallbackChat::new(vec![bad_key, healthy]).unwrap();
    assert!(chain.complete(&request()).await.is_ok());

    let bad_request = Flaky::failing_with("bad-request", || ProviderError::Api {
        status: 400,
        body: "malformed".into(),
    });
    let never_reached = Flaky::healthy("unreachable");
    let chain = FallbackChat::new(vec![bad_request, never_reached.clone()]).unwrap();
    let error = chain.complete(&request()).await.unwrap_err();
    assert!(matches!(error, ProviderError::Api { status: 400, .. }));
    assert_eq!(never_reached.calls(), 0, "4xx must not trigger failover");
}

#[tokio::test]
async fn whole_chain_down_returns_last_error_and_empty_chain_rejected() {
    let a = Flaky::failing_with("a", || ProviderError::RateLimited);
    let b = Flaky::failing_with("b", || ProviderError::Network("refused".into()));
    let chain = FallbackChat::new(vec![a, b]).unwrap();
    let error = chain.complete(&request()).await.unwrap_err();
    assert!(matches!(error, ProviderError::Network(_)));

    assert!(FallbackChat::new(vec![]).is_err());
}

/// Emits one delta, then fails mid-stream — models a provider that dropped
/// after text already reached the user.
struct MidStreamFail {
    name: &'static str,
}

#[async_trait]
impl ChatProvider for MidStreamFail {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Err(ProviderError::Network("mid-stream provider has no unary path".into()))
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
async fn streaming_fails_over_before_any_delta_is_emitted() {
    // Primary fails before streaming a single fragment → safe to reroute; the
    // fallback's whole reply streams through (default streaming emits it once).
    let primary = Flaky::failing_with("primary", || ProviderError::RateLimited);
    let secondary = Flaky::healthy("secondary");
    let chain = FallbackChat::new(vec![primary.clone(), secondary.clone()]).unwrap();

    let seen = Mutex::new(String::new());
    let sink = |fragment: &str| seen.lock().unwrap().push_str(fragment);
    let response = chain.complete_streaming(&request(), &sink).await.unwrap();

    assert!(response.message.content.unwrap().contains("secondary"));
    assert!(seen.lock().unwrap().contains("secondary"), "fallback streamed");
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
    let error = chain.complete_streaming(&request(), &sink).await.unwrap_err();

    assert!(matches!(error, ProviderError::Network(_)));
    assert_eq!(seen.lock().unwrap().as_str(), "partial ", "the pre-failure delta reached the sink");
    assert_eq!(
        secondary.calls(),
        0,
        "no failover once a delta was emitted",
    );
}
