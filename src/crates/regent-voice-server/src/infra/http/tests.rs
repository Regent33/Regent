//! Security-gate behavior tests for the HTTP surface.

use super::{AppState, router};
use crate::infra::engines::Engines;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn app() -> (Router, Arc<AppState>) {
    let state = Arc::new(AppState {
        engines: RwLock::new(Engines::default()),
        deacon: RwLock::new(None),
        agent_note: RwLock::new("test".into()),
        // Tests never spawn a real deacon: keep the retry gate far away.
        agent_retry_at: RwLock::new(Some(
            std::time::Instant::now() + std::time::Duration::from_secs(3600),
        )),
        token: "sekrit".into(),
    });
    (router(Arc::clone(&state)), state)
}

fn req(method: &str, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "localhost:8000")
}

#[tokio::test]
async fn non_local_host_is_forbidden() {
    let (app, _) = app();
    let r = Request::builder()
        .method("GET")
        .uri("/health")
        .header(header::HOST, "evil.tld")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(r).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn call_turn_requires_the_token() {
    let (app, _) = app();
    let bad = req("POST", "/call/turn").body(Body::empty()).unwrap();
    assert_eq!(
        app.clone().oneshot(bad).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let good = req("POST", "/call/turn")
        .header("x-call-token", "sekrit")
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.oneshot(good).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn cors_grant_only_for_the_call_ui_origin() {
    let (app, _) = app();
    let allowed = req("GET", "/call/token")
        .header(header::ORIGIN, "http://localhost:3000")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(allowed).await.unwrap();
    assert_eq!(
        res.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "http://localhost:3000"
    );
    let other = req("GET", "/call/token")
        .header(header::ORIGIN, "http://attacker.example")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(other).await.unwrap();
    assert!(
        res.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none(),
        "no CORS grant for unknown origins — the browser blocks the read"
    );
}

#[tokio::test]
async fn preflight_answers_for_the_allowed_origin() {
    let (app, _) = app();
    let r = req("OPTIONS", "/call/turn")
        .header(header::ORIGIN, "http://localhost:3000")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(r).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let allow = res
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .unwrap();
    assert!(allow.to_str().unwrap().contains("x-call-token"));
}

#[tokio::test]
async fn served_call_page_carries_the_real_token() {
    let (app, _) = app();
    let r = req("GET", "/call").body(Body::empty()).unwrap();
    let res = app.oneshot(r).await.unwrap();
    let body = axum::body::to_bytes(res.into_body(), 1_000_000)
        .await
        .unwrap();
    let page = String::from_utf8_lossy(&body);
    assert!(page.contains("content=\"sekrit\""));
    assert!(!page.contains("__CALL_TOKEN__"));
}
