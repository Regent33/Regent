use crate::domain::contracts::{DispatchHook, ToolExecutor};
use crate::domain::entities::ToolContext;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
struct RegisteredTool {
    definition: ToolDefinition,
    executor: Arc<dyn ToolExecutor>,
}

/// Explicit tool manifest: holds the definition (model-facing) and the
/// executor (machine-facing) per tool. BTreeMap keeps definition order
/// deterministic — the schema list must be byte-stable across turns for
/// prompt caching.
///
/// `Clone` is cheap (executors and hooks are `Arc`) and lets a caller derive a
/// variant catalog — e.g. delegation builds a child catalog = this one plus a
/// depth-bounded `delegate_task`.
#[derive(Default, Clone)]
pub struct ToolCatalog {
    tools: BTreeMap<String, RegisteredTool>,
    hooks: Vec<Arc<dyn DispatchHook>>,
}

impl ToolCatalog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a tool. Shadowing an existing name is rejected (the
    /// rule: accidental overwrites of built-ins are bugs, not features).
    pub fn register(
        &mut self,
        definition: ToolDefinition,
        executor: Arc<dyn ToolExecutor>,
    ) -> Result<(), RegentError> {
        if self.tools.contains_key(&definition.name) {
            return Err(RegentError::Config(format!(
                "tool '{}' is already registered",
                definition.name
            )));
        }
        self.tools.insert(
            definition.name.clone(),
            RegisteredTool {
                definition,
                executor,
            },
        );
        Ok(())
    }

    /// Observer hook around every executed dispatch (tracer/audit — the
    /// in-process plugin seam).
    pub fn add_hook(&mut self, hook: Arc<dyn DispatchHook>) {
        self.hooks.push(hook);
    }

    /// Model-facing schema list, deterministic order.
    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition.clone()).collect()
    }

    /// Removes tools by name (per-surface disable). Returns how many were
    /// removed; unknown names are ignored.
    pub fn disable(&mut self, names: &[String]) -> usize {
        let before = self.tools.len();
        self.tools
            .retain(|name, _| !names.iter().any(|n| n == name));
        before - self.tools.len()
    }

    /// Keeps only tools whose name is in `allowed` (a named agent's tool
    /// allow-list). Unknown names in `allowed` are ignored; an empty list keeps
    /// nothing. Returns how many were removed.
    pub fn restrict_to(&mut self, allowed: &[String]) -> usize {
        let before = self.tools.len();
        self.tools.retain(|name, _| allowed.iter().any(|a| a == name));
        before - self.tools.len()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Dispatches one tool call. Always returns a JSON string — unknown
    /// tools, argument parse failures, and executor errors all come back as
    /// `{"error": ...}` so the model sees well-formed JSON every time.
    pub async fn dispatch(&self, name: &str, arguments_json: &str, ctx: &ToolContext) -> String {
        let Some(entry) = self.tools.get(name) else {
            return tool_error_json(format!("unknown tool: {name}"));
        };
        let args: Value = match serde_json::from_str(arguments_json) {
            Ok(value) => value,
            Err(error) => {
                return tool_error_json(format!("invalid tool arguments (not JSON): {error}"));
            }
        };
        for hook in &self.hooks {
            hook.before_dispatch(name, &args);
        }
        let result = match entry.executor.execute(args, ctx).await {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(tool = name, %error, "tool execution failed");
                tool_error_json(format!("tool execution failed: {error}"))
            }
        };
        for hook in &self.hooks {
            hook.after_dispatch(name, &result);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;
    use async_trait::async_trait;
    use serde_json::json;

    struct Boom;

    #[async_trait]
    impl ToolExecutor for Boom {
        async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
            Err(RegentError::Tool {
                tool: "boom".into(),
                message: "kapow".into(),
            })
        }
    }

    fn definition(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.into(),
            description: "test".into(),
            parameters: json!({"type": "object"}),
            toolset: "test".into(),
        }
    }

    fn ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
    }

    #[tokio::test]
    async fn unknown_tool_and_bad_args_return_error_json() {
        let catalog = ToolCatalog::new();
        let out = catalog.dispatch("nope", "{}", &ctx()).await;
        assert!(out.contains("unknown tool"));
        let mut catalog = ToolCatalog::new();
        catalog
            .register(definition("boom"), Arc::new(Boom))
            .unwrap();
        let out = catalog.dispatch("boom", "not json", &ctx()).await;
        assert!(out.contains("invalid tool arguments"));
    }

    #[tokio::test]
    async fn executor_errors_are_wrapped_not_thrown() {
        let mut catalog = ToolCatalog::new();
        catalog
            .register(definition("boom"), Arc::new(Boom))
            .unwrap();
        let out = catalog.dispatch("boom", "{}", &ctx()).await;
        let value: Value = serde_json::from_str(&out).unwrap();
        assert!(value["error"].as_str().unwrap().contains("kapow"));
    }

    #[test]
    fn duplicate_registration_rejected_and_order_deterministic() {
        let mut catalog = ToolCatalog::new();
        catalog
            .register(definition("zeta"), Arc::new(Boom))
            .unwrap();
        catalog
            .register(definition("alpha"), Arc::new(Boom))
            .unwrap();
        assert!(
            catalog
                .register(definition("alpha"), Arc::new(Boom))
                .is_err()
        );
        let names: Vec<_> = catalog.definitions().into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
    }
}
