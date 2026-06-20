use regent_kernel::RegentError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("skill not found: {0}")]
    NotFound(String),

    #[error("skill already exists: {0}")]
    AlreadyExists(String),

    #[error("invalid skill {field}: {reason}")]
    Invalid { field: &'static str, reason: String },

    #[error("skill '{0}' is pinned — pinned skills are exempt from archive")]
    Pinned(String),

    #[error("'{0}' does not match exactly one occurrence in the skill body")]
    PatchMismatch(String),

    #[error("path escapes the skill directory: {0}")]
    PathEscape(String),

    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<std::io::Error> for SkillError {
    fn from(value: std::io::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<SkillError> for RegentError {
    fn from(value: SkillError) -> Self {
        RegentError::Store(value.to_string())
    }
}
