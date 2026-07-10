//! Turn streaming (prompt.submit notifications) + the title backfill sweep.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_deacon::Dispatcher;
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

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

// The backfill op names pre-existing untitled sessions that hold a real
// exchange, reusing first-turn titling's model call. Happy path: an untitled
// session with a user+assistant exchange gets a title (persisted), reported as
// `titled: 1` with `remaining: 0`.
#[tokio::test]
async fn dispatcher_backfill_titles_names_untitled_exchange() {
    let dir = TempDir::new().unwrap();
    // First reply feeds the turn; second feeds the backfill title-gen call.
    let provider = ScriptedProvider::with(vec![
        ScriptedProvider::text_reply("sure, here's a plan"),
        ScriptedProvider::text_reply("Plan the road trip"),
    ]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    // An untitled session with a real (user + assistant) exchange.
    let sid = sm.create_session().await.unwrap();
    sm.run_turn(&sid, "help me plan a road trip").await.unwrap();

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "session.backfill_titles".into(),
        params: json!({"limit": 10}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["titled"], 1);
    assert_eq!(v["result"]["skipped"], 0);
    assert_eq!(v["result"]["remaining"], 0);

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

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "session.backfill_titles".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["titled"], 0);
    assert_eq!(v["result"]["skipped"], 2);
    assert_eq!(v["result"]["remaining"], 0);

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
