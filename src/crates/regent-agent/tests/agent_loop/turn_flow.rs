//! Happy-path turns, budget/token ceilings, and the turns ledger.
//! (Repair retries live in turn_repairs.rs; deferred-tool recovery in
//! turn_recovery.rs.)

use crate::helpers::{
    ScriptedProvider, call, echo_catalog, test_context, text_response, tool_call_response,
};
use regent_agent::{Agent, AgentConfig};
use regent_kernel::Role;
use regent_store::Store;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn tool_round_trip_turn_persists_everything_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let provider = ScriptedProvider::scripted(vec![
        tool_call_response(vec![
            call("a", "echo", json!({"text": "one"})),
            call("b", "echo", json!({"text": "two"})),
        ]),
        text_response("all done"),
    ]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    let reply = agent.run_turn("run the echoes").await.unwrap();
    assert_eq!(reply, "all done");

    let rows = store.get_conversation(agent.session_id()).unwrap();
    let roles: Vec<Role> = rows.iter().map(|r| r.message.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Tool,
            Role::Assistant
        ]
    );
    // results re-attached in original call order
    assert_eq!(rows[2].message.tool_call_id.as_deref(), Some("a"));
    assert!(rows[2].message.content.as_deref().unwrap().contains("one"));
    assert_eq!(rows[3].message.tool_call_id.as_deref(), Some("b"));
}

// Gap L2: budget exhaustion is a graceful wrap-up, not a hard error — the
// turn returns Ok(summary) while the ledger still records `budget_exhausted`.
#[tokio::test]
async fn budget_ceiling_wraps_up_runaway_loops() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let config = AgentConfig {
        max_iterations: 3,
        ..AgentConfig::default()
    };
    let mut agent = Agent::new(
        ScriptedProvider::runaway(),
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        config,
    )
    .unwrap();

    // The runaway provider answers the wrap-up call with another tool-call
    // response (no text) — the fallback summary still comes back as Ok, the
    // stray tool calls are dropped, and the transcript stays legal.
    let reply = agent.run_turn("go").await.unwrap();
    assert!(reply.contains("budget exhausted"), "got: {reply}");
    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns[0].outcome, "budget_exhausted");
    // 3 working calls + 1 wrap-up call.
    assert_eq!(turns[0].api_calls, 4);
}

#[tokio::test]
async fn budget_wrap_up_returns_the_models_summary() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let config = AgentConfig {
        max_iterations: 2,
        ..AgentConfig::default()
    };
    let provider = ScriptedProvider::scripted(vec![
        tool_call_response(vec![call("a", "echo", json!({"text": "1"}))]),
        tool_call_response(vec![call("b", "echo", json!({"text": "2"}))]),
        // This response answers the tool-less wrap-up call.
        text_response("Done: X. Remaining: Y. Resume at Z."),
    ]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        config,
    )
    .unwrap();

    let reply = agent.run_turn("go").await.unwrap();
    assert_eq!(reply, "Done: X. Remaining: Y. Resume at Z.");
    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns[0].outcome, "budget_exhausted");
}

#[tokio::test]
async fn token_ceiling_halts_the_turn_before_max_iterations() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    // Each runaway call spends 15 tokens (prompt 10 + completion 5). A 20-token
    // ceiling admits the first two calls (running total 0, then 15) and wraps
    // up on the third (30 ≥ 20) — well before the 90-iteration default ceiling.
    // Proves the per-turn token cap bounds spend independently of the step
    // count (W2.4); the ledger carries the exhaustion either way.
    let config = AgentConfig {
        max_turn_tokens: Some(20),
        ..AgentConfig::default()
    };
    let mut agent = Agent::new(
        ScriptedProvider::runaway(),
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        config,
    )
    .unwrap();

    agent.run_turn("go").await.unwrap();
    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns[0].outcome, "budget_exhausted");
    // 2 working calls before the ceiling + 1 wrap-up call.
    assert_eq!(
        turns[0].api_calls, 3,
        "token ceiling should halt after 2 working calls (30 tokens)"
    );
}

#[tokio::test]
async fn turns_ledger_records_outcome_and_call_count() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = ScriptedProvider::scripted(vec![
        tool_call_response(vec![call("a", "echo", json!({"text": "x"}))]),
        text_response("done"),
    ]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    agent.run_turn("go").await.unwrap();

    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].outcome, "ok");
    assert_eq!(turns[0].api_calls, 2);
    assert_eq!(turns[0].model.as_deref(), Some("scripted-model"));
    assert!(turns[0].ended_at >= turns[0].started_at);
}

/// The timing buckets must survive the turn that produced them, and must not
/// out-total the turn they belong to.
///
/// `store_ms` was reset AFTER the user message had already been persisted, so
/// the one write the instrument was added to measure — a blocking SQLite append
/// before the first model call — was computed and then thrown away every turn.
/// Its cost silently joined the unattributed residual, which the docs promise
/// contains only in-memory work.
#[tokio::test]
async fn turn_timings_survive_the_write_they_measure_and_fit_inside_the_turn() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let provider = ScriptedProvider::scripted(vec![
        tool_call_response(vec![call("a", "echo", json!({"text": "one"}))]),
        text_response("done"),
    ]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    agent.run_turn("go").await.unwrap();
    let t = agent.last_turn_timings();

    // THE assertion. Every message this turn persisted must have been billed to
    // this turn — so the count must equal the rows in the store.
    //
    // A duration cannot carry this. A temp-dir SQLite append is sub-millisecond,
    // so `store_ms` floors to 0 and an assertion on it passes or fails on
    // machine speed. Worse, the earlier version of this test asserted only
    // `summed <= total_ms`, and DISCARDING a bucket's time can only make
    // `summed` smaller — a one-sided bound is monotonically insensitive to the
    // very bug it was named after, and passed with the defect reintroduced.
    let persisted = store.get_conversation(agent.session_id()).unwrap().len();
    assert_eq!(
        t.message_writes as usize, persisted,
        "the turn wrote {persisted} messages but billed {} — a write happened \
         outside the window this turn accounts for (the reset running after the \
         user message is how this broke before): {t:?}",
        t.message_writes
    );
    assert!(
        persisted >= 4,
        "user + assistant + tool + final: {persisted}"
    );

    // Parts of a turn cannot outlast it. This is what broke when `total_ms` was
    // stamped before the recovery `persist` calls that then added to `store_ms`.
    let summed = t.model_ms + t.tools_ms + t.store_ms + t.compact_ms + t.levers_ms;
    assert!(
        summed <= t.total_ms,
        "buckets sum to {summed}ms but the turn took {}ms: {t:?}",
        t.total_ms
    );

    // Per-turn, not cumulative. A second turn must re-zero the COUNT too —
    // which it can only do if the reset precedes that turn's first write.
    let before = t.message_writes;
    agent.run_turn("and again").await.unwrap_err();
    let second = agent.last_turn_timings();
    assert!(
        second.message_writes < before + persisted as u32,
        "second turn accumulated the first turn's writes: {second:?}"
    );
}
