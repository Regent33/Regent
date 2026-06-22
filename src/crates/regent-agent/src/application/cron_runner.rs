//! Cron → agent adapter: each due job runs in a **fresh** agent with cron
//! source, no graph memory, and no background review (the
//! `skip_memory` rule — scheduled runs must not mutate long-term state by
//! default). The scheduler owns the hard timeout around this runner.

use crate::application::agent::Agent;
use crate::domain::config::AgentConfig;
use async_trait::async_trait;
use regent_cron::{CronError, CronJob, JobRunner};
use regent_providers::ChatProvider;
use regent_store::Store;
use regent_tools::{ToolCatalog, ToolContext};
use std::sync::Arc;

pub struct AgentJobRunner {
    provider: Arc<dyn ChatProvider>,
    catalog: Arc<ToolCatalog>,
    store: Arc<Store>,
    tool_context: ToolContext,
    system_prompt: String,
    max_iterations: u32,
}

impl AgentJobRunner {
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
impl JobRunner for AgentJobRunner {
    async fn run(&self, job: &CronJob) -> Result<String, CronError> {
        let config = AgentConfig {
            source: "cron".to_owned(),
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
        Ok(agent.run_turn(&job.prompt).await?)
    }
}
