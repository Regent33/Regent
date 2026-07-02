use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid retry attempt: attempt={attempt}, max_attempts={max_attempts}")]
    InvalidRetryAttempt { attempt: u32, max_attempts: u32 },
    #[error("token budget exceeded: requested={requested}, budget={budget}")]
    BudgetExceeded { requested: u32, budget: u32 },
    #[error("invalid state: {0}")]
    InvalidState(String),
}
