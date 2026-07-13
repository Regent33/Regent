//! Session lifecycle (create → list → resume) + the ingress sandbox jail.

use crate::helpers::{ScriptedProvider, make_session_manager};
use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// ── Sandbox-on-ingress test (W1.2 / P1-005) ──────────────────────────────────

/// Scripted provider that also records the messages of the last request, so
/// a test can inspect the tool result the agent fed back.
struct RecordingProvider {
    responses: Mutex<VecDeque<ChatResponse>>,
    seen: Mutex<Vec<ChatMessage>>,
}

#[async_trait]
impl ChatProvider for RecordingProvider {
    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        *self.seen.lock().unwrap() = req.messages.clone();
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Parse("script exhausted".into()))
    }

    fn model(&self) -> &str {
        "scripted"
    }
}

/// Keyed sessions are external ingress (platform webhooks): a read outside
/// the workspace must be rejected by the sandbox even with REGENT_SANDBOX
/// unset — external turns are always jailed.
#[tokio::test]
async fn keyed_session_is_sandboxed_and_rejects_out_of_workspace_reads() {
    let dir = TempDir::new().unwrap();
    let outside = dir.path().join("secret.txt");
    std::fs::write(&outside, "ssh key material").unwrap();

    let read_outside = ChatResponse {
        message: ChatMessage::assistant(
            None,
            vec![regent_kernel::ToolCall {
                id: "call_1".into(),
                name: "read_file".into(),
                arguments: json!({"path": outside.to_string_lossy()}).to_string(),
            }],
        ),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            ..Default::default()
        },
        finish_reason: Some("tool_calls".into()),
    };
    let provider = Arc::new(RecordingProvider {
        responses: Mutex::new(vec![read_outside, ScriptedProvider::text_reply("done")].into()),
        seen: Mutex::new(Vec::new()),
    });
    // cwd "." is the workspace; `outside` (a temp dir) is beyond it.
    let (sm, _rx) = make_session_manager(&dir, Arc::clone(&provider) as Arc<dyn ChatProvider>);

    let sid = sm.ensure_keyed_session("telegram:123").await.unwrap();
    sm.run_turn(&sid, "read that file").await.unwrap();

    let seen = provider.seen.lock().unwrap();
    let tool_result = seen
        .iter()
        .rev()
        .find(|m| m.tool_call_id.as_deref() == Some("call_1"))
        .expect("tool result fed back to the provider");
    let body = tool_result.content.clone().unwrap_or_default();
    assert!(
        body.contains("escapes the sandbox root"),
        "external turn must not read outside the workspace; tool result was: {body}"
    );
}

// ── Session lifecycle tests ───────────────────────────────────────────────────

#[tokio::test]
async fn create_session_returns_sess_prefixed_id() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    let sid = sm.create_session().await.unwrap();
    assert!(sid.as_str().starts_with("sess_"), "id was: {sid}");
}

#[tokio::test]
async fn create_session_appears_in_list() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    let sid = sm.create_session().await.unwrap();
    let list = sm.list_sessions(10).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, sid.to_string());
}

#[tokio::test]
async fn run_turn_returns_agent_reply() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("hello")]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    let sid = sm.create_session().await.unwrap();
    let reply = sm.run_turn(&sid, "hi").await.unwrap();
    assert_eq!(reply, "hello");
}

#[tokio::test]
async fn resume_session_reconnects_history() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("first reply")]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    let sid = sm.create_session().await.unwrap();
    sm.run_turn(&sid, "first message").await.unwrap();

    // Resume in a fresh manager (simulates deacon restart with new provider)
    let provider2: Arc<dyn ChatProvider> = ScriptedProvider::with(vec![]);
    let (sm2, _rx2) = make_session_manager(&dir, provider2);
    let resumed = sm2.resume_session(sid.clone()).await.unwrap();
    assert_eq!(resumed, sid);
}

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
