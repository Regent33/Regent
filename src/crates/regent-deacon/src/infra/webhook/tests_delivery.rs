//! Outbound-delivery tests for the webhook module (file-size rule split).

use super::*;

fn delivery_with_stub() -> WebhookPlatformDelivery {
    let mut adapters = Registry::new();
    adapters.insert("stub".into(), Arc::new(StubAdapter));
    WebhookPlatformDelivery {
        adapters,
        file_senders: HashMap::new(),
        reactors: HashMap::new(),
        client: reqwest::Client::new(),
    }
}

/// A reactor that records what it was asked to do instead of calling a
/// platform — the point under test is which message id the sink chooses, which
/// is exactly the part a live call would hide.
struct RecordingReactor(std::sync::Mutex<Vec<(String, Option<String>, String)>>);

#[async_trait]
impl regent_gateway::WebhookReactor for RecordingReactor {
    async fn react(
        &self,
        _client: &reqwest::Client,
        chat_id: &str,
        message_id: Option<&str>,
        emoji: &str,
    ) -> Result<(), GatewayError> {
        self.0.lock().unwrap().push((
            chat_id.to_owned(),
            message_id.map(str::to_owned),
            emoji.to_owned(),
        ));
        Ok(())
    }
}

fn delivery_with_reactor(reactor: Arc<RecordingReactor>) -> WebhookPlatformDelivery {
    let mut adapters = Registry::new();
    adapters.insert("stub".into(), Arc::new(StubAdapter));
    let mut reactors: HashMap<String, Arc<dyn regent_gateway::WebhookReactor>> = HashMap::new();
    reactors.insert("stub".into(), reactor);
    WebhookPlatformDelivery {
        adapters,
        file_senders: HashMap::new(),
        reactors,
        client: reqwest::Client::new(),
    }
}

/// A platform that delivers but cannot react must get `send_message` WITHOUT
/// `react_to_message`, or the model is handed a tool that can only fail.
#[test]
fn a_platform_without_a_reactor_gets_no_reaction_sink() {
    let delivery = delivery_with_stub();
    assert!(delivery.sink_for("stub:c1").is_some());
    assert!(delivery.reaction_sink_for("stub:c1").is_none());
}

#[test]
fn a_platform_with_a_reactor_resolves_one_bound_to_the_conversation() {
    let reactor = Arc::new(RecordingReactor(std::sync::Mutex::new(Vec::new())));
    let delivery = delivery_with_reactor(Arc::clone(&reactor));
    let sink = delivery.reaction_sink_for("stub:c1").expect("resolves");
    assert_eq!(sink.targets(), vec!["stub:c1".to_owned()]);
    assert!(delivery.reaction_sink_for("nope:c1").is_none());
    assert!(delivery.reaction_sink_for("nocolon").is_none());
}

/// The whole point of the last-inbound map: "react to that" with no id has to
/// resolve to the message the ingress route recorded for THIS chat, and an
/// explicit id must win over it.
#[tokio::test]
async fn a_bare_react_targets_the_last_inbound_message_and_an_explicit_id_wins() {
    let reactor = Arc::new(RecordingReactor(std::sync::Mutex::new(Vec::new())));
    let sink = delivery_with_reactor(Arc::clone(&reactor))
        .reaction_sink_for("stub:react-chat")
        .unwrap();

    // Nothing recorded yet: the reactor is still called with None, and each
    // platform words "there is nothing here to react to" for itself.
    sink.react(None, "OK").await.unwrap();
    assert_eq!(reactor.0.lock().unwrap()[0].1, None);

    super::last_inbound::remember("stub", "react-chat", "m-42");
    sink.react(None, "OK").await.unwrap();
    assert_eq!(
        reactor.0.lock().unwrap()[1].1.as_deref(),
        Some("m-42"),
        "a bare react must use the last inbound message"
    );

    sink.react(Some("m-7"), "OK").await.unwrap();
    let calls = reactor.0.lock().unwrap();
    assert_eq!(calls[2].1.as_deref(), Some("m-7"), "an explicit id wins");
    assert_eq!(calls[2].0, "react-chat");
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
        test_queue(),
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
        test_queue(),
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
        test_queue(),
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

/// A `ChatService` whose `chat_keyed` blocks on a `Notify` until released —
/// lets a test hold one turn "in flight" to exercise the capacity boundary.
struct BlockingChat {
    calls: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
}
#[async_trait::async_trait]
impl ChatService for BlockingChat {
    async fn chat(&self, _s: Option<String>, _m: String) -> Result<ChatReply, DeaconError> {
        unimplemented!("only chat_keyed is exercised here")
    }
    async fn chat_keyed(&self, _key: &str, _m: String) -> Result<ChatReply, DeaconError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.release.notified().await;
        Ok(ChatReply {
            session: "s".into(),
            reply: "ok".into(),
        })
    }
}

#[tokio::test]
async fn a_burst_beyond_the_pending_cap_gets_told_to_wait_and_runs_no_extra_turn() {
    // Authz/rate are wide open so this isolates the queue-gate cap: capacity 1
    // → a SECOND sync request for the same chat, arriving while the first is
    // still in flight, must be refused with no additional turn.
    let calls = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(tokio::sync::Notify::new());
    let mut reg = Registry::new();
    reg.insert("sync".into(), Arc::new(SyncStubAdapter));
    let app = router(
        reg,
        Arc::new(BlockingChat {
            calls: Arc::clone(&calls),
            release: Arc::clone(&release),
        }),
        allow_all_auth(),
        test_home(),
        test_rate(),
        Arc::new(QueueGate::new(1)),
    );
    let req = || {
        Request::post("/webhook/sync")
            .header("x-stub-sig", "good")
            .body(axum::body::Body::from("{}"))
            .unwrap()
    };

    let first_app = app.clone();
    let first = tokio::spawn(async move { first_app.oneshot(req()).await.unwrap() });
    // Wait until the first request has actually entered chat_keyed (and is
    // therefore holding the gate's one slot) before firing the second.
    while calls.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }

    let second = app.oneshot(req()).await.unwrap();
    let bytes = axum::body::to_bytes(second.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        body["text"]
            .as_str()
            .unwrap_or_default()
            .contains("Still working"),
        "second message should be told to wait, got {body}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "no extra turn ran for the refused message"
    );

    release.notify_one();
    first.await.unwrap();
}
