//! Anthropic Messages API wire adapters (request building, response parsing,
//! streaming accumulation). Pure functions/types — no network — so each piece
//! is unit-testable in isolation.

pub mod messages;
pub mod request;
pub mod response;
pub mod stream;

pub use request::{build_payload, build_streaming_payload};
pub use response::parse_response;
pub use stream::StreamAccumulator;
