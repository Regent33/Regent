//! regent-graph — the memory domain engine over `regent-store`'s graph
//! tables (canonical `agents/memory`, semantic + episodic tiers).
//!
//! Clean-architecture internal layout: `domain/` (entities, errors, the
//! pure write-policy rules), `application/` (the GraphMemory orchestrator,
//! bounded-store use cases, hybrid retrieval). Persistence is injected —
//! `regent-store` is this crate's infra.
//!
//! Storage is write-through: there is no flush step to forget.

pub mod application;
pub mod domain;

pub use application::evals;
pub use application::orchestrators::{GraphMemory, MemoryNode};
pub use domain::entities::{AddOutcome, MemoryTarget, Provenance, Recalled};
pub use domain::errors::GraphError;
pub use domain::policy;
