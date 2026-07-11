//! Turn streaming (prompt.submit notifications) + the title backfill sweep.

use crate::helpers::{ScriptedProvider, make_session_manager};
use async_trait::async_trait;
use regent_deacon::Dispatcher;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

/// Infinite provider modeling a warm Anthropic cache: the FIRST model call is a
/// cold write (0 cache_read), every subsequent call reports 900 of 1000 prompt
/// tokens served from cache (90%). Infinite + stateless-per-call so the
/// background-review fork sharing this provider can't exhaust it or perturb the
/// per-turn assertion (unlike a finite scripted queue).
struct WarmCacheProvider {
    calls: AtomicUsize,
}

#[async_trait]
impl ChatProvider for WarmCacheProvider {
    async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(if n == 0 {
            ScriptedProvider::cached_reply("cold", 1000, 0)
        } else {
            ScriptedProvider::cached_reply("warm", 100, 900)
        })
    }

    fn model(&self) -> &str {
        "warm"
    }
}

#[tokio::test]
async fn prompt_submit_emits_turn_started_and_turn_complete() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("done")]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    let sid = sm.create_session().await.unwrap();
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "prompt.submit".into(),
        params: json!({"session_id": sid.to_string(), "text": "go"}),
        id: Some(json!(7)),
    })
    .await;

    // Expected stream: turn.started → message.complete → turn.complete → response.
    let mut methods = Vec::new();
    let mut response_id = None;
    let mut turn_complete = None;
    for _ in 0..4 {
        let line = tokio::time::timeout(std::time::Duration::from_secs(5), out_rx.recv())
            .await
            .expect("stream stalled")
            .expect("channel closed");
        let v: Value = serde_json::from_str(&line).unwrap();
        if let Some(m) = v.get("method").and_then(|m| m.as_str()) {
            methods.push(m.to_owned());
            if m == "turn.complete" {
                turn_complete = v.get("params").cloned();
            }
        } else {
            response_id = v.get("id").cloned();
            assert_eq!(v["result"]["reply"], "done");
        }
    }
    assert_eq!(
        methods,
        vec!["turn.started", "message.complete", "turn.complete"]
    );
    assert_eq!(response_id, Some(json!(7)));

    // The desktop status-bar ctx meter needs ALL THREE additive usage fields on
    // the SUCCESS turn.complete. The scripted provider reports prompt=10 /
    // completion=5, and the default agent config carries a non-zero context
    // budget.
    let params = turn_complete.expect("turn.complete carried params");
    assert_eq!(params["input_tokens"], 10, "input_tokens on turn.complete");
    assert_eq!(params["output_tokens"], 5, "output_tokens on turn.complete");
    assert!(
        params["context_max"].as_u64().is_some_and(|n| n > 0),
        "context_max present and non-zero: {params}"
    );
}

// SPL P2 acceptance (deliverable 5c / proposal §6): a 10-turn scripted session
// with mocked usage where turns ≥2 report ≥70% of input tokens as cache_read —
// assert the passthrough (agent → SessionManager::last_turn_usage, the exact
// accessor `turn.complete` reads) surfaces it. The real-API number is verified
// post-ship via telemetry; CI proves the plumbing carries it end to end.
#[tokio::test]
async fn warm_turns_pass_through_seventy_percent_cache_read() {
    let dir = TempDir::new().unwrap();
    let provider = Arc::new(WarmCacheProvider {
        calls: AtomicUsize::new(0),
    });
    let (sm, _rx) = make_session_manager(&dir, provider);
    let sid = sm.create_session().await.unwrap();

    for turn in 1..=10u32 {
        sm.run_turn(&sid, "go").await.unwrap();
        // The exact accessor `turn.complete` reads for the desktop meter.
        let (input, _output, _ctx, cache_read, cache_write) =
            sm.last_turn_usage(&sid).await.expect("known session");
        assert_eq!(cache_write, Some(0));
        if turn >= 2 {
            let read = cache_read.expect("warm turn reports cache_read");
            assert!(
                f64::from(read) >= 0.70 * f64::from(input),
                "turn {turn}: cache_read {read} is under 70% of input {input}"
            );
        }
    }
}

// SPL P2: the cached/fresh split rides `turn.complete` as additive fields when
// the provider reports it, and a clean turn carries no `cache_reset`.
#[tokio::test]
async fn turn_complete_carries_the_cache_split() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![ScriptedProvider::cached_reply("done", 200, 800)]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    let sid = sm.create_session().await.unwrap();
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "prompt.submit".into(),
        params: json!({"session_id": sid.to_string(), "text": "go"}),
        id: Some(json!(1)),
    })
    .await;

    let mut turn_complete = None;
    for _ in 0..4 {
        let line = tokio::time::timeout(std::time::Duration::from_secs(5), out_rx.recv())
            .await
            .expect("stream stalled")
            .expect("channel closed");
        let v: Value = serde_json::from_str(&line).unwrap();
        if v.get("method").and_then(|m| m.as_str()) == Some("turn.complete") {
            turn_complete = v.get("params").cloned();
        }
    }
    let params = turn_complete.expect("turn.complete carried params");
    assert_eq!(
        params["cache_read_tokens"], 800,
        "cache_read on turn.complete"
    );
    assert_eq!(
        params["cache_write_tokens"], 0,
        "cache_write on turn.complete"
    );
    assert_eq!(
        params["input_tokens"], 1000,
        "input rolls up uncached + cache"
    );
    assert!(
        params.get("cache_reset").is_none(),
        "a clean turn has no cache_reset: {params}"
    );
}

// The backfill sweep names pre-existing untitled sessions that hold a real
// exchange, reusing first-turn titling's model call. Happy path: an untitled
// session with a user+assistant exchange gets a title (persisted), reported as
// `titled: 1` with `remaining: 0`. Since 528629b the RPC replies `{started}`
// and runs detached (the serial read loop must not queue behind model calls),
// so the report semantics are asserted on the session manager directly and the
// RPC's reply shape is covered in the skip test below.
#[tokio::test]
async fn dispatcher_backfill_titles_names_untitled_exchange() {
    let dir = TempDir::new().unwrap();
    // First reply feeds the turn; second feeds the backfill title-gen call.
    let provider = ScriptedProvider::with(vec![
        ScriptedProvider::text_reply("sure, here's a plan"),
        ScriptedProvider::text_reply("Plan the road trip"),
    ]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    // An untitled session with a real (user + assistant) exchange.
    let sid = sm.create_session().await.unwrap();
    sm.run_turn(&sid, "help me plan a road trip").await.unwrap();

    let report = sm.backfill_titles(10).await.unwrap();
    assert_eq!(report.titled, 1);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.remaining, 0);

    // Title was persisted (cleaned) on the session row.
    let list = sm.list_sessions(10).unwrap();
    assert_eq!(list[0].title.as_deref(), Some("Plan the road trip"));
}

// Skip path: a thin session (fewer than two messages) is skipped with no model
// call, and an already-titled session is skipped too — so a re-run of the sweep
// is a clean no-op. The empty script proves no title-gen call was made.
#[tokio::test]
async fn dispatcher_backfill_titles_skips_thin_and_titled_sessions() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    // A freshly created session has no messages → nothing to title from.
    let thin = sm.create_session().await.unwrap();
    // A titled session is never re-titled.
    let titled = sm.create_session().await.unwrap();
    sm.rename_session(&titled, "Already Named").unwrap();

    // The RPC acknowledges immediately (detached sweep — 528629b): callers
    // watch `session.titled` events, never a blocking report reply.
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "session.backfill_titles".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["started"], true);

    // The report semantics, on the sweep itself: both sessions skipped, no
    // model call made (the empty script would panic otherwise).
    let report = sm.backfill_titles(50).await.unwrap();
    assert_eq!(report.titled, 0);
    assert_eq!(report.skipped, 2);
    assert_eq!(report.remaining, 0);

    // The thin session stayed untitled; no phantom rename occurred.
    let list = sm.list_sessions(10).unwrap();
    assert!(
        list.iter()
            .find(|m| m.id == thin.to_string())
            .unwrap()
            .title
            .is_none()
    );
}
