use super::*;
use crate::domain::errors::DeaconError;
use crate::infra::http_listener::ChatReply;
use async_trait::async_trait;
use axum::http::Request;
use regent_gateway::{AuthSnapshot, GatewayError, MessageEvent, RateLimiter};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;

// Auth fixtures: `allow_all` runs turns for the happy-path tests; the default
// snapshot is default-deny (the P0-001 gate). Persist target is a temp dir.
fn allow_all_auth() -> Arc<AuthPolicy> {
    Arc::new(AuthPolicy::new(AuthSnapshot {
        allow_all: true,
        ..Default::default()
    }))
}
fn deny_auth() -> Arc<AuthPolicy> {
    Arc::new(AuthPolicy::new(AuthSnapshot::default()))
}
fn test_home() -> Arc<PathBuf> {
    Arc::new(std::env::temp_dir())
}
fn test_rate() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::per_minute(0)) // unlimited in tests
}

/// A `ChatService` that counts `chat_keyed` calls — proves whether a turn ran.
struct CountingChat(Arc<AtomicUsize>);
#[async_trait]
impl ChatService for CountingChat {
    async fn chat(&self, _s: Option<String>, _m: String) -> Result<ChatReply, DeaconError> {
        Ok(ChatReply {
            session: "s".into(),
            reply: "ok".into(),
        })
    }
    async fn chat_keyed(
        &self,
        _key: &str,
        _message: String,
    ) -> Result<ChatReply, DeaconError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(ChatReply {
            session: "s".into(),
            reply: "ok".into(),
        })
    }
}

struct StubAdapter;
impl WebhookAdapter for StubAdapter {
    fn platform(&self) -> &str {
        "stub"
    }
    fn verify(&self, _b: &[u8], signature: Option<&str>, _t: Option<&str>) -> bool {
        signature == Some("good")
    }
    fn parse_webhook(&self, _b: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        Ok(vec![MessageEvent {
            platform: "stub".into(),
            chat_id: "c1".into(),
            user_id: "c1".into(),
            text: "hi".into(),
        }])
    }
    fn send_request(&self, m: &OutboundMessage) -> SendRequest {
        // Loopback:1 fails fast — the background deliver is not asserted on.
        SendRequest {
            url: "http://127.0.0.1:1/x".into(),
            auth: SendAuth::None,
            body: SendBody::Json(json!({"t": m.text})),
        }
    }
    fn signature_header(&self) -> Option<&str> {
        Some("x-stub-sig")
    }
    fn verify_get(&self, query: &str) -> Option<String> {
        query.strip_prefix("echo=").map(ToOwned::to_owned)
    }
}

/// Like `StubAdapter` but replies synchronously (Teams/Google Chat shape).
struct SyncStubAdapter;
impl WebhookAdapter for SyncStubAdapter {
    fn platform(&self) -> &str {
        "sync"
    }
    fn verify(&self, _b: &[u8], signature: Option<&str>, _t: Option<&str>) -> bool {
        signature == Some("good")
    }
    fn parse_webhook(&self, _b: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        Ok(vec![MessageEvent {
            platform: "sync".into(),
            chat_id: "c1".into(),
            user_id: "c1".into(),
            text: "hi".into(),
        }])
    }
    fn send_request(&self, _m: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: String::new(),
            auth: SendAuth::None,
            body: SendBody::Json(json!({})),
        }
    }
    fn signature_header(&self) -> Option<&str> {
        Some("x-stub-sig")
    }
    fn sync_reply(&self) -> bool {
        true
    }
}

struct StubChat;
#[async_trait]
impl ChatService for StubChat {
    async fn chat(&self, _s: Option<String>, _m: String) -> Result<ChatReply, DeaconError> {
        Ok(ChatReply {
            session: "s".into(),
            reply: "ok".into(),
        })
    }
}

fn app() -> Router {
    let mut reg = Registry::new();
    reg.insert("stub".into(), Arc::new(StubAdapter));
    router(reg, Arc::new(StubChat), allow_all_auth(), test_home(), test_rate())
}

async fn status(sig: Option<&str>, path: &str) -> StatusCode {
    let mut b = Request::post(path);
    if let Some(s) = sig {
        b = b.header("x-stub-sig", s);
    }
    app()
        .oneshot(b.body(axum::body::Body::from("{}")).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn valid_signature_is_accepted() {
    assert_eq!(status(Some("good"), "/webhook/stub").await, StatusCode::OK);
}

#[tokio::test]
async fn bad_or_missing_signature_is_rejected() {
    assert_eq!(
        status(Some("bad"), "/webhook/stub").await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status(None, "/webhook/stub").await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn unknown_platform_is_not_found() {
    assert_eq!(
        status(Some("good"), "/webhook/nope").await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn get_handshake_echoes_or_rejects() {
    let app = app();
    // Valid challenge → 200 with the echoed body.
    let resp = app
        .clone()
        .oneshot(
            Request::get("/webhook/stub?echo=hi")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"hi");
    // No challenge → 401; unknown platform → 404.
    let reject = app
        .clone()
        .oneshot(
            Request::get("/webhook/stub")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reject.status(), StatusCode::UNAUTHORIZED);
    let missing = app
        .oneshot(
            Request::get("/webhook/nope?echo=x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

fn delivery_with_stub() -> WebhookPlatformDelivery {
    let mut adapters = Registry::new();
    adapters.insert("stub".into(), Arc::new(StubAdapter));
    WebhookPlatformDelivery {
        adapters,
        file_senders: HashMap::new(),
        client: reqwest::Client::new(),
    }
}

#[test]
fn sink_for_resolves_known_platforms_and_rejects_the_rest() {
    let delivery = delivery_with_stub();
    // Known platform → a sink bound to that conversation's target.
    let sink = delivery
        .sink_for("stub:c1")
        .expect("known platform resolves");
    assert_eq!(sink.targets(), vec!["stub:c1".to_owned()]);
    // Unknown platform and malformed keys → no sink (falls back to CLI).
    assert!(delivery.sink_for("nope:c1").is_none());
    assert!(delivery.sink_for("nocolon").is_none());
}

#[tokio::test]
async fn file_send_declines_when_the_platform_has_no_uploader() {
    let sink = delivery_with_stub().sink_for("stub:c1").unwrap();
    let err = sink
        .deliver_file("", std::path::Path::new("x.txt"), "")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not supported on stub"));
}

#[tokio::test]
async fn sync_reply_returns_the_reply_in_the_response_body() {
    let mut reg = Registry::new();
    reg.insert("sync".into(), Arc::new(SyncStubAdapter));
    let app = router(reg, Arc::new(StubChat), allow_all_auth(), test_home(), test_rate());
    let req = Request::post("/webhook/sync")
        .header("x-stub-sig", "good")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // StubChat replies "ok"; the default sync_response wraps it as {"text": …}.
    assert_eq!(body["text"], "ok");
}

#[tokio::test]
async fn unauthorized_sender_gets_pairing_prompt_and_runs_no_turn() {
    // Signature-valid but UNauthorized sender → pairing prompt, no turn.
    // This is the P0-001 regression guard: default-deny on the webhook plane.
    let calls = Arc::new(AtomicUsize::new(0));
    let mut reg = Registry::new();
    reg.insert("sync".into(), Arc::new(SyncStubAdapter));
    let app = router(
        reg,
        Arc::new(CountingChat(Arc::clone(&calls))),
        deny_auth(),
        test_home(),
        test_rate(),
    );
    let req = Request::post("/webhook/sync")
        .header("x-stub-sig", "good")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        body["text"]
            .as_str()
            .unwrap_or_default()
            .contains("pairing code"),
        "unauthorized sender should get the pairing prompt, got {body}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "no turn may run for an unauthorized sender"
    );
}

#[tokio::test]
async fn rate_limited_sender_is_told_to_slow_down_and_runs_no_extra_turn() {
    // Authz is open (allow_all) so this isolates the W2.4 rate brake:
    // capacity 1 → the first message runs a turn, the second (same user) is
    // throttled with no extra turn.
    let calls = Arc::new(AtomicUsize::new(0));
    let mut reg = Registry::new();
    reg.insert("sync".into(), Arc::new(SyncStubAdapter));
    let app = router(
        reg,
        Arc::new(CountingChat(Arc::clone(&calls))),
        allow_all_auth(),
        test_home(),
        Arc::new(RateLimiter::per_minute(1)),
    );
    let body = |resp: axum::response::Response| async move {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
    };
    let req = || {
        Request::post("/webhook/sync")
            .header("x-stub-sig", "good")
            .body(axum::body::Body::from("{}"))
            .unwrap()
    };

    let first = body(app.clone().oneshot(req()).await.unwrap()).await;
    assert_eq!(first["text"], "ok", "first message runs a turn");
    let second = body(app.oneshot(req()).await.unwrap()).await;
    assert!(
        second["text"]
            .as_str()
            .unwrap_or_default()
            .contains("too fast"),
        "second message should be rate-limited, got {second}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "only the first message runs a turn"
    );
}
