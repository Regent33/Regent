use thiserror::Error;

/// Typed failures shared across the workspace. Crate-local error types
/// (store, provider) convert into this at the boundary they cross.
#[derive(Debug, Error)]
pub enum RegentError {
    /// A transcript append would violate message-role alternation.
    #[error("transcript invariant violated: {0}")]
    Transcript(String),

    /// The model/provider call failed after retries.
    #[error("provider failure: {0}")]
    Provider(String),

    /// A tool executor failed in a way that could not be wrapped as a
    /// JSON error result (executor panics are wrapped, this is plumbing).
    #[error("tool failure ({tool}): {message}")]
    Tool { tool: String, message: String },

    /// Persistence failure.
    #[error("store failure: {0}")]
    Store(String),

    /// The iteration budget was exhausted before a final response.
    #[error("iteration budget exhausted after {0} calls")]
    BudgetExhausted(u32),

    /// The turn was interrupted (user cancellation / shutdown).
    #[error("interrupted")]
    Interrupted,

    /// Configuration is missing or invalid.
    #[error("config error: {0}")]
    Config(String),
}
