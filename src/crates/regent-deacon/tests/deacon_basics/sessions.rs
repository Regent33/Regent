//! Session lifecycle (create → list → resume) + the ingress sandbox jail.

use crate::helpers::{ScriptedProvider, make_session_manager};
use async_trait::async_trait;
use or_core::TokenUsage;
use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::sync::Notify;

// ── Sandbox-on-ingress test (W1.2 / P1-005) ──────────────────────────────────

/// Scripted provider that also records the messages of the last request, so
/// a test can inspect the tool result the agent fed back.
struct RecordingProvider {
    responses: Mutex<VecDeque<ChatResponse>>,
    seen: Mutex<Vec<ChatMessage>>,
}

/// Holds the first provider request open so a resume can race it. A second
/// provider call before release proves the resume replaced the live agent.
struct BlockingProvider {
    calls: AtomicUsize,
    first_entered: Notify,
    release_first: Notify,
    seen: Mutex<Vec<Vec<ChatMessage>>>,
}

#[async_trait]
impl ChatProvider for BlockingProvider {
    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(req.messages.clone());
        if call == 0 {
            self.first_entered.notify_one();
            self.release_first.notified().await;
        }
        Ok(ScriptedProvider::text_reply(if call == 0 {
            "first reply"
        } else {
            "second reply"
        }))
    }

    fn model(&self) -> &str {
        "blocking"
    }
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
        body.contains("outside this session's workspace"),
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
async fn resume_during_a_turn_keeps_one_agent_and_serial_history() {
    let dir = TempDir::new().unwrap();
    let provider = Arc::new(BlockingProvider {
        calls: AtomicUsize::new(0),
        first_entered: Notify::new(),
        release_first: Notify::new(),
        seen: Mutex::new(Vec::new()),
    });
    let (sm, _rx) = make_session_manager(&dir, Arc::clone(&provider) as Arc<dyn ChatProvider>);
    let sid = sm.create_session().await.unwrap();

    let first_sm = Arc::clone(&sm);
    let first_sid = sid.clone();
    let first = tokio::spawn(async move { first_sm.run_turn(&first_sid, "make a deck").await });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        provider.first_entered.notified(),
    )
    .await
    .expect("first provider request did not start");

    sm.resume_session(sid.clone()).await.unwrap();
    let second_sm = Arc::clone(&sm);
    let second_sid = sid.clone();
    let second = tokio::spawn(async move { second_sm.run_turn(&second_sid, "proceed").await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "resume replaced the live agent and allowed an overlapping turn"
    );

    provider.release_first.notify_one();
    assert_eq!(first.await.unwrap().unwrap(), "first reply");
    assert_eq!(second.await.unwrap().unwrap(), "second reply");

    let seen = provider.seen.lock().unwrap();
    let second_messages = &seen[1];
    assert!(second_messages.iter().any(|m| {
        m.role == regent_kernel::Role::Assistant && m.content.as_deref() == Some("first reply")
    }));
}

// Session-scoped RPC surface tests (interrupt/approval when idle,
// context.budget, model.changed) live in session_rpc.rs.
