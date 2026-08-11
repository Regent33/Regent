use regent_kernel::RegentError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("write contention: still busy after {attempts} attempts")]
    Contention { attempts: u32 },

    #[error("corrupt row: {0}")]
    CorruptRow(String),

    #[error("unknown session: {0}")]
    UnknownSession(String),

    #[error(
        "persona '{key}' would be {attempted} bytes — over its {limit}-byte budget. The persona \
         rides EVERY turn's system prompt, so keep it tight: consolidate to durable \
         identity/preferences and store episodic facts in memory instead. (Bytes, not \
         characters: accented and non-Latin text costs 2-4 bytes each, so it fits fewer \
         characters than English does)"
    )]
    PersonaBudget {
        key: String,
        attempted: usize,
        limit: usize,
    },

    #[error("profile: {0}")]
    Profile(String),
}

impl From<StoreError> for RegentError {
    fn from(value: StoreError) -> Self {
        RegentError::Store(value.to_string())
    }
}
