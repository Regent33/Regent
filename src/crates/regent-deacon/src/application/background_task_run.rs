//! Driving one background job to a terminal state: the slot it holds, the
//! three ways out, and what a finished run entitles us to claim. Split from
//! `background_task_tool.rs` (file-size rule).

use super::background_task_tool::{DEFAULT_TIMEOUT_SECS, MAX_CONCURRENT_JOBS};
use crate::application::session_manager::SessionManager;
use regent_jobs::{Completion, Fact, JobLedger, StopReason};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::OwnedSemaphorePermit;

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
pub(super) fn assess(_report: &str, artifacts: usize) -> Completion {
    Completion {
        process_completed: Fact::Yes,
        artifact_produced: if artifacts > 0 { Fact::Yes } else { Fact::No },
        result_validated: Fact::Unknown,
        outcome_achieved: Fact::Unknown,
    }
}

/// How often a running job checks whether cancellation was requested.
const CANCEL_POLL_SECS: u64 = 15;

/// A slot for one job, or the refusal to hand the model instead.
///
/// **Refuse, never queue.** A queued job would sit in the ledger's `queued`
/// state for minutes, and that is precisely what boot recovery reclaims — it
/// has no attempt row, so no lease can protect it, and waiting here would
/// reintroduce the bug the lease was added to fix. Refusing is also the more
/// useful answer: the model can say "once the current ones finish" rather than
/// leaving the user waiting on a queue nothing surfaces.
pub(super) fn take_slot(
    slots: &Arc<tokio::sync::Semaphore>,
) -> Result<OwnedSemaphorePermit, String> {
    Arc::clone(slots).try_acquire_owned().map_err(|_| {
        json!({
            "started": false,
            "note": format!(
                "already running the maximum of {MAX_CONCURRENT_JOBS} background jobs. This one \
                 was NOT started: tell the user so plainly, and that it needs starting again \
                 once one finishes. Do NOT describe it as queued or in progress."),
        })
        .to_string()
    })
}

/// Drives one job to a terminal state. Three ways out, and each records a
/// DIFFERENT outcome — collapsing them is how a job that was killed for time
/// used to be indistinguishable from one that quietly succeeded.
pub(super) async fn run_to_completion(
    sessions: Arc<SessionManager>,
    ledger: Arc<JobLedger>,
    job_id: String,
    attempt: i64,
    task: String,
    label: String,
) {
    let state = drive(&sessions, &ledger, &job_id, attempt, task).await;
    // The ONE push a background job makes. Until this existed, a finished job
    // reached the user only by riding `wrap_prompt` into their next message —
    // so a job could complete and sit silent indefinitely while the user waited
    // for the "I'll report back" the tool told the model to promise. This does
    // not speak for the agent (no model call, no cost, nothing to barge into a
    // live turn); it tells the client the news exists. The detail still arrives
    // with the next turn.
    //
    // Emitted at the single exit, not at each of the four `return`s inside
    // `drive`, so a new way for a job to end cannot forget to announce itself.
    sessions.emit_event(
        "job.finished",
        json!({ "job_id": job_id, "label": label, "state": state }),
    );
}

/// Drives the job and records its outcome, returning the terminal state it
/// reached. Split from the notification above so every exit is announced.
async fn drive(
    sessions: &Arc<SessionManager>,
    ledger: &Arc<JobLedger>,
    job_id: &str,
    attempt: i64,
    task: String,
) -> &'static str {
    // Records the transcript this job runs in the moment it exists, so the
    // evidence pointer is set even if the job is later interrupted or killed.
    let work = sessions.run_detached_task(&task, |session| {
        ledger.attach_session(job_id, &session.to_string());
    });
    tokio::pin!(work);
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS));
    tokio::pin!(deadline);
    let mut poll = tokio::time::interval(std::time::Duration::from_secs(CANCEL_POLL_SECS));
    poll.tick().await; // the first tick is immediate; skip it

    loop {
        tokio::select! {
            outcome = &mut work => {
                return match outcome {
                    Ok(report) => {
                        let artifacts = ledger.artifacts(job_id).len();
                        ledger.finish(job_id, attempt, assess(&report, artifacts), Some(&report));
                        "finished"
                    }
                    Err(error) => {
                        ledger.fail(job_id, attempt, &error.to_string());
                        "failed"
                    }
                };
            }
            () = &mut deadline => {
                // Dropping the future cancels the async chain. The job did not
                // finish, and we cannot say whether it would have.
                ledger.stop(
                    job_id,
                    attempt,
                    StopReason::TimedOut,
                    "stopped after exceeding its 45-minute deadline; it did not finish",
                );
                return "timed_out";
            }
            _ = poll.tick() => {
                // ponytail: a primary-key read every 15s, straight on the
                // runtime. Move to spawn_blocking if the poll ever gets hot.
                if ledger.cancel_requested(job_id) {
                    ledger.stop(job_id, attempt, StopReason::Cancelled, "cancelled by the user");
                    return "cancelled";
                }
                // Proof of life on the tick that was already happening. Without
                // it, any other deacon booting (the CLI spawns one per command)
                // reclaims this job as abandoned while it is still working.
                ledger.heartbeat(job_id, attempt);
            }
        }
    }
}
