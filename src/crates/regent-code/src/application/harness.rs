//! The harness loop: assemble code context → (Plan: read-only agent turn → plan
//! text → approve) → (Execute: full agent turn) → verify → on fail: revert.
//! The model decides; this code constrains (read-only plan toolset), gates
//! (approval), executes, and verifies. It wraps `regent_agent::Agent` rather
//! than re-implementing the loop — `max_steps`, interrupts, and compression are
//! inherited.

use crate::domain::{Phase, VerifyOutcome, plan_toolset};
use async_trait::async_trait;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_store::Store;
use regent_tools::{ApprovalDecision, ToolCatalog, ToolContext};
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
            system_prompt: system_prompt.into(),
            config,
            verifier,
            checkpoint,
        }
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
        if decision == ApprovalDecision::Deny {
            return Ok(CodeOutcome {
                approved: false,
                executed: false,
                plan,
                verify: None,
                reverted: false,
                report: "Plan was not approved — nothing was executed.".to_owned(),
            });
        }

        // Snapshot the tree *before* editing so a failed verify can revert.
        let snapshot = self.checkpoint.snapshot().await?;

        let report = self.execute_phase(task, &plan).await?;

        let verify = self.verifier.verify(&self.tool_context.cwd).await?;
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

    /// Phase 2: a fresh agent with the full toolset executes the approved plan.
    async fn execute_phase(&self, task: &str, plan: &str) -> Result<String, RegentError> {
        let mut agent = Agent::new(
            Arc::clone(&self.provider),
            Arc::clone(&self.catalog),
            Arc::clone(&self.store),
            self.tool_context.clone(),
            self.system_prompt.clone(),
            self.config.clone(),
        )?;
        agent.run_turn(&execute_prompt(task, plan)).await
    }
}

/// Plan-phase turn text. Applies Claude Code's plan-mode discipline: a hard
/// read-only constraint that supersedes other instructions, explore-and-reuse,
/// and a structured, concise-but-executable plan.
fn plan_prompt(task: &str) -> String {
    format!(
        "Plan mode is active — this is a READ-ONLY phase. You MUST NOT make any edits or run \
         any mutating tools; only the read-only tools (read_file, glob, search_files, ls) are \
         available to you. This supersedes any other instruction to edit.\n\n\
         Task: {task}\n\n\
         Explore the codebase with the read-only tools to understand what's needed, then write a \
         concise, executable PLAN. Prefer reusing existing functions, utilities, and patterns over \
         adding new code. Structure the plan as:\n\
         - Context — why this change is needed, the problem it addresses\n\
         - Approach — your single recommended approach (not a list of alternatives)\n\
         - Files — the specific files to create or modify\n\
         - Reuse — existing code to build on, with file paths\n\
         - Verification — how to confirm it works (the tests/build to run)\n\n\
         Keep it scannable but detailed enough to execute. Output the plan as your reply."
    )
}

/// Execute-phase turn text. The plan is approved; implement it with the full
/// toolset, fix root causes, reuse code, and don't expand scope.
fn execute_prompt(task: &str, plan: &str) -> String {
    format!(
        "Execute mode — the plan below is APPROVED. Implement it now using your full toolset. \
         Fix the root cause, not the symptom; reuse existing code; match the surrounding style; \
         don't gold-plate or expand scope beyond the plan. When done, reply with a concise report \
         of what you changed.\n\n\
         Task: {task}\n\n\
         Approved plan:\n{plan}"
    )
}
