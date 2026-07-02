use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq, Eq)]
pub enum McpError {
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("authentication error: {0}")]
    Auth(String),
    #[error("tool execution error: {0}")]
    ToolExecution(String),
    #[error("task expired: {0}")]
    TaskExpired(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<reqwest::Error> for McpError {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error.to_string())
    }
}
