//! Session-scoped RPC surfaces: idle interrupt/approval, `context.budget`,
//! and the `model.changed` announcement. (Lifecycle + sandbox: sessions.rs.)

use crate::helpers::{ScriptedProvider, make_session_manager};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn interrupt_returns_false_when_idle() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let sid = sm.create_session().await.unwrap();
    assert!(!sm.interrupt(&sid).await);
}

#[tokio::test]
async fn resolve_approval_returns_false_when_no_pending() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let sid = sm.create_session().await.unwrap();
    assert!(!sm.resolve_approval(&sid, true, None).await);
}

// SPL §3.4: `context.budget` returns the live prompt-composition breakdown —
// per-segment chars/est_tokens plus tier totals — for an open session, and a
// clean error for an unknown one.
#[tokio::test]
async fn context_budget_reports_tiers_for_an_open_session() {
    use regent_deacon::Dispatcher;
    use tokio::sync::mpsc::unbounded_channel;

    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let sid = sm.create_session().await.unwrap();
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "context.budget".into(),
        params: json!({"session_id": sid.to_string()}),
        id: Some(json!(1)),
    })
    .await;
    let v: serde_json::Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let r = &v["result"];
    assert!(r["tier0"]["chars"].as_u64().unwrap() > 0, "{r}");
    assert!(r["tool_defs"]["chars"].as_u64().unwrap() > 0);
    let segments = r["segments"].as_array().unwrap();
    assert!(
        segments.iter().any(|s| s["name"] == "system_prompt"),
        "{segments:?}"
    );
    assert!(segments.iter().all(|s| s["est_tokens"].is_u64()));

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "context.budget".into(),
        params: json!({"session_id": "nope"}),
        id: Some(json!(2)),
    })
    .await;
    let v: serde_json::Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown session")
    );
}

// Applying a model switch announces itself: `set_model` (model.set, and the
// Model page's primary-apply which re-points the active model through it)
// emits `model.changed` so the composer pill and status bar update live.
#[tokio::test]
async fn set_model_emits_model_changed() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, mut rx) = make_session_manager(&dir, provider);
    sm.set_model("nvidia/z-ai/glm-5.2");
    assert_eq!(sm.model(), "nvidia/z-ai/glm-5.2");
    let v: serde_json::Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["method"], "model.changed");
    assert_eq!(v["params"]["model"], "nvidia/z-ai/glm-5.2");
}
