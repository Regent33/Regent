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

use crate::application::jobs::{JobLedger, JobLimits};
use crate::application::session_manager::SessionManager;
use crate::domain::job::{Completion, Fact, StopReason};
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_tools::{ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::{Arc, Weak};

/// A background job that has produced nothing for this long is past its
/// deadline and is reported as overdue. It is not killed — see `JobLedger`.
const DEFAULT_TIMEOUT_SECS: u64 = 45 * 60;

/// Prepend job updates to a real user turn. Returns the text unchanged when
/// there is nothing to report.
#[must_use]
pub fn wrap_prompt(ledger: &JobLedger, text: &str) -> String {
    let note = ledger.updates();
    if note.is_empty() {
        return text.to_owned();
    }
    format!(
        "[System note — background job update, not yet seen by the user; relay it naturally in \
         your reply (on a call: speak the takeaway in a sentence or two). Report each job's state \
         AS GIVEN: do not describe an interrupted or unverified job as finished:\n{note}]\n\n{text}"
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
}

impl BackgroundTaskTool {
    #[must_use]
    pub fn new(sessions: Weak<SessionManager>, ledger: Arc<JobLedger>) -> Self {
        Self { sessions, ledger }
    }
}

/// What a finished detached run entitles us to claim.
///
/// Only two of the four facts are observable here. The process returned, and
/// we can count what it registered as artifacts. Nothing on this path checks
/// the output, and nothing judges whether the goal was met.
///
/// `outcome_achieved` therefore stays `Unknown`, which makes every background
/// job `Inconclusive` until something actually validates it. An earlier draft
/// inferred `Yes` from "an artifact exists and the report is non-empty" — but
/// an artifact can be partial or irrelevant and a report can describe its own
/// failure, so that was the original defect wearing a new name. The agent's
/// own account of what it did survives verbatim in `result`; it is a claim the
/// user can read, not a fact the system asserts.
fn assess(_report: &str, artifacts: usize) -> Completion {
    Completion {
        process_completed: Fact::Yes,
        artifact_produced: if artifacts > 0 { Fact::Yes } else { Fact::No },
        result_validated: Fact::Unknown,
        outcome_achieved: Fact::Unknown,
    }
}

/// How often a running job checks whether cancellation was requested.
const CANCEL_POLL_SECS: u64 = 15;

/// Drives one job to a terminal state. Three ways out, and each records a
/// DIFFERENT outcome — collapsing them is how a job that was killed for time
/// used to be indistinguishable from one that quietly succeeded.
async fn run_to_completion(
    sessions: Arc<SessionManager>,
    ledger: Arc<JobLedger>,
    job_id: String,
    attempt: i64,
    task: String,
) {
    // Records the transcript this job runs in the moment it exists, so the
    // evidence pointer is set even if the job is later interrupted or killed.
    let work = sessions.run_detached_task(&task, |session| {
        ledger.attach_session(&job_id, &session.to_string());
    });
    tokio::pin!(work);
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS));
    tokio::pin!(deadline);
    let mut poll = tokio::time::interval(std::time::Duration::from_secs(CANCEL_POLL_SECS));
    poll.tick().await; // the first tick is immediate; skip it

    loop {
        tokio::select! {
            outcome = &mut work => {
                match outcome {
                    Ok(report) => {
                        let artifacts = ledger.artifacts(&job_id).len();
                        ledger.finish(&job_id, attempt, assess(&report, artifacts), Some(&report));
                    }
                    Err(error) => ledger.fail(&job_id, attempt, &error.to_string()),
                }
                return;
            }
            () = &mut deadline => {
                // Dropping the future cancels the async chain. The job did not
                // finish, and we cannot say whether it would have.
                ledger.stop(
                    &job_id,
                    attempt,
                    StopReason::TimedOut,
                    "stopped after exceeding its 45-minute deadline; it did not finish",
                );
                return;
            }
            _ = poll.tick() => {
                // ponytail: a primary-key read every 15s, straight on the
                // runtime. Move to spawn_blocking if the poll ever gets hot.
                if ledger.cancel_requested(&job_id) {
                    ledger.stop(&job_id, attempt, StopReason::Cancelled, "cancelled by the user");
                    return;
                }
                // Proof of life on the tick that was already happening. Without
                // it, any other deacon booting (the CLI spawns one per command)
                // reclaims this job as abandoned while it is still working.
                ledger.heartbeat(&job_id, attempt);
            }
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
        tokio::spawn(async move {
            run_to_completion(sessions, ledger, job_id, attempt, task).await;
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
