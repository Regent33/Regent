//! Unit tests for `profile_model` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::{BilledInputs, CacheModel, billed_tokens_a_vs_b};

const TEST_MODEL: CacheModel = CacheModel {
    name: "test",
    write_mult: 1.25,
    read_mult: 0.1,
};

#[test]
fn chat_dominant_low_escalation_mix_favors_two_profiles() {
    let mix = BilledInputs {
        light_tokens: 2_000.0,
        full_tokens: 4_000.0,
        escalation_share: 0.1,
        avg_turns: 10.0,
        chat_sessions: 100.0,
        agentic_sessions: 0.0,
    };
    let (a, b) = billed_tokens_a_vs_b(&mix, &TEST_MODEL);
    assert!(
        a < b,
        "chat-dominant, low-escalation mix: A should win (a={a}, b={b})"
    );
}

#[test]
fn near_universal_escalation_favors_the_single_prefix() {
    let mix = BilledInputs {
        light_tokens: 2_000.0,
        full_tokens: 4_000.0,
        escalation_share: 0.95,
        avg_turns: 4.0,
        chat_sessions: 100.0,
        agentic_sessions: 0.0,
    };
    let (a, b) = billed_tokens_a_vs_b(&mix, &TEST_MODEL);
    assert!(
        b < a,
        "near-universal escalation: B should win (a={a}, b={b})"
    );
}

#[test]
fn all_agentic_sessions_cost_the_same_either_way() {
    // No chat sessions to split into light/full — A and B both reduce to
    // "1 full write + (turns-1) full reads per session" with no escalation
    // tax, so they must land on the same total.
    let mix = BilledInputs {
        light_tokens: 2_000.0,
        full_tokens: 4_000.0,
        escalation_share: 0.0,
        avg_turns: 10.0,
        chat_sessions: 0.0,
        agentic_sessions: 50.0,
    };
    let (a, b) = billed_tokens_a_vs_b(&mix, &TEST_MODEL);
    assert!((a - b).abs() < 1e-6, "no chat sessions to split on: a={a} b={b}");
}

#[test]
fn zero_sessions_bill_nothing() {
    let mix = BilledInputs {
        light_tokens: 2_000.0,
        full_tokens: 4_000.0,
        escalation_share: 0.0,
        avg_turns: 1.0,
        chat_sessions: 0.0,
        agentic_sessions: 0.0,
    };
    let (a, b) = billed_tokens_a_vs_b(&mix, &TEST_MODEL);
    assert_eq!(a, 0.0);
    assert_eq!(b, 0.0);
}
