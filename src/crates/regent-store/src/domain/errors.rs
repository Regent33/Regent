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
}

impl From<StoreError> for RegentError {
    fn from(value: StoreError) -> Self {
        RegentError::Store(value.to_string())
    }
}
