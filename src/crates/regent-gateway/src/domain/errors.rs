use regent_kernel::RegentError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("platform transport failure: {0}")]
    Transport(String),

    #[error("platform response parse failure: {0}")]
    Parse(String),

    #[error("conversation failure: {0}")]
    Conversation(String),
}

impl From<RegentError> for GatewayError {
    fn from(value: RegentError) -> Self {
        Self::Conversation(value.to_string())
    }
}
