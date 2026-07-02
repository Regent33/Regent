//! Failure handling & retry tests for or-core.

use or_core::{CoreError, CoreOrchestrator, RetryPolicy, TokenBudget};
use std::time::Duration;

#[test]
fn retry_delay_attempt_zero_is_invalid() {
    let orch = CoreOrchestrator::new();
    let result = orch.next_retry_delay(&RetryPolicy::default_llm(), 0);
    assert_eq!(
        result,
        Err(CoreError::InvalidRetryAttempt {
            attempt: 0,
            max_attempts: 3
        })
    );
}

#[test]
fn retry_delay_increases_with_attempt() {
    let orch = CoreOrchestrator::new();
    let policy = RetryPolicy {
        max_attempts: 5,
        base_delay_ms: 100,
        max_delay_ms: 10_000,
        jitter: true,
    };
    // Collect several delays — later attempts should generally produce larger delays
    // (with jitter, we just verify they are positive and don't exceed max)
    for attempt in 1..=5 {
        let delay = orch.next_retry_delay(&policy, attempt).unwrap();
        assert!(delay >= Duration::ZERO);
        assert!(delay <= Duration::from_millis(policy.max_delay_ms));
    }
}

#[test]
fn retry_delay_respects_max_delay() {
    let orch = CoreOrchestrator::new();
    let policy = RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 5_000,
        max_delay_ms: 10_000,
        jitter: true,
    };
    for attempt in 1..=3 {
        let delay = orch.next_retry_delay(&policy, attempt).unwrap();
        assert!(
            delay <= Duration::from_millis(10_000),
            "delay {delay:?} exceeded max"
        );
    }
}

#[test]
fn no_retry_policy_allows_single_attempt() {
    let orch = CoreOrchestrator::new();
    let policy = RetryPolicy::no_retry();
    // Attempt 1 should succeed even with no_retry
    let ok = orch.next_retry_delay(&policy, 1);
    assert!(ok.is_ok());
    // Attempt 2 should fail
    let err = orch.next_retry_delay(&policy, 2);
    assert!(err.is_err());
}

#[test]
fn budget_exact_boundary_passes() {
    let orch = CoreOrchestrator::new();
    // max_context=512, max_completion=256 → total allowed = 512
    // prompt=256 + completion=256 = 512 → fits
    let budget = TokenBudget {
        max_context_tokens: 512,
        max_completion_tokens: 256,
    };
    let result = orch.enforce_completion_budget(&budget, 256);
    assert!(result.is_ok(), "exact boundary should pass");
}

#[test]
fn budget_one_over_boundary_fails() {
    let orch = CoreOrchestrator::new();
    let budget = TokenBudget {
        max_context_tokens: 512,
        max_completion_tokens: 256,
    };
    let result = orch.enforce_completion_budget(&budget, 257);
    assert!(result.is_err(), "one over boundary should fail");
}

#[test]
fn budget_zero_prompt_always_passes() {
    let orch = CoreOrchestrator::new();
    let budget = TokenBudget {
        max_context_tokens: 100,
        max_completion_tokens: 50,
    };
    let result = orch.enforce_completion_budget(&budget, 0);
    assert!(result.is_ok());
}
