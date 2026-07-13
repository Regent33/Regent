//! Unit tests for `mod` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use async_trait::async_trait;
use regent_providers::{ChatResponse, ProviderError};
use std::sync::Mutex;

/// Returns a fixed reply (or errors), and records the last request's user
/// text so a test can assert what the aggregator actually saw.
struct Mock {
    reply: Option<&'static str>, // None ⇒ error (a failing proposer)
    last_user: Mutex<Option<String>>,
}
impl Mock {
    fn ok(reply: &'static str) -> Arc<Self> {
        Arc::new(Self {
            reply: Some(reply),
            last_user: Mutex::new(None),
        })
    }
    fn failing() -> Arc<Self> {
        Arc::new(Self {
            reply: None,
            last_user: Mutex::new(None),
        })
    }
}
#[async_trait]
impl ChatProvider for Mock {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let user = request
            .messages
            .last()
            .and_then(|m| m.content.clone())
            .unwrap_or_default();
        *self.last_user.lock().unwrap() = Some(user);
        match self.reply {
            Some(text) => Ok(ChatResponse {
                message: ChatMessage::assistant(Some(text.to_owned()), vec![]),
                usage: or_core::TokenUsage::default(),
                finish_reason: Some("stop".into()),
            }),
            None => Err(ProviderError::Parse("boom".into())),
        }
    }
    fn model(&self) -> &str {
        "mock"
    }
}

#[test]
fn aggregator_brief_includes_every_proposal_and_the_brief() {
    let b = aggregator_brief("do X", &["a".into(), "b".into(), "c".into()]);
    assert!(b.contains("do X"));
    assert!(b.contains("Proposal 1:\na"));
    assert!(b.contains("Proposal 2:\nb"));
    assert!(b.contains("Proposal 3:\nc"));
}

#[test]
fn aggregator_brief_with_no_proposals_is_just_the_brief() {
    assert_eq!(aggregator_brief("do X", &[]), "do X");
}

#[tokio::test]
async fn aggregator_sees_all_proposals_and_returns_its_synthesis() {
    let agg = Mock::ok("synthesized");
    let runner = MomRunner::new(
        vec![Mock::ok("alpha"), Mock::ok("beta"), Mock::ok("gamma")],
        Arc::clone(&agg) as Arc<dyn ChatProvider>,
    );
    let out = runner.run("the brief").await.unwrap();
    assert_eq!(out, "synthesized");
    let seen = agg.last_user.lock().unwrap().clone().unwrap();
    assert!(seen.contains("alpha") && seen.contains("beta") && seen.contains("gamma"));
    assert!(seen.contains("the brief"));
}

#[tokio::test]
async fn a_failing_proposer_is_skipped_not_fatal() {
    let agg = Mock::ok("synthesized");
    let runner = MomRunner::new(
        vec![Mock::ok("alpha"), Mock::failing(), Mock::ok("gamma")],
        Arc::clone(&agg) as Arc<dyn ChatProvider>,
    );
    let out = runner.run("brief").await.unwrap();
    assert_eq!(out, "synthesized");
    let seen = agg.last_user.lock().unwrap().clone().unwrap();
    assert!(seen.contains("alpha") && seen.contains("gamma"));
    assert!(!seen.contains("Proposal 3"), "only 2 survived");
}

#[tokio::test]
async fn max_proposers_caps_the_fan_out() {
    let agg = Mock::ok("synthesized");
    let runner = MomRunner::new(
        vec![Mock::ok("a"), Mock::ok("b"), Mock::ok("c"), Mock::ok("d")],
        Arc::clone(&agg) as Arc<dyn ChatProvider>,
    )
    .with_max_proposers(2);
    runner.run("brief").await.unwrap();
    let seen = agg.last_user.lock().unwrap().clone().unwrap();
    assert!(
        seen.contains("Proposal 2") && !seen.contains("Proposal 3"),
        "capped at 2"
    );
}
