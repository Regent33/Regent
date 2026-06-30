//! M4 delegation contract: bounded parallel leaf fan-out returns results in
//! task order, isolates failures per child, and gives every child its own
//! session with only the task brief.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{DelegateTool, DelegationConfig};
use regent_kernel::{ChatMessage, Role, ToolCall};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{DenyAll, ToolCatalog, ToolContext};
use serde_json::{Value, json};
use std::sync::Arc;

/// Responds based on the request's last user message — deterministic under
/// concurrency (a pop-in-order script would race across parallel children).
struct ContentKeyed;

#[async_trait]
impl ChatProvider for ContentKeyed {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let brief = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .and_then(|m| m.content.clone())
            .unwrap_or_default();
        if brief.contains("boom") {
            return Err(ProviderError::Api {
                status: 400,
                body: "synthetic child failure".into(),
            });
        }
        Ok(ChatResponse {
            message: ChatMessage::assistant(Some(format!("echo[{brief}]")), vec![]),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
        })
    }

    fn model(&self) -> &str {
        "content-keyed"
    }
}

fn catalog_with_delegation(store: &Arc<Store>) -> Arc<ToolCatalog> {
    let mut catalog = ToolCatalog::new();
    DelegateTool::new(
        Arc::new(ContentKeyed),
        Arc::clone(store),
        Arc::new(ToolCatalog::new()), // leaf catalog: no delegate, no memory
        DelegationConfig {
            max_concurrent: 2,
            ..DelegationConfig::default()
        },
    )
    .register(&mut catalog)
    .unwrap();
    Arc::new(catalog)
}

#[tokio::test]
async fn parallel_leaf_delegation_returns_ordered_isolated_results() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let catalog = catalog_with_delegation(&store);
    let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll));

    let args = json!({
        "tasks": ["alpha task", "boom task", "gamma task"],
        "context": "shared brief"
    })
    .to_string();
    let output = catalog.dispatch("delegate_task", &args, &ctx).await;
    let value: Value = serde_json::from_str(&output).unwrap();
    let results = value["results"].as_array().unwrap();

    // Input order preserved despite parallel execution (cap = 2).
    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["task"], "alpha task");
    assert_eq!(results[1]["task"], "boom task");
    assert_eq!(results[2]["task"], "gamma task");

    // Children saw the shared context + ONLY their task brief.
    assert_eq!(results[0]["status"], "ok");
    assert_eq!(
        results[0]["summary"],
        "echo[Context: shared brief\n\nTask: alpha task]"
    );
    assert_eq!(results[2]["status"], "ok");

    // One child failing does not poison its siblings.
    assert_eq!(results[1]["status"], "failed");
    assert!(
        results[1]["summary"]
            .as_str()
            .unwrap()
            .contains("synthetic child failure")
    );

    // Each successful child ran in its own persisted session.
    let alpha_session = results[0]["session_id"].as_str().unwrap();
    let gamma_session = results[2]["session_id"].as_str().unwrap();
    assert_ne!(alpha_session, gamma_session);
    let rows = store
        .get_conversation(&regent_kernel::SessionId::from_string(alpha_session))
        .unwrap();
    assert_eq!(rows.len(), 2, "child session = brief + reply only");
}

#[tokio::test]
async fn single_goal_shape_and_missing_args_are_handled() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let catalog = catalog_with_delegation(&store);
    let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll));

    let output = catalog
        .dispatch(
            "delegate_task",
            &json!({"goal": "solo task"}).to_string(),
            &ctx,
        )
        .await;
    let value: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["results"][0]["summary"], "echo[solo task]");

    let output = catalog.dispatch("delegate_task", "{}", &ctx).await;
    assert!(output.contains("provide 'goal' or non-empty 'tasks'"));
}

/// Delegates once on the first call (brief contains GO_DEEPER), then — having
/// received a tool result — finishes. Drives a child to delegate one more
/// level so we can observe depth-2 end to end.
struct DepthProvider;

#[async_trait]
impl ChatProvider for DepthProvider {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let done = |text: &str| ChatResponse {
            message: ChatMessage::assistant(Some(text.into()), vec![]),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
        };
        // A tool result is back → the child has delegated and gotten an answer.
        if request
            .messages
            .last()
            .map(|m| m.role == Role::Tool)
            .unwrap_or(false)
        {
            return Ok(done("nested ok"));
        }
        let brief = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .and_then(|m| m.content.clone())
            .unwrap_or_default();
        if brief.contains("GO_DEEPER") {
            return Ok(ChatResponse {
                message: ChatMessage::assistant(
                    None,
                    vec![ToolCall {
                        id: "c1".into(),
                        name: "delegate_task".into(),
                        arguments: json!({"goal": "leaf work"}).to_string(),
                    }],
                ),
                usage: TokenUsage::default(),
                finish_reason: Some("tool_calls".into()),
            });
        }
        Ok(done(&format!("echo[{brief}]")))
    }

    fn model(&self) -> &str {
        "depth-provider"
    }
}

#[tokio::test]
async fn a_child_can_delegate_one_more_level() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let mut catalog = ToolCatalog::new();
    DelegateTool::new(
        Arc::new(DepthProvider),
        Arc::clone(&store),
        Arc::new(ToolCatalog::new()), // empty leaf
        DelegationConfig {
            max_concurrent: 2,
            max_depth: 2,
            ..DelegationConfig::default()
        },
    )
    .register(&mut catalog)
    .unwrap();
    let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll));

    let output = catalog
        .dispatch(
            "delegate_task",
            &json!({"goal": "GO_DEEPER"}).to_string(),
            &ctx,
        )
        .await;
    let value: Value = serde_json::from_str(&output).unwrap();

    // The child reached "nested ok" only by receiving a tool result — i.e. its
    // own depth-2 `delegate_task` existed and ran a grandchild.
    assert_eq!(value["results"][0]["status"], "ok");
    assert_eq!(value["results"][0]["summary"], "nested ok");
}
