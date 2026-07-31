//! Interrupt semantics: pre-cancelled turns and mid-call cancellation.

use crate::helpers::{ScriptedProvider, echo_catalog, test_context, text_response};
use async_trait::async_trait;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::{RegentError, Role};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use std::sync::Arc;
use std::sync::Mutex;

#[tokio::test]
async fn pre_cancelled_turn_is_interrupted_before_any_model_call() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = ScriptedProvider::scripted(vec![text_response("never seen")]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        store,
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    agent.cancel_handle().cancel();

    let error = agent.run_turn("hello").await.unwrap_err();
    assert!(matches!(error, RegentError::Interrupted));
}

#[tokio::test]
async fn mid_call_interrupt_abandons_the_model_call_cleanly() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = ScriptedProvider::slow(std::time::Duration::from_secs(30));
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    let session = agent.session_id().clone();
    let cancel = agent.cancel_handle();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();
    });

    let error = agent.run_turn("hello").await.unwrap_err();
    assert!(matches!(error, RegentError::Interrupted));
    // The question survives the interruption, closed by a note saying no answer
    // came — dropping it left a follow-up ("proceed") alone in the transcript.
    // No PARTIAL model text enters history; the note is synthetic.
    let rows = store.get_conversation(&session).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].message.role, Role::User);
    assert_eq!(rows[0].message.content.as_deref(), Some("hello"));
    assert_eq!(rows[1].message.role, Role::Assistant);
    assert!(
        rows[1]
            .message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("interrupted me"),
        "the note must say the user interrupted, not that something failed"
    );
    // The interrupted turn is still in the reproducibility ledger.
    let turns = store.turns_for_session(&session).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].outcome, "interrupted");
}

/// Hangs on the first call (so a cancel lands mid-call) and records what the
/// SECOND turn actually sent to the model.
struct RecordsSecondTurn {
    calls: Mutex<usize>,
    second_request: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ChatProvider for RecordsSecondTurn {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let nth = {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            *calls
        };
        if nth == 1 {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
        *self.second_request.lock().unwrap() = request
            .messages
            .iter()
            .map(|m| m.content.clone().unwrap_or_default())
            .collect();
        Ok(text_response("here it is"))
    }

    fn model(&self) -> &str {
        "records-second-turn"
    }
}

/// The reported bug: barging in on "make a deck about Alice" and sending
/// "proceed" made Regent answer "what would you like me to proceed with?" —
/// the interrupted question had been deleted from history, so the follow-up
/// reached the model alone.
#[tokio::test]
async fn a_barged_in_question_is_still_in_context_for_the_follow_up() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let second_request = Arc::new(Mutex::new(Vec::new()));
    let provider: Arc<dyn ChatProvider> = Arc::new(RecordsSecondTurn {
        calls: Mutex::new(0),
        second_request: Arc::clone(&second_request),
    });
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        store,
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    let cancel = agent.cancel_handle();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();
    });
    assert!(matches!(
        agent.run_turn("make a deck about Alice").await.unwrap_err(),
        RegentError::Interrupted
    ));

    // What the deacon does between turns, and what a barge-in does: reset the
    // interrupt and send the next message.
    agent.reset_interrupt();
    agent.run_turn("proceed").await.unwrap();

    let sent = second_request.lock().unwrap().clone();
    assert!(
        sent.iter().any(|m| m.contains("make a deck about Alice")),
        "the interrupted question must still be in the request, got {sent:?}"
    );
    assert_eq!(sent.last().map(String::as_str), Some("proceed"));
}
