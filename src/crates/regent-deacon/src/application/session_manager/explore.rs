//! The `explore` scout run (gap T3): a fresh, cheap agent over the read-only
//! plan-mode tool subset answers a reconnaissance question and returns only
//! its conclusions — the parent's context grows by one tool result instead of
//! a dozen raw file reads. The child session persists with source `explore`
//! (inspectable via `sessions list`).

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_agent::{Agent, AgentConfig, EXPLORE_PROMPT};
use regent_tools::ToolContext;
use std::sync::{Arc, OnceLock};

impl SessionManager {
    /// One scout turn on a fresh read-only agent. The catalog is physically
    /// restricted to the plan-mode subset (and `explore` itself is not in that
    /// allowlist, so a scout can never recurse into another scout).
    pub async fn run_explore(
        &self,
        question: &str,
        context: Option<&str>,
    ) -> Result<String, DeaconError> {
        let provider = self.provider();
        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let mut catalog = self.build_main_catalog(&provider, &sid_cell, None).await?;
        let names: Vec<String> = catalog.definitions().into_iter().map(|d| d.name).collect();
        catalog.restrict_to(&regent_code::plan_toolset(regent_code::Phase::Plan, &names));

        let config = AgentConfig {
            max_iterations: 15,
            max_turn_tokens: Some(60_000),
            source: "explore".to_owned(),
            ..self.agent_template.clone()
        };
        // Read-only toolset → nothing to approve; deny-by-default is safe.
        let ctx = ToolContext::new(self.cwd.clone(), Arc::new(regent_tools::DenyAll));
        let mut agent = Agent::new(
            provider,
            Arc::new(catalog),
            Arc::clone(&self.store),
            ctx,
            EXPLORE_PROMPT,
            config,
        )
        .map_err(DeaconError::Core)?;

        let prompt = match context {
            Some(extra) => format!("{question}\n\nContext from the caller:\n{extra}"),
            None => question.to_owned(),
        };
        agent.run_turn(&prompt).await.map_err(DeaconError::Core)
    }
}
