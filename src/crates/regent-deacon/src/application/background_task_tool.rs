//! The `background_task` tool — fire-and-acknowledge for long jobs (building
//! software, deep research, producing documents, spreadsheets, decks). It spawns
//! a detached full-toolset agent session and returns immediately, so a live
//! voice/chat turn never blocks on the work.
//!
//! State lives in the durable job ledger, NOT in this process. Before W1 the
//! board was a `static Mutex<Vec<Task>>`: a deacon restart dropped every running
//! job without a word, after the user had been told "I'll report back". Results
//! come back through `wrap_prompt`, which the dispatcher calls on every real
//! `prompt.submit`.

use super::background_task_run::{run_to_completion, take_slot};
use crate::application::session_manager::SessionManager;
use async_trait::async_trait;
use regent_jobs::{JobLedger, JobLimits};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_tools::{ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::{Arc, Weak};

/// A background job that has produced nothing for this long is stopped and
/// recorded as `TimedOut` — enforced by the deadline arm in
/// `run_to_completion`, not merely stored on the row.
pub(super) const DEFAULT_TIMEOUT_SECS: u64 = 45 * 60;

/// How many detached jobs may run at once, process-wide (W9's concurrency cap).
///
/// Each one is a **full-toolset agent session** making its own model calls, and
/// nothing else bounded them: the idempotency key stops the same job twice, not
/// six different ones, so a model emitting a batch of parallel `background_task`
/// calls fanned out unboundedly against the same provider quota. W2 exists
/// because this codebase has already been bitten by amplifying 429s.
///
/// Three, matching `DelegationConfig::max_concurrent` — the codebase's existing
/// answer to "how many child agents at once" — rather than a new number nobody
/// has measured.
pub const MAX_CONCURRENT_JOBS: usize = 3;

/// Prepend job updates to a real user turn. Returns the text unchanged when
/// there is nothing to report, plus the job ids whose delivery the caller must
/// confirm with `JobLedger::mark_delivered` **after the turn succeeds**.
///
/// Confirming here instead would repeat the original bug: the note was marked
/// delivered while it was being built, so an interrupted turn (or a provider
/// error, or a model that ignored it) ate the only copy of the report and the
/// user never heard the outcome.
#[must_use]
pub fn wrap_prompt(ledger: &JobLedger, text: &str) -> (String, Vec<String>) {
    let (note, pending) = ledger.pending_updates();
    if note.is_empty() {
        return (text.to_owned(), pending);
    }
    (
        format!(
            "[System note — background job update, not yet seen by the user; relay it naturally in \
             your reply (on a call: speak the takeaway in a sentence or two). Report each job's state \
             AS GIVEN: do not describe an interrupted or unverified job as finished:\n{note}]\n\n{text}"
        ),
        pending,
    )
}

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "background_task".into(),
        description: "Start a long-running job in the background and return IMMEDIATELY: \
             building or changing software, deep research, generating documents, spreadsheets, \
             or presentations — anything needing more than a minute or two of work. A separate \
             agent with your full toolset runs it to completion; its result is delivered to you \
             automatically in a later turn, and you relay it to the user then. After calling \
             this, tell the user the job has started and that you'll report back — do NOT wait \
             or poll. Not for quick lookups or questions you can answer in this turn."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "task": {"type": "string", "description": "The job, in full, with all context the user gave — the background agent sees nothing else."},
                "label": {"type": "string", "description": "A short human label for the job, e.g. 'build the expense tracker'."}
            },
            "required": ["task"]
        }),
        toolset: "delegation".into(),
    }
}

/// Runs the job on a detached session via the live manager. `Weak` so the tool
/// never keeps the manager alive past shutdown.
pub struct BackgroundTaskTool {
    sessions: Weak<SessionManager>,
    ledger: Arc<JobLedger>,
    /// Shared across sessions — see [`MAX_CONCURRENT_JOBS`].
    slots: Arc<tokio::sync::Semaphore>,
}

impl BackgroundTaskTool {
    #[must_use]
    pub fn new(
        sessions: Weak<SessionManager>,
        ledger: Arc<JobLedger>,
        slots: Arc<tokio::sync::Semaphore>,
    ) -> Self {
        Self {
            sessions,
            ledger,
            slots,
        }
    }
}

#[async_trait]
impl ToolExecutor for BackgroundTaskTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(task) = args.get("task").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: task"));
        };
        let Some(sessions) = self.sessions.upgrade() else {
            return Ok(tool_error_json("deacon is shutting down"));
        };
        let label = args
            .get("label")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| task.chars().take(60).collect());

        // Dropped on every early return below, so a refused or duplicate claim
        // never leaks a slot; moved into the spawned task on the success path.
        let permit = match take_slot(&self.slots) {
            Ok(permit) => permit,
            Err(refusal) => return Ok(refusal),
        };

        let limits = JobLimits {
            max_attempts: 1,
            timeout_secs: Some(DEFAULT_TIMEOUT_SECS),
        };
        let (id, created) = match self.ledger.claim("background", &label, task, limits) {
            Ok(claimed) => claimed,
            Err(error) => return Ok(tool_error_json(format!("could not record job: {error}"))),
        };
        if !created {
            // The model re-fires this tool; the doom-loop guard caught four
            // repeats in a day. Hand back the original rather than doing the
            // work twice and leaving a twin to report "still running" forever.
            return Ok(json!({
                "started": false,
                "job_id": id,
                "label": label,
                "note": "This exact job is ALREADY running — this call did not start a second \
                         one. Tell the user it's still in progress; do not launch it again."
            })
            .to_string());
        }
        let Some(attempt) = self.ledger.start(&id, None) else {
            return Ok(tool_error_json("job could not be started"));
        };

        let ledger = Arc::clone(&self.ledger);
        let task = task.to_owned();
        let job_id = id.clone();
        let job_label = label.clone();
        tokio::spawn(async move {
            run_to_completion(sessions, ledger, job_id, attempt, task, job_label).await;
            // Held for exactly as long as the job runs; every exit from
            // `run_to_completion` (finished, failed, timed out, cancelled)
            // passes through here.
            drop(permit);
        });

        Ok(json!({
            "started": true,
            "job_id": id,
            "label": label,
            "note": "Running in the background. Tell the user it's started and you'll report \
                     the result when it's ready — do NOT wait for it in this turn. Its outcome \
                     arrives with an explicit state; relay that state, don't assume success."
        })
        .to_string())
    }
}

#[cfg(test)]
#[path = "tests/background_task_tool.rs"]
mod tests;
