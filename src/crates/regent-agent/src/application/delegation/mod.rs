//! Delegation — `delegate_task` spawns isolated sub-agents: each child gets
//! ONLY its task brief (never the parent's history), its own session and
//! budget. Parallel fan-out is bounded (`max_concurrent`) and
//! **order-preserving**: results re-attach in input order regardless of
//! completion order. Synchronous like Hermes: the parent waits.
//!
//! Bounded nesting (`max_depth`, default 2): a child below the cap gets the
//! leaf catalog **plus** its own depth+1 `delegate_task`, so it can fan out one
//! more level; a child at the cap gets the leaf catalog only (no delegate) —
//! the hard recursion stop. The leaf catalog the composition root injects
//! still decides what real tools (memory, terminal, …) children may use.
//!
//! This module owns the config + tool schema; `tool` owns the executor.
//!
//! or-colony note (ADR-008): its parallel mode is fail-fast `try_join_all`
//! without a concurrency cap; once those land upstream this becomes a
//! ColonyOrchestrator adapter.

mod tool;
pub use tool::DelegateTool;

use regent_kernel::ToolDefinition;
use serde_json::json;

#[derive(Clone)]
pub struct DelegationConfig {
    /// Hermes `delegation.max_concurrent_children` default.
    pub max_concurrent: usize,
    /// Hermes `delegation.max_iterations` default for children.
    pub child_max_iterations: u32,
    /// How many levels of delegation are allowed below the top-level tool.
    /// 1 = leaf children only (Hermes default behavior); 2 = a child may
    /// delegate once more. The hard recursion stop.
    pub max_depth: usize,
    pub child_system_prompt: String,
}

impl Default for DelegationConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 3,
            child_max_iterations: 50,
            max_depth: 2,
            child_system_prompt: "You are a focused worker agent. Complete exactly the task \
                                  you are given using your tools, then reply with a concise \
                                  summary of what you did and found."
                .to_owned(),
        }
    }
}

#[must_use]
pub fn delegate_definition() -> ToolDefinition {
    ToolDefinition {
        name: "delegate_task".into(),
        description: "Delegate work to isolated worker agents. Pass a single goal, or tasks \
                      (array) to run several workers in parallel. Workers see only their task \
                      plus the optional context string — never this conversation. Each returns \
                      a summary; results come back in task order."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "goal": {"type": "string", "description": "One task for one worker."},
                "tasks": {"type": "array", "items": {"type": "string"},
                          "description": "Several independent tasks run in parallel."},
                "context": {"type": "string", "description": "Shared brief prepended to every task."}
            }
        }),
        toolset: "delegation".into(),
    }
}
