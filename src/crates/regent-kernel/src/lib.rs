//! regent-kernel — the only freely-importable layer of the Regent workspace
//! (canonical `shared/kernel`): `types/` (branded IDs, messages, the
//! alternation-enforcing transcript, base Failure) and `contracts/` (the
//! tool definition contract). No I/O, no framework imports.

pub mod contracts;
pub mod redact;
pub mod types;

pub use contracts::embedding::EmbeddingProvider;
pub use redact::{RedactingWriter, redact_secrets};
pub use contracts::tool::{ToolDefinition, tool_error_json, tool_result_json};
pub use types::error::RegentError;
pub use types::id::{SessionId, TaskId};
pub use types::message::{ChatMessage, Role, ToolCall};
pub use types::transcript::Transcript;
