use regent_kernel::RegentError;
use regent_store::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("store failure: {0}")]
    Store(#[from] StoreError),

    /// The memory contract: never auto-compact — tell the agent to
    /// consolidate in the same turn. Carries everything it needs to do so.
    #[error(
        "memory at {used}/{limit} bytes; adding {attempted} bytes would exceed the limit. \
             Consolidate now: replace overlapping entries with shorter ones or remove stale \
             entries, then retry. (Bytes, not characters: accented and non-Latin text costs 2-4 \n             bytes each.) If the store is ALREADY over its limit, only removing entries can \n             bring it back under"
    )]
    BudgetExceeded {
        used: usize,
        limit: usize,
        attempted: usize,
        entries: Vec<String>,
    },

    #[error("'{0}' matches multiple entries — use a more specific substring")]
    AmbiguousMatch(String),

    #[error("'{0}' does not match any entry")]
    NoMatch(String),

    /// Write-policy rejection (injection patterns, invisible unicode, size).
    #[error("content rejected: {0}")]
    Rejected(String),

    /// Embedding generation failed — the vector lane degrades to FTS+graph.
    #[error("embedding failure: {0}")]
    Embedding(String),
}

impl From<GraphError> for RegentError {
    fn from(value: GraphError) -> Self {
        RegentError::Store(value.to_string())
    }
}
