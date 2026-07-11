//! E2E harness behavior with a scripted provider and stub verify/checkpoint
//! ports, over the real `Agent` loop and a real in-memory store: plan → approve
//! → execute → verify, on both the green path and the verify-fail → revert path,
//! plus the deny gate and the no-git degrade.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::AgentConfig;
use regent_code::{Checkpoint, CodeHarness, Verifier, VerifyOutcome};
use regent_kernel::{ChatMessage, RegentError};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{AllowAll, ApprovalHandler, DenyAll, ToolCatalog, ToolContext};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

struct ScriptedProvider {
    responses: Mutex<VecDeque<ChatResponse>>,
}

impl ScriptedProvider {
    fn new(texts: &[&str]) -> Arc<Self> {
        let responses = texts.iter().map(|t| text_response(t)).collect();
        Arc::new(Self {
            responses: Mutex::new(responses),
        })
    }
}

#[async_trait]
impl ChatProvider for ScriptedProvider {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Parse("script exhausted".into()))
    }
    fn model(&self) -> &str {
        "scripted-model"
    }
}

fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(Some(text.into()), vec![]),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            ..Default::default()
        },
        finish_reason: Some("stop".into()),
    }
}

/// Verifier that always reports the same outcome (or none).
struct StubVerifier(Option<VerifyOutcome>);

#[async_trait]
impl Verifier for StubVerifier {
    async fn verify(&self, _workspace: &Path) -> Result<Option<VerifyOutcome>, RegentError> {
        Ok(self.0.clone())
    }
}

/// Checkpoint that hands out a fixed snapshot id (or none) and records every
/// restore call so a test can assert revert happened — or didn't.
struct StubCheckpoint {
    snapshot_id: Option<String>,
    restored: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Checkpoint for StubCheckpoint {
    async fn snapshot(&self) -> Result<Option<String>, RegentError> {
        Ok(self.snapshot_id.clone())
    }
    async fn restore(&self, id: &str) -> Result<(), RegentError> {
        self.restored.lock().unwrap().push(id.to_owned());
        Ok(())
    }
}

fn passed(summary: &str) -> Option<VerifyOutcome> {
    Some(VerifyOutcome {
        passed: true,
        summary: summary.into(),
    })
}

fn failed(summary: &str) -> Option<VerifyOutcome> {
    Some(VerifyOutcome {
        passed: false,
        summary: summary.into(),
    })
}

/// Builds a harness wired with the given provider script, approval handler, and
/// stub ports; returns it plus the restore-call log.
fn harness(
    texts: &[&str],
    approval: Arc<dyn ApprovalHandler>,
    verify: Option<VerifyOutcome>,
    snapshot_id: Option<String>,
) -> (CodeHarness, Arc<Mutex<Vec<String>>>) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let ctx = ToolContext::new(std::env::temp_dir(), approval);
    let restored = Arc::new(Mutex::new(Vec::new()));
    let checkpoint = StubCheckpoint {
        snapshot_id,
        restored: Arc::clone(&restored),
    };
    let h = CodeHarness::new(
        ScriptedProvider::new(texts),
        Arc::new(ToolCatalog::new()),
        store,
        ctx,
        "system",
        AgentConfig::default(),
        Arc::new(StubVerifier(verify)),
        Arc::new(checkpoint),
    );
    (h, restored)
}

#[tokio::test]
async fn green_path_executes_and_keeps_changes() {
    let (h, restored) = harness(
        &["PLAN: edit foo", "Done — edited foo."],
        Arc::new(AllowAll),
        passed("test result: ok. 3 passed"),
        Some("snap-1".into()),
    );

    let outcome = h.run("add a foo").await.unwrap();

    assert!(outcome.approved);
    assert!(outcome.executed);
    assert_eq!(outcome.plan, "PLAN: edit foo");
    assert!(outcome.verify.as_ref().unwrap().passed);
    assert!(!outcome.reverted);
    assert!(
        restored.lock().unwrap().is_empty(),
        "green path never reverts"
    );
}

#[tokio::test]
async fn verify_failure_reverts_to_snapshot() {
    let (h, restored) = harness(
        &["PLAN: edit bar", "Edited bar."],
        Arc::new(AllowAll),
        failed("error[E0277]: the trait bound is not satisfied"),
        Some("snap-7".into()),
    );

    let outcome = h.run("add a bar").await.unwrap();

    assert!(outcome.executed);
    assert!(!outcome.verify.as_ref().unwrap().passed);
    assert!(outcome.reverted, "a failed verify reverts");
    assert_eq!(*restored.lock().unwrap(), vec!["snap-7".to_owned()]);
}

#[tokio::test]
async fn denied_plan_executes_nothing() {
    // Only the plan turn runs; the execute response is never needed.
    let (h, restored) = harness(
        &["PLAN: risky change"],
        Arc::new(DenyAll),
        passed("unused"),
        Some("snap-1".into()),
    );

    let outcome = h.run("do the risky thing").await.unwrap();

    assert!(!outcome.approved);
    assert!(!outcome.executed);
    assert!(outcome.verify.is_none());
    assert!(!outcome.reverted);
    assert!(outcome.report.contains("not approved"));
    assert!(restored.lock().unwrap().is_empty());
}

#[tokio::test]
async fn verify_failure_without_snapshot_degrades_to_report_only() {
    // No snapshot (e.g. not a git repo): a failed verify is surfaced but the
    // tree is NOT auto-reverted.
    let (h, restored) = harness(
        &["PLAN: edit baz", "Edited baz."],
        Arc::new(AllowAll),
        failed("1 test failed"),
        None,
    );

    let outcome = h.run("add a baz").await.unwrap();

    assert!(outcome.executed);
    assert!(!outcome.verify.as_ref().unwrap().passed);
    assert!(
        !outcome.reverted,
        "no snapshot → revert degrades to report-only"
    );
    assert!(restored.lock().unwrap().is_empty());
}
