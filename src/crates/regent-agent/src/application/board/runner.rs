//! The agent-backed [`TaskRunner`]: each task runs in a fresh agent (kanban
//! source), mirroring the cron runner. The board task becomes the prompt.

use super::TaskRunner;
use crate::application::agent::Agent;
use crate::domain::config::AgentConfig;
use async_trait::async_trait;
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_store::{KanbanTaskRow, Store};
use regent_tools::{ToolCatalog, ToolContext};
use std::sync::Arc;

/// Resolves a named agent's stored `model` string to a ready provider (its
/// fallback chain already built). `None` ⇒ fall back to the runner's default
/// provider. The deacon supplies this over its `ProviderRegistry`; the agent
/// crate stays free of provider-config types (ADR-026).
pub type ProviderResolver = Arc<dyn Fn(&str) -> Option<Arc<dyn ChatProvider>> + Send + Sync>;

pub struct AgentTaskRunner {
    provider: Arc<dyn ChatProvider>,
    catalog: Arc<ToolCatalog>,
    store: Arc<Store>,
    tool_context: ToolContext,
    system_prompt: String,
    max_iterations: u32,
    /// Per-agent provider resolver (ADR-026). `None` ⇒ every task runs on the
    /// default `provider` (today's behavior).
    resolver: Option<ProviderResolver>,
}

impl AgentTaskRunner {
    #[must_use]
    pub fn new(
        provider: Arc<dyn ChatProvider>,
        catalog: Arc<ToolCatalog>,
        store: Arc<Store>,
        tool_context: ToolContext,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            catalog,
            store,
            tool_context,
            system_prompt: system_prompt.into(),
            max_iterations: 25,
            resolver: None,
        }
    }

    /// Attach a per-agent provider resolver: a named agent with a `model`
    /// override runs on that model's provider (with config-default fallbacks).
    #[must_use]
    pub fn with_resolver(mut self, resolver: ProviderResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }
}

impl AgentTaskRunner {
    /// Prompt + tool catalog + provider for a task. A task assigned to a *named
    /// agent* (an `agents` row) runs with that agent's system prompt, tool
    /// allow-list, and — if it has a `model` override and a resolver is attached
    /// — that model's provider (ADR-026); otherwise the runner's defaults.
    fn resolve(&self, task: &KanbanTaskRow) -> (String, Arc<ToolCatalog>, Arc<dyn ChatProvider>) {
        let default = || {
            (
                self.system_prompt.clone(),
                Arc::clone(&self.catalog),
                Arc::clone(&self.provider),
            )
        };
        let Some(name) = task.assignee.as_deref() else {
            return default();
        };
        match self.store.find_agent(name) {
            Ok(Some(agent)) => {
                let prompt = if agent.system_prompt.trim().is_empty() {
                    self.system_prompt.clone()
                } else {
                    agent.system_prompt
                };
                let catalog = match agent.tools.as_deref() {
                    Some(csv) if !csv.trim().is_empty() => {
                        let allowed: Vec<String> = csv
                            .split(',')
                            .map(|s| s.trim().to_owned())
                            .filter(|s| !s.is_empty())
                            .collect();
                        let mut sub = (*self.catalog).clone();
                        sub.restrict_to(&allowed);
                        Arc::new(sub)
                    }
                    _ => Arc::clone(&self.catalog),
                };
                // Per-agent provider: the stored `model` resolved through the
                // resolver (provider + config fallbacks). Unresolved/absent ⇒
                // the default provider — never block a task on model config.
                let provider = agent
                    .model
                    .as_deref()
                    .filter(|m| !m.trim().is_empty())
                    .and_then(|m| self.resolver.as_ref().and_then(|r| r(m)))
                    .unwrap_or_else(|| Arc::clone(&self.provider));
                tracing::info!(agent = name, task = task.id, model = ?agent.model, "board task running as named agent");
                (prompt, catalog, provider)
            }
            // Assignee is a plain worker id / unknown name → default worker.
            _ => default(),
        }
    }
}

#[async_trait]
impl TaskRunner for AgentTaskRunner {
    async fn run(&self, task: &KanbanTaskRow) -> Result<String, RegentError> {
        let config = AgentConfig {
            source: "kanban".to_owned(),
            max_iterations: self.max_iterations,
            ..AgentConfig::default()
        };
        let (system_prompt, catalog, provider) = self.resolve(task);
        let mut agent = Agent::new(
            provider,
            catalog,
            Arc::clone(&self.store),
            self.tool_context.clone(),
            system_prompt,
            config,
        )?;
        let prompt = if task.description.is_empty() {
            task.title.clone()
        } else {
            format!("{}\n\n{}", task.title, task.description)
        };
        agent.run_turn(&prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_kernel::ToolDefinition;
    use regent_providers::{ChatRequest, ChatResponse, ProviderError};
    use serde_json::json;

    struct NoProvider;
    #[async_trait]
    impl ChatProvider for NoProvider {
        async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            unreachable!("resolve() never calls the model")
        }
        fn model(&self) -> &str {
            "none"
        }
    }

    /// A provider whose `model()` is a fixed tag — lets a test assert *which*
    /// provider `resolve()` picked without calling the model.
    struct Tagged(&'static str);
    #[async_trait]
    impl ChatProvider for Tagged {
        async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            unreachable!("resolve() never calls the model")
        }
        fn model(&self) -> &str {
            self.0
        }
    }

    struct Noop;
    #[async_trait]
    impl regent_tools::ToolExecutor for Noop {
        async fn execute(
            &self,
            _a: serde_json::Value,
            _c: &ToolContext,
        ) -> Result<String, RegentError> {
            Ok("{}".into())
        }
    }

    fn def(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.into(),
            description: "t".into(),
            parameters: json!({"type": "object"}),
            toolset: "core".into(),
        }
    }

    fn task(assignee: Option<&str>) -> KanbanTaskRow {
        KanbanTaskRow {
            id: "t1".into(),
            board: "default".into(),
            title: "do it".into(),
            description: String::new(),
            status: "in_progress".into(),
            assignee: assignee.map(ToOwned::to_owned),
            created_at: 0.0,
            updated_at: 0.0,
        }
    }

    fn runner(store: Arc<Store>) -> AgentTaskRunner {
        let mut catalog = ToolCatalog::new();
        catalog.register(def("search"), Arc::new(Noop)).unwrap();
        catalog.register(def("write_file"), Arc::new(Noop)).unwrap();
        AgentTaskRunner::new(
            Arc::new(NoProvider),
            Arc::new(catalog),
            store,
            ToolContext::new(std::env::temp_dir(), Arc::new(regent_tools::DenyAll)),
            "default worker prompt",
        )
    }

    fn names(c: &ToolCatalog) -> Vec<String> {
        c.definitions().into_iter().map(|d| d.name).collect()
    }

    #[test]
    fn unassigned_task_uses_default_prompt_and_full_catalog() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let r = runner(Arc::clone(&store));
        let (prompt, cat, _provider) = r.resolve(&task(None));
        assert_eq!(prompt, "default worker prompt");
        assert_eq!(names(&cat).len(), 2);
    }

    #[test]
    fn named_agent_overrides_prompt_and_restricts_tools() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        store
            .upsert_agent("researcher", "web", "You research.", None, Some("search"))
            .unwrap();
        let r = runner(Arc::clone(&store));
        let (prompt, cat, _provider) = r.resolve(&task(Some("researcher")));
        assert_eq!(prompt, "You research.");
        assert_eq!(
            names(&cat),
            vec!["search".to_owned()],
            "tool allow-list applied"
        );
    }

    #[test]
    fn named_agent_with_model_runs_on_the_resolved_provider() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        store
            .upsert_agent("fast", "quick", "Be quick.", Some("groq/llama"), None)
            .unwrap();
        // Resolver only knows the configured model; anything else ⇒ default.
        let resolver: ProviderResolver = Arc::new(|m: &str| {
            (m == "groq/llama").then(|| Arc::new(Tagged("groq-llama")) as Arc<dyn ChatProvider>)
        });
        let r = runner(Arc::clone(&store)).with_resolver(resolver);
        let (_prompt, _cat, provider) = r.resolve(&task(Some("fast")));
        assert_eq!(
            provider.model(),
            "groq-llama",
            "ran on its resolved provider"
        );
    }

    #[test]
    fn named_agent_model_without_resolver_falls_back_to_default_provider() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        store
            .upsert_agent("fast", "quick", "Be quick.", Some("groq/llama"), None)
            .unwrap();
        let r = runner(Arc::clone(&store)); // no resolver attached
        let (_prompt, _cat, provider) = r.resolve(&task(Some("fast")));
        assert_eq!(
            provider.model(),
            "none",
            "default provider used (NoProvider)"
        );
    }

    #[test]
    fn unknown_assignee_falls_back_to_default() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let r = runner(Arc::clone(&store));
        let (prompt, cat, _provider) = r.resolve(&task(Some("regent")));
        assert_eq!(prompt, "default worker prompt");
        assert_eq!(names(&cat).len(), 2);
    }
}
