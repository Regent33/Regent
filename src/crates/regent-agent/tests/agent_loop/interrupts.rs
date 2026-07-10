//! Interrupt semantics: pre-cancelled turns and mid-call cancellation.

use crate::helpers::{ScriptedProvider, echo_catalog, test_context, text_response};
use regent_agent::{Agent, AgentConfig};
use regent_kernel::{RegentError, Role};
use regent_store::Store;
use std::sync::Arc;

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
    // Only the user message persisted — no partial assistant entered history.
    let rows = store.get_conversation(&session).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.role, Role::User);
    // The interrupted turn is still in the reproducibility ledger.
    let turns = store.turns_for_session(&session).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].outcome, "interrupted");
}
