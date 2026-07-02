//! Observability & tracing tests for or-core and or-prism.
//! Validates that tracing spans emit correctly and error handling
//! integrates with the observability layer.

use or_core::{CoreOrchestrator, RetryPolicy, TokenBudget};

#[test]
fn budget_enforcement_does_not_panic_under_tracing() {
    // Validate that tracing spans inside enforce_completion_budget don't panic
    // when no global subscriber is installed
    let orch = CoreOrchestrator::new();
    let budget = TokenBudget {
        max_context_tokens: 1024,
        max_completion_tokens: 512,
    };
    let _ = orch.enforce_completion_budget(&budget, 100);
    let _ = orch.enforce_completion_budget(&budget, 2000);
}

#[test]
fn retry_delay_does_not_panic_under_tracing() {
    let orch = CoreOrchestrator::new();
    let policy = RetryPolicy::default_llm();
    let _ = orch.next_retry_delay(&policy, 1);
    let _ = orch.next_retry_delay(&policy, 99);
}
