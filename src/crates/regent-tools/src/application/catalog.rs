use crate::domain::contracts::{
    DispatchHook, PermissionAction, ToolExecutor, evaluate_permissions, subject_of,
};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

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
    /// Token-efficiency: tools whose full schema is NOT sent per request.
    /// They stay executable; `load_tools` (or a direct call) activates them,
    /// after which their definition appears in `definitions()`.
    deferred: BTreeSet<String>,
    /// Runtime-activated deferred tools — shared (`Arc`) so the `load_tools`
    /// executor and every clone of this catalog see the same set.
    activated: Arc<RwLock<BTreeSet<String>>>,
    /// Spill-file sequence for truncated results — shared across clones so
    /// names never collide within a process.
    spill_seq: Arc<AtomicU64>,
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

    /// Model-facing schema list, deterministic order. Deferred tools appear
    /// only once activated (their names still surface via `load_tools`).
    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        let activated = self.activated.read().expect("catalog lock poisoned");
        self.tools
            .values()
            .filter(|t| {
                !self.deferred.contains(&t.definition.name)
                    || activated.contains(&t.definition.name)
            })
            .map(|t| t.definition.clone())
            .collect()
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
        // A direct call to a deferred tool activates it (forgiving path — the
        // model knew the name from the load_tools index and guessed the args).
        if self.deferred.contains(name) {
            self.activated
                .write()
                .expect("catalog lock poisoned")
                .insert(name.to_owned());
        }
        let args: Value = match serde_json::from_str(arguments_json) {
            Ok(value) => value,
            Err(error) => {
                return tool_error_json(format!("invalid tool arguments (not JSON): {error}"));
            }
        };
        // Gap S5/S6: permission rules, data not code — last match wins. Deny
        // returns its feedback as the tool result (the model steers instead
        // of stalling); Ask routes through the surface's approval handler.
        // No matching rule = today's behavior exactly.
        if let Some(rule) = evaluate_permissions(&ctx.permission_rules, name, &subject_of(&args)) {
            match rule.action {
                PermissionAction::Allow => {}
                PermissionAction::Deny => {
                    return tool_error_json(rule.feedback.clone().unwrap_or_else(|| {
                        format!("'{name}' is denied here by a permission rule")
                    }));
                }
                PermissionAction::Ask => {
                    let decision = ctx
                        .approval
                        .request(
                            name,
                            &subject_of(&args),
                            "a permission rule requires approval",
                        )
                        .await;
                    if decision.denied() {
                        let message = decision
                            .feedback()
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("'{name}' was not approved"));
                        return tool_error_json(message);
                    }
                }
            }
        }
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
        // Gap T6: an oversized result never enters history raw — the model
        // gets the head plus a receipt; hooks see what the model sees.
        let result = super::truncation::truncate_oversized(&self.spill_seq, name, result, ctx);
        for hook in &self.hooks {
            hook.after_dispatch(name, &result);
        }
        result
    }
}

mod tiering;

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
