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
    async fn chat_keyed(&self, _key: &str, _message: String) -> Result<ChatReply, DeaconError> {
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
    router(
        reg,
        Arc::new(StubChat),
        allow_all_auth(),
        test_home(),
        test_rate(),
    )
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

#[path = "tests_delivery.rs"]
mod tests_delivery;
