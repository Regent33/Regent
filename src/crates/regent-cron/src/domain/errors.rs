use regent_kernel::RegentError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CronError {
    #[error("invalid schedule: {0}")]
    InvalidSchedule(String),

    #[error("job storage failure: {0}")]
    Storage(String),

    #[error("job run failed: {0}")]
    RunFailed(String),
}

impl From<std::io::Error> for CronError {
    fn from(value: std::io::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<RegentError> for CronError {
    fn from(value: RegentError) -> Self {
        Self::RunFailed(value.to_string())
    }
}
