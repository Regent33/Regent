use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, FallbackChat, ProviderError};
use std::sync::Arc;
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
