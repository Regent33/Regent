//! regent-store — single-file SQLite persistence (canonical
//! `shared/infrastructure/storage`): WAL mode, sessions, full message
//! history, FTS5 search, turns ledger, and the graph-memory tables.
//!
//! Clean-architecture internal layout: `domain/` holds row entities and
//! typed errors (pure data); `infra/` holds the connection policy, schema,
//! and every SQL implementation. Memory *semantics* live in `regent-graph`.
//!
//! The API is synchronous by design; async callers wrap calls in
//! `tokio::task::spawn_blocking` (the agent does this in one place).

pub mod domain;
pub mod infra;

pub use domain::entities::{
    BoardRow, InsightsRollup, KanbanTaskRow, NeighborRow, NodeRow, PendingWriteRow, ReviewPolicy,
    SearchHit, SessionMeta, StoredMessage, TurnRecord,
};
pub use domain::errors::StoreError;
pub use infra::db::{Store, now_epoch};
pub use infra::persona::{ABOUT_SECTIONS, is_valid_persona_key};
