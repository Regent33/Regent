//! Unit tests for `session_mix` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use regent_kernel::{ChatMessage, SessionId, ToolCall};

fn seed_session(
    store: &Store,
    id: &str,
    source: &str,
    turns: i64,
    input_tokens_per_call: i64,
    api_calls: i64,
    escalate: bool,
) {
    let sid = SessionId::from_string(id.to_owned());
    store
        .create_session(&sid, source, None, None, None)
        .unwrap();
    for _ in 0..turns {
        store
            .record_turn(&sid, "test-model", 1, "ok", None, now_epoch())
            .unwrap();
    }
    for _ in 0..api_calls {
        store
            .record_usage(&sid, input_tokens_per_call, 0, true)
            .unwrap();
    }
    if escalate {
        store
            .append_message(
                &sid,
                &ChatMessage::assistant(
                    None,
                    vec![ToolCall {
                        id: "call_1".into(),
                        name: "code_task".into(),
                        arguments: "{}".into(),
                    }],
                ),
                None,
                None,
            )
            .unwrap();
    }
}

#[test]
fn mix_aggregates_per_source_and_flags_escalation() {
    let store = Store::open_in_memory().unwrap();
    // Two chat sessions on "deacon" (one escalating), one non-escalating
    // "explore" session — a chat-dominant, lightly-escalating mix.
    seed_session(&store, "s1", "deacon", 3, 300, 3, false);
    seed_session(&store, "s2", "deacon", 5, 500, 4, true);
    seed_session(&store, "s3", "explore", 2, 200, 2, false);

    let report = store.session_mix(365.0).unwrap();

    assert_eq!(report.total_sessions, 3);
    assert_eq!(report.escalating_sessions, 1);
    assert!(
        (report.escalation_share - 1.0 / 3.0).abs() < 1e-9,
        "escalation_share: {}",
        report.escalation_share
    );

    let deacon = report
        .by_source
        .iter()
        .find(|s| s.source == "deacon")
        .expect("deacon source present");
    assert_eq!(deacon.session_count, 2);
    assert_eq!(deacon.total_turns, 8); // 3 + 5
    assert!((deacon.avg_turns_per_session - 4.0).abs() < 1e-9);
    assert_eq!(deacon.total_input_tokens, 900 + 2000); // 3*300 + 4*500
    assert!(
        (deacon.avg_input_tokens_per_call - (2900.0 / 7.0)).abs() < 1e-6,
        "avg_input_tokens_per_call: {}",
        deacon.avg_input_tokens_per_call
    );

    let explore = report
        .by_source
        .iter()
        .find(|s| s.source == "explore")
        .expect("explore source present");
    assert_eq!(explore.session_count, 1);
    assert_eq!(explore.total_turns, 2);
    assert!((explore.avg_turns_per_session - 2.0).abs() < 1e-9);
}

#[test]
fn outside_the_window_never_counts() {
    let store = Store::open_in_memory().unwrap();
    seed_session(&store, "s1", "deacon", 3, 300, 3, false);

    // A negative window pushes the cutoff into the future (now + 1 day), so
    // every session started "now" falls before it and is excluded — the
    // simplest way to prove the boundary is honored without relying on
    // clock-tick granularity.
    let report = store.session_mix(-1.0).unwrap();
    assert_eq!(report.total_sessions, 0);
    assert_eq!(report.escalation_share, 0.0, "no NaN on zero sessions");
    assert!(report.by_source.is_empty());
}
