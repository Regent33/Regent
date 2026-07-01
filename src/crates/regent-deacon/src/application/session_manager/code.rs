//! Coding-harness session flows: `code.plan` (read-only → PLAN) and `code.start`
//! (snapshot → execute the approved plan → verify → revert-on-fail). Split from
//! `mod.rs` to keep that file on generic session lifecycle. Reuses the existing
//! session turn path, so the execute turn streams + approves like any other.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_code::{Checkpoint, Verifier};
use regent_kernel::SessionId;

/// Result of `code.start`: the execute phase's report, the verify outcome (when
/// a lane was detected), and whether the working tree was reverted.
pub struct CodeStartResult {
    pub session_id: SessionId,
    pub report: String,
    pub verify: Option<regent_code::VerifyOutcome>,
    pub reverted: bool,
}

impl SessionManager {
    /// `code.plan` — a read-only session researches the task and returns a PLAN.
    /// Plan-mode strips the catalog to read-only tools, so the turn cannot edit.
    pub async fn code_plan(&self, task: &str) -> Result<(SessionId, String), DeaconError> {
        let session_id = self.create_session_keyed(None, true).await?;
        let plan = self
            .run_turn(&session_id, &regent_code::plan_prompt(task))
            .await?;
        Ok((session_id, plan))
    }

    /// `code.start` — snapshot the tree, run the approved plan with the full
    /// toolset (the existing approval/streaming/interrupt path applies), verify
    /// with the repo's detected lane, and revert to the snapshot on failure.
    /// Outside a git repo the snapshot is `None` and revert degrades to
    /// report-only (the failure is still surfaced, never silently kept).
    pub async fn code_start(&self, task: &str, plan: &str) -> Result<CodeStartResult, DeaconError> {
        let checkpoint = regent_code::GitCheckpoint::new(self.cwd.clone());
        let snapshot = checkpoint.snapshot().await.map_err(DeaconError::Core)?;

        let session_id = self.create_session_keyed(None, false).await?;
        let report = self
            .run_turn(&session_id, &regent_code::execute_prompt(task, plan))
            .await?;

        let verify = regent_code::VerifyRunner
            .verify(&self.cwd)
            .await
            .map_err(DeaconError::Core)?;
        let reverted = match &verify {
            Some(outcome) if !outcome.passed => match &snapshot {
                Some(id) => {
                    checkpoint.restore(id).await.map_err(DeaconError::Core)?;
                    true
                }
                None => false,
            },
            _ => false,
        };
        Ok(CodeStartResult {
            session_id,
            report,
            verify,
            reverted,
        })
    }
}
