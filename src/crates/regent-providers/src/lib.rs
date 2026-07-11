//! regent-providers — chat providers with native tool calling (canonical
//! `shared/infrastructure/http`, LLM edge).
//!
//! Clean-architecture internal layout: `domain/` (the `ChatProvider`
//! contract, request/response entities, typed errors), `application/`
//! (the sticky failover chain orchestrator), `infra/` (OpenAI-compatible
//! wire adapters + HTTP implementation). Retry/backoff/token accounting
//! reuse `or-core`; upstreaming a tool-call-native contract into Orchustr
//! is the long-term path (ADR-002).

pub mod application;
pub mod domain;
pub mod infra;

pub use application::orchestrators::{ActiveChangeFn, FallbackChat};
pub use domain::contracts::ChatProvider;
pub use domain::entities::{CachePolicy, CacheTtl, ChatRequest, ChatResponse};
pub use domain::errors::ProviderError;
pub use infra::anthropic_chat::{AnthropicChat, AnthropicChatConfig};
pub use infra::openai_compat::{OpenAiCompatChat, OpenAiCompatChatConfig};
