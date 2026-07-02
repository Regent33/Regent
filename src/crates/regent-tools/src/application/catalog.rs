use crate::domain::contracts::{DispatchHook, ToolExecutor};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
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
            .filter(|t| !self.deferred.contains(&t.definition.name) || activated.contains(&t.definition.name))
            .map(|t| t.definition.clone())
            .collect()
    }

    /// Marks registered tools as deferred (schemas withheld until loaded) and
    /// registers the `load_tools` loader, whose description carries a one-line
    /// index of what's loadable. Unknown names are ignored. No-op for an
    /// empty effective list — no loader, zero prompt cost.
    pub fn defer(&mut self, names: &[String]) -> Result<usize, RegentError> {
        let known: Vec<String> = names
            .iter()
            .filter(|n| self.tools.contains_key(*n) && *n != "load_tools")
            .cloned()
            .collect();
        if known.is_empty() {
            return Ok(0);
        }
        self.deferred.extend(known.iter().cloned());
        let index: String = known
            .iter()
            .map(|n| {
                let desc = &self.tools[n].definition.description;
                let hook: String = desc.chars().take(80).collect();
                format!("{n} ({hook}…)")
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let deferred_defs: Vec<ToolDefinition> = known
            .iter()
            .map(|n| self.tools[n].definition.clone())
            .collect();
        self.register(
            ToolDefinition {
                name: "load_tools".into(),
                description: format!(
                    "Load the full schema of deferred tools, making them callable. More tools \
                     exist than are listed — load one when its purpose matches the task: {index}"
                ),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "names": {"type": "array", "items": {"type": "string"},
                                  "description": "Deferred tool names to load."}
                    },
                    "required": ["names"]
                }),
                toolset: "core".into(),
            },
            Arc::new(LoadToolsTool {
                deferred: deferred_defs,
                activated: Arc::clone(&self.activated),
            }),
        )?;
        Ok(self.deferred.len())
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
        self.tools
            .retain(|name, _| allowed.iter().any(|a| a == name));
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

/// Executor for `load_tools`: returns the requested deferred definitions
/// (the model reads the schemas from the result) and activates them so they
/// also appear in the next request's tool list.
struct LoadToolsTool {
    deferred: Vec<ToolDefinition>,
    activated: Arc<RwLock<BTreeSet<String>>>,
}

#[async_trait]
impl ToolExecutor for LoadToolsTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let requested: Vec<String> = args
            .get("names")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if requested.is_empty() {
            return Ok(tool_error_json("load_tools needs 'names' (a non-empty array)"));
        }
        let mut loaded = Vec::new();
        let mut unknown = Vec::new();
        for name in &requested {
            match self.deferred.iter().find(|d| &d.name == name) {
                Some(def) => {
                    self.activated
                        .write()
                        .expect("catalog lock poisoned")
                        .insert(name.clone());
                    loaded.push(json!({
                        "name": def.name,
                        "description": def.description,
                        "parameters": def.parameters,
                    }));
                }
                None => unknown.push(name.clone()),
            }
        }
        Ok(json!({
            "loaded": loaded,
            "unknown": unknown,
            "note": "loaded tools are callable now and listed from the next turn on",
        })
        .to_string())
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

    struct Echo;

    #[async_trait]
    impl ToolExecutor for Echo {
        async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
            Ok("\"ok\"".into())
        }
    }

    /// Deferred tools: schema withheld until loaded, still executable, and
    /// `load_tools` returns the schema + activates for the next turn.
    #[tokio::test]
    async fn deferred_tools_hide_until_loaded_but_stay_executable() {
        let mut catalog = ToolCatalog::new();
        catalog.register(definition("rare_tool"), Arc::new(Echo)).unwrap();
        catalog.register(definition("core_tool"), Arc::new(Echo)).unwrap();
        catalog.defer(&["rare_tool".into(), "no_such".into()]).unwrap();

        let names: Vec<_> = catalog.definitions().into_iter().map(|d| d.name).collect();
        assert!(names.contains(&"core_tool".to_owned()));
        assert!(names.contains(&"load_tools".to_owned()));
        assert!(!names.contains(&"rare_tool".to_owned()), "deferred schema withheld");

        // load_tools returns the schema and activates it.
        let out = catalog
            .dispatch("load_tools", r#"{"names":["rare_tool","nope"]}"#, &ctx())
            .await;
        assert!(out.contains("rare_tool") && out.contains("nope"));
        let names: Vec<_> = catalog.definitions().into_iter().map(|d| d.name).collect();
        assert!(names.contains(&"rare_tool".to_owned()), "activated after load");

        // Direct calls to a deferred tool always execute (forgiving path).
        let mut catalog2 = ToolCatalog::new();
        catalog2.register(definition("rare_tool"), Arc::new(Echo)).unwrap();
        catalog2.defer(&["rare_tool".into()]).unwrap();
        assert_eq!(catalog2.dispatch("rare_tool", "{}", &ctx()).await, "\"ok\"");
        let names: Vec<_> = catalog2.definitions().into_iter().map(|d| d.name).collect();
        assert!(names.contains(&"rare_tool".to_owned()), "direct call activates");
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
