//! Integration tests for the daemon layer.
//! Cover: RPC type serialisation, session lifecycle (create → list → resume),
//! turn execution with a scripted provider.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::AgentConfig;
use regent_daemon::{Dispatcher, SessionManager};
use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_skills::{FsSkillRepository, SkillLibrary};
use regent_store::Store;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

// ── Scripted provider ────────────────────────────────────────────────────────

struct ScriptedProvider {
    responses: Mutex<VecDeque<ChatResponse>>,
}

impl ScriptedProvider {
    fn with(responses: Vec<ChatResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
        })
    }

    fn text_reply(text: &str) -> ChatResponse {
        ChatResponse {
            message: ChatMessage::assistant(Some(text.into()), vec![]),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            finish_reason: Some("stop".into()),
        }
    }
}

#[async_trait]
impl ChatProvider for ScriptedProvider {
    async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
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

// ── Test helpers ─────────────────────────────────────────────────────────────

fn make_session_manager(
    dir: &TempDir,
    provider: Arc<dyn ChatProvider>,
) -> (
    Arc<SessionManager>,
    tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(SkillLibrary::new(Arc::new(
        FsSkillRepository::new(dir.path().join("skills")).unwrap(),
    )));
    let (tx, rx) = unbounded_channel();
    let model = provider.model().to_owned();
    let factory: regent_daemon::ProviderFactory = Arc::new(move |_model| Arc::clone(&provider));
    let sm = Arc::new(SessionManager::new(
        factory,
        model,
        store,
        graph,
        skills,
        PathBuf::from("."),
        AgentConfig::default(),
        Vec::new(), // disabled_tools
        tx,
    ));
    (sm, rx)
}

// ── RPC type tests ────────────────────────────────────────────────────────────

#[test]
fn rpc_request_round_trips() {
    let raw = r#"{"jsonrpc":"2.0","method":"health","params":{},"id":1}"#;
    let req: regent_daemon::RpcRequest = serde_json::from_str(raw).unwrap();
    assert_eq!(req.method, "health");
    assert_eq!(req.id, Some(json!(1)));
}

#[test]
fn ok_response_serialises_correctly() {
    use regent_daemon::domain::entities::ok_response;
    let resp = ok_response(Some(json!(42)), json!({"status": "ok"}));
    let s = serde_json::to_string(&resp).unwrap();
    let v: Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], 42);
    assert_eq!(v["result"]["status"], "ok");
    assert!(v.get("error").is_none());
}

#[test]
fn err_response_serialises_correctly() {
    use regent_daemon::domain::entities::err_response;
    let resp = err_response(Some(json!(1)), -32601, "Method not found");
    let v: Value = serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32601);
    assert!(v.get("result").is_none());
}

#[test]
fn notification_has_no_id_field() {
    use regent_daemon::RpcNotification;
    let n = RpcNotification::new("turn.started", json!({"session_id": "x"}));
    let v: Value = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
    assert!(v.get("id").is_none());
    assert_eq!(v["method"], "turn.started");
}

#[test]
fn rpc_request_without_id_is_notification() {
    let raw = r#"{"jsonrpc":"2.0","method":"ping","params":{}}"#;
    let req: regent_daemon::RpcRequest = serde_json::from_str(raw).unwrap();
    assert!(req.id.is_none());
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

    // Resume in a fresh manager (simulates daemon restart with new provider)
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
    assert!(!sm.resolve_approval(&sid, true).await);
}

// ── Dispatcher routing tests ──────────────────────────────────────────────────

#[tokio::test]
async fn dispatcher_health_returns_ok() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "health".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;

    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["status"], "ok");
    assert_eq!(v["id"], 1);
}

#[tokio::test]
async fn dispatcher_unknown_method_returns_minus_32601() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "no.such.method".into(),
        params: json!({}),
        id: Some(json!(99)),
    })
    .await;

    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32601);
    assert_eq!(v["id"], 99);
}

#[tokio::test]
async fn dispatcher_model_list_and_set() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    // list exposes the catalog
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "model.list".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let items = v["result"].as_array().unwrap();
    assert!(items.iter().any(|m| m["id"] == "claude-sonnet-4-6"));

    // set switches the active model for new sessions
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "model.set".into(),
        params: json!({"model": "claude-opus-4-8"}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["model"], "claude-opus-4-8");
    assert_eq!(sm.model(), "claude-opus-4-8");
}

#[tokio::test]
async fn dispatcher_model_list_merges_configured_providers() {
    // §A.P1: model.list surfaces configured providers' models as "<provider>/<model>".
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let cfg: regent_daemon::DaemonConfig = serde_json::from_value(json!({
        "providers": {
            "groq": { "kind": "groq", "api_key_env": "X", "models": ["llama-3.3-70b"] }
        }
    }))
    .unwrap();
    let d = Dispatcher::new(sm, tx).with_config(cfg);

    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "model.list".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let items = v["result"].as_array().unwrap();
    // static catalog still present …
    assert!(items.iter().any(|m| m["id"] == "claude-sonnet-4-6"));
    // … plus the configured provider's model, namespaced.
    assert!(items.iter().any(|m| m["id"] == "groq/llama-3.3-70b"), "merged provider model");
}

#[tokio::test]
async fn dispatcher_memory_pending_and_reject() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    // empty approval queue
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "memory.pending".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert!(v["result"].as_array().unwrap().is_empty());

    // rejecting an unknown id is a clean no-op
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "memory.reject".into(),
        params: json!({"id": "pw_nope"}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["removed"], false);
}

#[tokio::test]
async fn dispatcher_session_create_then_list() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    // session.create
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "session.create".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let sid = v["result"]["session_id"].as_str().unwrap().to_owned();
    assert!(sid.starts_with("sess_"));

    // session.list
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "session.list".into(),
        params: json!({}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let items = v["result"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["session_id"].as_str().unwrap(), sid);
}

#[tokio::test]
async fn dispatcher_model_get_and_skills_list() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "model.get".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["model"], "scripted");

    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "skills.list".into(),
        params: json!({}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert!(v["result"].is_array());
}

// The in-process `regent` admin tool routes through this: it must reach real
// handlers once installed, refuse turn/session-lifecycle methods, and report
// when the composition root hasn't installed the self-handle.
#[tokio::test]
async fn run_admin_command_routes_and_refuses_lifecycle() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    // Not installed yet → clear refusal (no panic, no hang).
    let err = sm
        .run_admin_command("model.get", json!({}))
        .await
        .unwrap_err();
    assert!(err.contains("not installed"), "got: {err}");

    sm.install_admin(regent_daemon::AdminDeps::default());

    // Happy path: forwards to the live model.get handler.
    let result = sm.run_admin_command("model.get", json!({})).await.unwrap();
    assert_eq!(result["model"], "scripted");

    // Turn/session lifecycle is off-limits to the agent.
    let err = sm
        .run_admin_command("prompt.submit", json!({}))
        .await
        .unwrap_err();
    assert!(err.contains("live turn/session"), "got: {err}");
}

#[tokio::test]
async fn dispatcher_config_get_round_trips_the_loaded_config() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let cfg = regent_daemon::DaemonConfig::default();
    let d = Dispatcher::new(sm, tx).with_config(cfg);

    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "config.get".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["_config_version"], 1);
    assert_eq!(v["result"]["cron"]["tick_interval_secs"], 30);
}

#[tokio::test]
async fn dispatcher_cron_add_list_remove_round_trip() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let repo: Arc<dyn regent_cron::JobRepository> =
        Arc::new(regent_cron::FsJobRepository::new(dir.path().join("cron")).unwrap());
    let d = Dispatcher::new(sm, tx).with_cron(repo);

    // add
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "cron.add".into(),
        params: json!({"name": "report", "schedule": "30m", "prompt": "write the report"}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let job_id = v["result"]["id"].as_str().unwrap().to_owned();
    assert!(job_id.starts_with("job_"));

    // list
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "cron.list".into(),
        params: json!({}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let jobs = v["result"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["name"], "report");

    // remove
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "cron.remove".into(),
        params: json!({"id": job_id}),
        id: Some(json!(3)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["removed"], true);

    // bad schedule is a -32602
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "cron.add".into(),
        params: json!({"name": "x", "schedule": "tuesday", "prompt": "y"}),
        id: Some(json!(4)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32602);
}

#[tokio::test]
async fn prompt_submit_emits_turn_started_and_turn_complete() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("done")]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    let sid = sm.create_session().await.unwrap();
    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "prompt.submit".into(),
        params: json!({"session_id": sid.to_string(), "text": "go"}),
        id: Some(json!(7)),
    })
    .await;

    // Expected stream: turn.started → message.complete → turn.complete → response.
    let mut methods = Vec::new();
    let mut response_id = None;
    for _ in 0..4 {
        let line = tokio::time::timeout(std::time::Duration::from_secs(5), out_rx.recv())
            .await
            .expect("stream stalled")
            .expect("channel closed");
        let v: Value = serde_json::from_str(&line).unwrap();
        if let Some(m) = v.get("method").and_then(|m| m.as_str()) {
            methods.push(m.to_owned());
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
}

#[tokio::test]
async fn dispatcher_commands_list_is_non_empty() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    d.handle(regent_daemon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "commands.list".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;

    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert!(!v["result"].as_array().unwrap().is_empty());
}
