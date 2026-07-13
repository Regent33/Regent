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
#[path = "runner_tests.rs"]
mod tests;
