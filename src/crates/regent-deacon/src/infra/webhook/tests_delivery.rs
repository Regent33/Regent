//! Outbound-delivery tests for the webhook module (file-size rule split).

use super::*;

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
    let app = router(
        reg,
        Arc::new(StubChat),
        allow_all_auth(),
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
