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

// The prompt layers, separated by role — see `domain::prompts`:
// SYSTEM_PROMPT (behavior/identity) · CONSTITUTIONAL_PROMPT (opt-in values
// layer) · CAPABILITIES (command-surface reference).
pub use domain::prompts::{
    CAPABILITIES, CODING_PROMPT, CONSTITUTIONAL_PROMPT, ConstitutionSection, EXPLORE_PROMPT,
    SYSTEM_PROMPT, VISUAL_EXPLAINER, WRAP_UP_PROMPT, constitution_chunks, constitution_core,
    constitution_sections, constitution_text,
};

pub use application::agent::{Agent, DeltaSink};
pub use application::board::{
    AgentReviewer, AgentTaskRunner, BoardDispatcher, ProviderResolver, ReviewVerdict, Reviewer,
    TaskOutcome, TaskRunner,
};
pub use application::cron_runner::AgentJobRunner;
pub use application::delegation::{DelegateTool, DelegationConfig, delegate_definition};
pub use application::mom::MomRunner;
pub use application::review::ReviewSetup;
pub use domain::config::{AgentConfig, CompressionConfig};
