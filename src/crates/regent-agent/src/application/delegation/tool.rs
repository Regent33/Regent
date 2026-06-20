//! The `delegate_task` executor: builds each child's catalog (depth-bounded),
//! runs the bounded order-preserving fan-out, and isolates per-child failures.

use super::{DelegationConfig, delegate_definition};
use crate::application::agent::Agent;
use crate::domain::config::AgentConfig;
use async_trait::async_trait;
use futures::StreamExt;
use regent_kernel::{RegentError, tool_error_json};
use regent_providers::ChatProvider;
use regent_store::Store;
use regent_tools::{ToolCatalog, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct DelegateTool {
    provider: Arc<dyn ChatProvider>,
    store: Arc<Store>,
    leaf_catalog: Arc<ToolCatalog>,
    config: DelegationConfig,
    /// This tool's nesting level (top-level = 1). Children spawned here run at
    /// the same level; the delegate tool *they* may receive is `depth + 1`.
    depth: usize,
}

impl DelegateTool {
    #[must_use]
    pub fn new(
        provider: Arc<dyn ChatProvider>,
        store: Arc<Store>,
        leaf_catalog: Arc<ToolCatalog>,
        config: DelegationConfig,
    ) -> Self {
        Self { provider, store, leaf_catalog, config, depth: 1 }
    }

    pub fn register(self, catalog: &mut ToolCatalog) -> Result<(), RegentError> {
        catalog.register(delegate_definition(), Arc::new(self))
    }

    /// The catalog a child spawned here receives. Below the depth cap it is the
    /// leaf catalog plus a `depth + 1` delegate tool (so the child can nest one
    /// more level); at the cap it is the leaf catalog only — no `delegate_task`.
    fn child_catalog(&self) -> Arc<ToolCatalog> {
        if self.depth >= self.config.max_depth {
            return Arc::clone(&self.leaf_catalog);
        }
        let mut catalog = (*self.leaf_catalog).clone();
        let deeper = DelegateTool {
            provider: Arc::clone(&self.provider),
            store: Arc::clone(&self.store),
            leaf_catalog: Arc::clone(&self.leaf_catalog),
            config: self.config.clone(),
            depth: self.depth + 1,
        };
        // Leaf catalogs never carry `delegate_task`; ignore a (impossible)
        // duplicate rather than failing the whole delegation.
        let _ = catalog.register(delegate_definition(), Arc::new(deeper));
        Arc::new(catalog)
    }

    async fn run_child(&self, task: String, context_note: String, ctx: ToolContext) -> Value {
        let brief = if context_note.is_empty() {
            task.clone()
        } else {
            format!("Context: {context_note}\n\nTask: {task}")
        };
        let config = AgentConfig {
            source: "delegate".to_owned(),
            max_iterations: self.config.child_max_iterations,
            ..AgentConfig::default()
        };
        let child = Agent::new(
            Arc::clone(&self.provider),
            self.child_catalog(),
            Arc::clone(&self.store),
            ctx,
            self.config.child_system_prompt.clone(),
            config,
        );
        match child {
            Ok(mut agent) => match agent.run_turn(&brief).await {
                Ok(summary) => json!({
                    "task": task, "status": "ok", "summary": summary,
                    "session_id": agent.session_id().as_str(),
                }),
                Err(error) => json!({"task": task, "status": "failed", "summary": error.to_string()}),
            },
            Err(error) => json!({"task": task, "status": "failed", "summary": error.to_string()}),
        }
    }
}

#[async_trait]
impl ToolExecutor for DelegateTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let tasks: Vec<String> = match args.get("tasks").and_then(Value::as_array) {
            Some(array) => array
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect(),
            None => match args.get("goal").and_then(Value::as_str) {
                Some(goal) => vec![goal.to_owned()],
                None => return Ok(tool_error_json("provide 'goal' or non-empty 'tasks'")),
            },
        };
        if tasks.is_empty() {
            return Ok(tool_error_json("provide 'goal' or non-empty 'tasks'"));
        }
        let context_note = args
            .get("context")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        tracing::info!(children = tasks.len(), cap = self.config.max_concurrent, "delegation fan-out");

        // buffered() bounds concurrency AND preserves input order.
        let results: Vec<Value> = futures::stream::iter(
            tasks
                .into_iter()
                .map(|task| self.run_child(task, context_note.clone(), ctx.clone())),
        )
        .buffered(self.config.max_concurrent)
        .collect()
        .await;

        Ok(json!({"results": results}).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_kernel::ToolDefinition;

    struct NoProvider; // `child_catalog` never calls the model
    #[async_trait]
    impl ChatProvider for NoProvider {
        async fn complete(
            &self,
            _request: &regent_providers::ChatRequest,
        ) -> Result<regent_providers::ChatResponse, regent_providers::ProviderError> {
            unreachable!("child_catalog does not run the agent")
        }
        fn model(&self) -> &str {
            "none"
        }
    }

    struct Noop;
    #[async_trait]
    impl ToolExecutor for Noop {
        async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
            Ok("{}".into())
        }
    }

    fn tool_at(depth: usize, max_depth: usize) -> DelegateTool {
        let mut leaf = ToolCatalog::new();
        let def = ToolDefinition {
            name: "search".into(),
            description: "leaf tool".into(),
            parameters: json!({"type": "object"}),
            toolset: "leaf".into(),
        };
        leaf.register(def, Arc::new(Noop)).unwrap();
        DelegateTool {
            provider: Arc::new(NoProvider),
            store: Arc::new(Store::open_in_memory().unwrap()),
            leaf_catalog: Arc::new(leaf),
            config: DelegationConfig { max_depth, ..DelegationConfig::default() },
            depth,
        }
    }

    fn has(catalog: &ToolCatalog, name: &str) -> bool {
        catalog.definitions().iter().any(|d| d.name == name)
    }
    #[test]
    fn below_cap_children_can_delegate_one_more_level() {
        let child = tool_at(1, 2).child_catalog();
        assert!(has(&child, "search"), "leaf tools are preserved");
        assert!(has(&child, "delegate_task"), "below cap → child may nest");
    }

    #[test]
    fn at_cap_children_get_leaf_only() {
        let grandchild = tool_at(2, 2).child_catalog();
        assert!(has(&grandchild, "search"), "leaf tools still present");
        assert!(!has(&grandchild, "delegate_task"), "at cap → recursion stops");
    }

    #[test]
    fn max_depth_one_reproduces_leaf_only_behavior() {
        assert!(!has(&tool_at(1, 1).child_catalog(), "delegate_task"));
    }
}
