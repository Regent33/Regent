//! Coding-harness session flows: `code.plan` (read-only → PLAN) and `code.start`
//! (snapshot → execute the approved plan → verify → revert-on-fail). Split from
//! `mod.rs` to keep that file on generic session lifecycle. Reuses the existing
//! session turn path, so the execute turn streams + approves like any other.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_code::{Checkpoint, Verifier};
use regent_kernel::SessionId;

/// Result of `code.start`: the execute phase's report, the verify outcome (when
/// a lane was detected), how many fix turns ran after a red verify, and whether
/// the working tree was reverted.
pub struct CodeStartResult {
    pub session_id: SessionId,
    pub report: String,
    pub verify: Option<regent_code::VerifyOutcome>,
    pub fix_attempts: u32,
    pub reverted: bool,
}

/// Gap H4: bounded fix turns after a red verify before the revert backstop.
const MAX_FIX_ATTEMPTS: u32 = 2;

impl SessionManager {
    /// `code.plan` — a read-only session researches the task and returns a PLAN.
    /// Plan-mode strips the catalog to read-only tools, so the turn cannot edit.
    /// `skill` names a library skill (bundled or on disk) whose body is appended
    /// to the session's system prompt at build; unknown names error out.
    pub async fn code_plan(
        &self,
        task: &str,
        skill: Option<&str>,
    ) -> Result<(SessionId, String), DeaconError> {
        let session_id = self
            .create_session_keyed(None, super::lifecycle::SessionKind::CodePlan, skill)
            .await?;
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
    /// `review_skills` (Wave 3e) names review skills (`code-reviewer`,
    /// `secure-code-guardian`, or any library skill) each run as a read-only
    /// phase over the resulting diff; findings append to the report.
    pub async fn code_start(
        &self,
        task: &str,
        plan: &str,
        skill: Option<&str>,
        review_skills: &[String],
    ) -> Result<CodeStartResult, DeaconError> {
        let checkpoint = regent_code::GitCheckpoint::new(self.cwd.clone());
        let snapshot = checkpoint.snapshot().await.map_err(DeaconError::Core)?;

        let session_id = self
            .create_session_keyed(None, super::lifecycle::SessionKind::CodeExecute, skill)
            .await?;
        // Announce the run session BEFORE the (minutes-long) execute turn: the
        // client planned in a different, read-only session, and without this id
        // its Stop / approval / streaming bindings all target the wrong one.
        let started = crate::domain::entities::RpcNotification::new(
            "code.started",
            serde_json::json!({"session_id": session_id.to_string()}),
        );
        if let Ok(line) = serde_json::to_string(&started) {
            self.out_tx.send(line).ok();
        }
        let mut report = self
            .run_turn(&session_id, &regent_code::execute_prompt(task, plan))
            .await?;

        // Gap H4: a red verify feeds its failure output back into the SAME
        // session for a bounded fix turn (the session's context holds what it
        // just did); the pre-execute snapshot still backstops the whole run.
        let mut verify = regent_code::VerifyRunner
            .verify(&self.cwd)
            .await
            .map_err(DeaconError::Core)?;
        let mut fix_attempts = 0;
        while let Some(outcome) = &verify {
            if outcome.passed || fix_attempts >= MAX_FIX_ATTEMPTS {
                break;
            }
            fix_attempts += 1;
            tracing::info!(fix_attempts, "verify red — running a fix turn");
            report = self
                .run_turn(&session_id, &regent_code::fix_prompt(&outcome.summary))
                .await?;
            verify = regent_code::VerifyRunner
                .verify(&self.cwd)
                .await
                .map_err(DeaconError::Core)?;
        }

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

        // Wave 3e: optional review phases — a read-only session wearing the
        // named review skill judges the surviving diff; findings append to
        // the report. Nothing to review after a revert.
        if !review_skills.is_empty() && !reverted {
            match diff_of(&self.cwd).await {
                Some(diff) if !diff.trim().is_empty() => {
                    for name in review_skills {
                        let findings = self.run_review_phase(name, task, &diff).await?;
                        report.push_str(&format!("\n\n## Review — {name}\n{findings}"));
                    }
                }
                _ => report.push_str("\n\n(review skipped — no diff to review)"),
            }
        }

        Ok(CodeStartResult {
            session_id,
            report,
            verify,
            fix_attempts,
            reverted,
        })
    }

    /// One read-only review turn: a `CodePlan`-kind session (write tools
    /// physically absent) wearing `skill` reviews the diff.
    async fn run_review_phase(
        &self,
        skill: &str,
        task: &str,
        diff: &str,
    ) -> Result<String, DeaconError> {
        let session_id = self
            .create_session_keyed(None, super::lifecycle::SessionKind::CodePlan, Some(skill))
            .await?;
        self.run_turn(
            &session_id,
            &format!(
                "Review the changes just made for the task below, following your active \
                 skill's instructions. Judge the DIFF (you may read surrounding files for \
                 context). Report your findings as your reply.\n\nTask: {task}\n\nDiff:\n{diff}"
            ),
        )
        .await
    }
}

/// The working tree's diff against HEAD (staged + unstaged), capped so a huge
/// change can't blow up the review prompt. `None` when git is unavailable.
// ponytail: untracked files don't show in `git diff HEAD` — reviews cover
// edits; add `--intent-to-add` plumbing if new-file review ever matters.
async fn diff_of(cwd: &std::path::Path) -> Option<String> {
    const DIFF_CAP_CHARS: usize = 60_000;
    let output = tokio::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(cwd)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut diff = String::from_utf8_lossy(&output.stdout).into_owned();
    if diff.chars().count() > DIFF_CAP_CHARS {
        diff = diff.chars().take(DIFF_CAP_CHARS).collect();
        diff.push_str("\n[diff truncated for review]");
    }
    Some(diff)
}
