//! regent-voice-server — the local speech server (Rust port of
//! `python-voice-server`): OpenAI-compatible `/v1/audio/*` endpoints plus the
//! hands-free browser call at `/call`, backed by the full agent deacon over
//! stdio JSON-RPC.
//!
//! Layout per ADR-007: `domain/` holds the pure turn logic (text sanitizers,
//! sentence streaming, RPC line routing); `application/` the turn pipeline;
//! `infra/` the deacon client, engine ports, and the hardened HTTP surface.

pub mod application;
pub mod domain;
pub mod infra;

pub use domain::rpc::{RpcEvent, classify};
pub use domain::sentences::SentenceSplitter;
pub use domain::speakable::{strip_markdown, strip_spoken};
