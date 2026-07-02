use crate::domain::entities::{BackoffStrategy, RetryPolicy, TokenBudget};
use crate::domain::errors::CoreError;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct CoreOrchestrator {
    strategy: BackoffStrategy,
}

impl Default for CoreOrchestrator {
    fn default() -> Self {
        Self {
            strategy: BackoffStrategy::ExponentialFullJitter,
        }
    }
}

impl CoreOrchestrator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use = "budget validation errors should be handled"]
    pub fn enforce_completion_budget(
        &self,
        budget: &TokenBudget,
        prompt_tokens: u32,
    ) -> Result<(), CoreError> {
        let span = tracing::info_span!(
            "core.enforce_completion_budget",
            otel.name = "core.enforce_completion_budget",
            prompt_tokens,
            status = tracing::field::Empty,
        );
        let _guard = span.enter();
        let requested = prompt_tokens.saturating_add(budget.max_completion_tokens);
        let result = if budget.fits(prompt_tokens, budget.max_completion_tokens) {
            Ok(())
        } else {
            Err(CoreError::BudgetExceeded {
                requested,
                budget: budget.max_context_tokens,
            })
        };
        span.record("status", if result.is_ok() { "success" } else { "failure" });
        result
    }

    #[must_use = "retry planning errors should be handled"]
    pub fn next_retry_delay(
        &self,
        policy: &RetryPolicy,
        attempt: u32,
    ) -> Result<Duration, CoreError> {
        let span = tracing::info_span!(
            "core.next_retry_delay",
            otel.name = "core.next_retry_delay",
            attempt,
            status = tracing::field::Empty,
        );
        let _guard = span.enter();
        let result = if attempt == 0 || attempt > policy.max_attempts {
            Err(CoreError::InvalidRetryAttempt {
                attempt,
                max_attempts: policy.max_attempts,
            })
        } else {
            Ok(Duration::from_millis(
                self.strategy.delay_ms(policy, attempt),
            ))
        };
        span.record("status", if result.is_ok() { "success" } else { "failure" });
        result
    }
}
