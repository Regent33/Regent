//! The harness loop: assemble code context → (Plan: read-only agent turn → plan
//! text → approve) → (Execute: full agent turn) → verify → on fail: revert.
//! The model decides; this code constrains (read-only plan toolset), gates
//! (approval), executes, and verifies. It wraps `regent_agent::Agent` rather
//! than re-implementing the loop — `max_steps`, interrupts, and compression are
//! inherited.

use crate::domain::{Phase, VerifyOutcome, plan_toolset};
use async_trait::async_trait;
use regent_agent::{Agent, AgentConfig, CODING_PROMPT};
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_store::Store;
use regent_tools::{ToolCatalog, ToolContext};
use std::path::Path;
use std::sync::Arc;

/// Runs the repo's detected verify lane after an edit batch. Split from the
/// harness so the loop is testable without spawning real build commands.
#[async_trait]
pub trait Verifier: Send + Sync {
    /// `Ok(None)` = no verify lane detected (verify skipped); `Ok(Some(_))` =
    /// the lane ran, with its pass/fail outcome.
    async fn verify(&self, workspace: &Path) -> Result<Option<VerifyOutcome>, RegentError>;
}

/// Snapshots the working tree before execution so a failed verify can revert to
/// the last green state (compensation/saga — never stack on a broken state).
#[async_trait]
pub trait Checkpoint: Send + Sync {
    /// `Ok(None)` = cannot snapshot here (e.g. not a git repo) → revert will
    /// degrade to report-only, surfaced rather than silently skipped.
    async fn snapshot(&self) -> Result<Option<String>, RegentError>;
    /// Restores the working tree to snapshot `id`.
    async fn restore(&self, id: &str) -> Result<(), RegentError>;
}

/// The result of one harness run.
#[derive(Debug, Clone)]
pub struct CodeOutcome {
    /// Whether the plan passed the approval gate (false → nothing was executed).
    pub approved: bool,
    /// Whether the execute phase ran.
    pub executed: bool,
    /// The plan produced by the read-only phase.
    pub plan: String,
    /// The verify result, or `None` when no lane was detected / not executed.
    pub verify: Option<VerifyOutcome>,
    /// Fix turns run after a red verify (gap H4) — 0 on a first-try green.
    pub fix_attempts: u32,
    /// Whether the working tree was reverted (verify failed and a snapshot
    /// existed). False with a failed verify means revert degraded to report-only.
    pub reverted: bool,
    /// The execute phase's final report, or a short status when it didn't run.
    pub report: String,
}

/// A coding-specialized harness over an `Agent`. Holds the construction inputs
/// for the two phase-agents (plan = read-only catalog, execute = full catalog)
/// plus the verify + checkpoint ports.
pub struct CodeHarness {
    provider: Arc<dyn ChatProvider>,
    /// The full toolset the execute phase gets; the plan phase is a read-only
    /// subset of it.
    catalog: Arc<ToolCatalog>,
    store: Arc<Store>,
    /// Reused for both phases — carries the cwd (workspace) and the approval
    /// handler the plan gate reuses.
    tool_context: ToolContext,
    system_prompt: String,
    config: AgentConfig,
    verifier: Arc<dyn Verifier>,
    checkpoint: Arc<dyn Checkpoint>,
    /// Gap H4: bounded fix turns after a red verify before the revert backstop.
    max_fix_attempts: u32,
}

impl CodeHarness {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<dyn ChatProvider>,
        catalog: Arc<ToolCatalog>,
        store: Arc<Store>,
        tool_context: ToolContext,
        system_prompt: impl Into<String>,
        config: AgentConfig,
        verifier: Arc<dyn Verifier>,
        checkpoint: Arc<dyn Checkpoint>,
    ) -> Self {
        Self {
            provider,
            catalog,
            store,
            tool_context,
            // The coding overlay leads: engineering discipline extends (and
            // where they conflict, wins over) the surface's persona prompt.
            system_prompt: format!("{CODING_PROMPT}\n\n{}", system_prompt.into()),
            config,
            verifier,
            checkpoint,
            max_fix_attempts: 2,
        }
    }

    /// Overrides the fix-retry bound (default 2). 0 restores the old
    /// one-shot revert-on-red behavior.
    #[must_use]
    pub fn with_max_fix_attempts(mut self, attempts: u32) -> Self {
        self.max_fix_attempts = attempts;
        self
    }

    /// Runs the full harness: plan (read-only) → approve → execute → verify →
    /// revert-on-fail.
    pub async fn run(&self, task: &str) -> Result<CodeOutcome, RegentError> {
        let plan = self.plan_phase(task).await?;

        // Approval gate — reuse the surface's ApprovalHandler. Non-approval
        // aborts before any edit (a core invariant: never proceed by default).
        let decision = self
            .tool_context
            .approval
            .request("code", &plan, "execute this coding plan")
            .await;
        if decision.denied() {
            let report = match decision.feedback() {
                Some(feedback) => format!("Plan was not approved: {feedback}"),
                None => "Plan was not approved — nothing was executed.".to_owned(),
            };
            return Ok(CodeOutcome {
                approved: false,
                executed: false,
                plan,
                verify: None,
                fix_attempts: 0,
                reverted: false,
                report,
            });
        }

        // Snapshot the tree *before* editing so a failed verify can revert.
        let snapshot = self.checkpoint.snapshot().await?;

        // Gap H4: the execute agent stays alive across fix attempts — its
        // context holds what it just did; a fresh agent would re-read the
        // world. The pre-execute snapshot still guards the whole sequence.
        let mut agent = self.execute_agent()?;
        let mut report = agent.run_turn(&execute_prompt(task, &plan)).await?;
        let mut verify = self.verifier.verify(&self.tool_context.cwd).await?;
        let mut fix_attempts = 0;
        while let Some(outcome) = &verify {
            if outcome.passed || fix_attempts >= self.max_fix_attempts {
                break;
            }
            fix_attempts += 1;
            tracing::info!(fix_attempts, "verify red — running a fix turn");
            report = agent.run_turn(&fix_prompt(&outcome.summary)).await?;
            verify = self.verifier.verify(&self.tool_context.cwd).await?;
        }

        let reverted = match &verify {
            Some(outcome) if !outcome.passed => match &snapshot {
                Some(id) => {
                    self.checkpoint.restore(id).await?;
                    true
                }
                // Degrade: no snapshot (e.g. not a git repo) → report only,
                // never silently leave a broken tree pretending it was undone.
                None => false,
            },
            _ => false,
        };

        Ok(CodeOutcome {
            approved: true,
            executed: true,
            plan,
            verify,
            fix_attempts,
            reverted,
            report,
        })
    }

    /// Phase 1: a fresh agent with the read-only subset of the catalog produces
    /// a plan. The write/terminal tools are physically absent from this agent's
    /// catalog, so plan-mode cannot edit even if the model tries.
    async fn plan_phase(&self, task: &str) -> Result<String, RegentError> {
        let full_names: Vec<String> = self
            .catalog
            .definitions()
            .into_iter()
            .map(|d| d.name)
            .collect();
        let mut plan_catalog = (*self.catalog).clone();
        plan_catalog.restrict_to(&plan_toolset(Phase::Plan, &full_names));

        let mut agent = Agent::new(
            Arc::clone(&self.provider),
            Arc::new(plan_catalog),
            Arc::clone(&self.store),
            self.tool_context.clone(),
            self.system_prompt.clone(),
            self.config.clone(),
        )?;
        agent.run_turn(&plan_prompt(task)).await
    }

    /// Phase 2's agent: full toolset, editing tools wrapped with edit-time
    /// diagnostics (gap H5) so breakage shows up in the same tool result as
    /// the edit that caused it. Returned (not run) so `run` can keep it alive
    /// across verify-fix attempts.
    fn execute_agent(&self) -> Result<Agent, RegentError> {
        let mut exec_catalog = (*self.catalog).clone();
        crate::infra::wrap_diagnostics(&mut exec_catalog, &self.tool_context.cwd);
        Agent::new(
            Arc::clone(&self.provider),
            Arc::new(exec_catalog),
            Arc::clone(&self.store),
            self.tool_context.clone(),
            self.system_prompt.clone(),
            self.config.clone(),
        )
    }
}

pub use prompts::{execute_prompt, fix_prompt, plan_prompt};

mod prompts;
