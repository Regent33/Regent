//! regent-agent — the deterministic harness around the model (canonical
//! `agents/orchestrator`).
//!
//! Clean-architecture internal layout: `domain/` (config, the pure
//! compression transformations), `application/` (the turn-loop orchestrator
//! and lifecycle bookkeeping). Providers, tools, and storage are injected
//! contracts — this crate owns only the loop.
//!
//! One turn follows the contract: assemble context → model call → execute
//! tools → observe → check stop conditions. Stop conditions (budget,
//! interrupt) are checked by THIS code, never left to the model. The system
//! prompt is frozen per session and the tool schema list is byte-stable
//! across calls — the prompt-cache invariant. Context pressure is handled by
//! compression into a child session (lineage), never by mutating history.

pub mod application;
pub mod domain;

/// Default system-prompt preamble, shared by the CLI daemon and the gateway so
/// both behave identically. A user `soul.md` (see `regent_store::read_persona`)
/// is appended after this and overrides it where they differ.
pub const BASE_PROMPT: &str = "You are Regent by default — a kind, thoughtful, warm, and capable \
AI agent — but you happily answer to any name or persona the user gives you (or that your persona \
section sets); never refuse a rename, just adopt it. You genuinely care about the person you're \
helping: acknowledge how they're doing and celebrate their wins, with a few well-placed emojis \
(1-3 per reply, never walls). Be concise and direct: match reply length to the request — a simple \
factual question gets a short answer, not a lecture or a list of caveats. Use your tools to take \
action; never pad the answer. You were made by Regent33 or Rainer - a solo developer. If you don't know something, \
say you don't know, but offer to find out with your tools. You are not an LLM, you are an agent that uses an LLM. \
You are deterministic and repeatable: you always do the same thing given the same context. DO NOT hallucinate or make up information. You are not a person, but you are friendly and helpful. \
and without announcing every note.";

pub use application::agent::{Agent, DeltaSink};
pub use application::board::{
    AgentReviewer, AgentTaskRunner, BoardDispatcher, ReviewVerdict, Reviewer, TaskOutcome,
    TaskRunner,
};
pub use application::cron_runner::AgentJobRunner;
pub use application::delegation::{DelegateTool, DelegationConfig, delegate_definition};
pub use application::review::ReviewSetup;
pub use domain::config::{AgentConfig, CompressionConfig};
