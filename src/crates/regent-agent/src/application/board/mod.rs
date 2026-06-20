//! Kanban board dispatcher — the orchestration core for multi-agent work
//! (P6). This module owns the contracts ([`TaskRunner`], [`Reviewer`]) and
//! splits the implementation: `dispatcher` (claim → run → review), `runner`
//! (the agent-backed [`TaskRunner`]).
//!
//! Each dispatch pass claims the next `todo` task on a board (atomically, so
//! two dispatchers never double-run it), runs it, and resolves it by the
//! board's review policy. A failed run auto-blocks; a clean run is *reviewed*
//! before it can reach `done` — the dispatcher never auto-completes work.
//!
//! Columns: `todo → in_progress → in_review → done`, with `blocked` reachable
//! from anywhere.

mod dispatcher;
mod reviewer;
mod runner;

pub use dispatcher::{BoardDispatcher, TaskOutcome};
pub use reviewer::AgentReviewer;
pub use runner::AgentTaskRunner;

use async_trait::async_trait;
use regent_kernel::RegentError;
use regent_store::KanbanTaskRow;

/// Runs one claimed task. `Ok(summary)` completes it; `Err` blocks it.
#[async_trait]
pub trait TaskRunner: Send + Sync {
    async fn run(&self, task: &KanbanTaskRow) -> Result<String, RegentError>;
}

/// A reviewer's verdict on submitted work (the `agent` review policy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewVerdict {
    Approve,
    /// Send back for rework, carrying the reviewer's reason.
    Reject(String),
}

/// Judges a task's finished work. Injected like [`TaskRunner`] so the `agent`
/// review policy stays testable without a live model.
#[async_trait]
pub trait Reviewer: Send + Sync {
    async fn review(&self, task: &KanbanTaskRow, work: &str) -> Result<ReviewVerdict, RegentError>;
}
