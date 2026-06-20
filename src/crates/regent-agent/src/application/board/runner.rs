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

pub struct AgentTaskRunner {
    provider: Arc<dyn ChatProvider>,
    catalog: Arc<ToolCatalog>,
    store: Arc<Store>,
    tool_context: ToolContext,
    system_prompt: String,
    max_iterations: u32,
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
        let mut agent = Agent::new(
            Arc::clone(&self.provider),
            Arc::clone(&self.catalog),
            Arc::clone(&self.store),
            self.tool_context.clone(),
            self.system_prompt.clone(),
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
