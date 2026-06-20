use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("core: {0}")]
    Core(#[from] regent_kernel::RegentError),
    #[error("store: {0}")]
    Store(#[from] regent_store::StoreError),
    #[error("config: {0}")]
    Config(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
}
