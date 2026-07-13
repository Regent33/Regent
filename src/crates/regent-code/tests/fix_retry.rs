//! Gap H4 acceptance: a red verify feeds its failure output back to the SAME
//! execute agent for a bounded fix turn — red→green means one fix turn and no
//! revert; red all the way down means revert after `max_fix_attempts`.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::AgentConfig;
use regent_code::{Checkpoint, CodeHarness, Verifier, VerifyOutcome};
use regent_kernel::{ChatMessage, RegentError};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{AllowAll, ToolCatalog, ToolContext};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

struct ScriptedProvider(Mutex<VecDeque<String>>);

#[async_trait]
impl ChatProvider for ScriptedProvider {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let text = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Parse("script exhausted".into()))?;
        Ok(ChatResponse {
            message: ChatMessage::assistant(Some(text), vec![]),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                ..Default::default()
            },
            finish_reason: Some("stop".into()),
        })
    }
    fn model(&self) -> &str {
        "scripted-model"
    }
}

/// Verifier that yields a scripted sequence of outcomes, one per call.
struct SeqVerifier(Mutex<VecDeque<Option<VerifyOutcome>>>);

#[async_trait]
impl Verifier for SeqVerifier {
    async fn verify(&self, _workspace: &Path) -> Result<Option<VerifyOutcome>, RegentError> {
        Ok(self.0.lock().unwrap().pop_front().flatten())
    }
}

struct StubCheckpoint(Arc<Mutex<Vec<String>>>);

#[async_trait]
impl Checkpoint for StubCheckpoint {
    async fn snapshot(&self) -> Result<Option<String>, RegentError> {
        Ok(Some("snap-1".into()))
    }
    async fn restore(&self, id: &str) -> Result<(), RegentError> {
        self.0.lock().unwrap().push(id.to_owned());
        Ok(())
    }
}

fn red(summary: &str) -> Option<VerifyOutcome> {
    Some(VerifyOutcome {
        passed: false,
        summary: summary.into(),
    })
}

fn green() -> Option<VerifyOutcome> {
    Some(VerifyOutcome {
        passed: true,
        summary: "ok. 3 passed".into(),
    })
}

fn harness(
    texts: &[&str],
    verdicts: Vec<Option<VerifyOutcome>>,
) -> (CodeHarness, Arc<Mutex<Vec<String>>>) {
    let restored = Arc::new(Mutex::new(Vec::new()));
    let h = CodeHarness::new(
        Arc::new(ScriptedProvider(Mutex::new(
            texts.iter().map(|t| (*t).to_owned()).collect(),
        ))),
        Arc::new(ToolCatalog::new()),
        Arc::new(Store::open_in_memory().unwrap()),
        ToolContext::new(std::env::temp_dir(), Arc::new(AllowAll)),
        "system",
        AgentConfig::default(),
        Arc::new(SeqVerifier(Mutex::new(verdicts.into()))),
        Arc::new(StubCheckpoint(Arc::clone(&restored))),
    );
    (h, restored)
}

#[tokio::test]
async fn red_then_green_runs_one_fix_turn_and_keeps_changes() {
    let (h, restored) = harness(
        &["PLAN: edit foo", "Edited foo.", "Fixed the type error."],
        vec![red("error[E0308]: mismatched types"), green()],
    );

    let outcome = h.run("add a foo").await.unwrap();

    assert_eq!(outcome.fix_attempts, 1);
    assert!(outcome.verify.as_ref().unwrap().passed);
    assert!(!outcome.reverted);
    assert_eq!(outcome.report, "Fixed the type error.");
    assert!(
        restored.lock().unwrap().is_empty(),
        "green end never reverts"
    );
}

#[tokio::test]
async fn red_all_the_way_reverts_after_bounded_attempts() {
    let (h, restored) = harness(
        &[
            "PLAN: edit bar",
            "Edited bar.",
            "Tried fix 1.",
            "Tried fix 2.",
        ],
        vec![
            red("2 tests failed"),
            red("2 tests failed"),
            red("1 test failed"),
        ],
    );

    let outcome = h.run("add a bar").await.unwrap();

    assert_eq!(outcome.fix_attempts, 2, "bounded at max_fix_attempts");
    assert!(!outcome.verify.as_ref().unwrap().passed);
    assert!(outcome.reverted, "still red after the budget → revert");
    assert_eq!(*restored.lock().unwrap(), vec!["snap-1".to_owned()]);
}

#[tokio::test]
async fn zero_max_fix_attempts_restores_one_shot_revert() {
    let (h, restored) = harness(
        &["PLAN: edit baz", "Edited baz."],
        vec![red("1 test failed")],
    );
    let outcome = h.with_max_fix_attempts(0).run("add a baz").await.unwrap();

    assert_eq!(outcome.fix_attempts, 0);
    assert!(outcome.reverted);
    assert_eq!(restored.lock().unwrap().len(), 1);
}
