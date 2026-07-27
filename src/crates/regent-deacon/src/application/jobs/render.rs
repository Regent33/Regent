//! Turning job rows into the note prepended to a user turn.
//!
//! Kept separate from the ledger because this is where honesty is enforced in
//! words: every finished job is rendered with the caveat its evidence supports,
//! and an interrupted one is never dressed up as either success or failure.

use crate::domain::job::{Completion, Fact, JobState};
use regent_store::domain::job_entities::JobRow;

/// A finished result longer than this is trimmed — the full output still lives
/// in the job's session transcript, which `session_id` points at.
const RESULT_MAX_CHARS: usize = 4000;

fn completion_of(job: &JobRow) -> Completion {
    Completion {
        process_completed: Fact::parse(&job.process_completed),
        artifact_produced: Fact::parse(&job.artifact_produced),
        result_validated: Fact::parse(&job.result_validated),
        outcome_achieved: Fact::parse(&job.outcome_achieved),
    }
}

fn trimmed(text: &str) -> String {
    let mut out = text.trim().to_owned();
    if out.chars().count() > RESULT_MAX_CHARS {
        out = out.chars().take(RESULT_MAX_CHARS).collect();
        out.push_str("… (trimmed)");
    }
    out
}

fn render_finished(job: &JobRow, note: &mut String) {
    let state = JobState::parse(&job.state);
    let headline = match state {
        JobState::Succeeded => "FINISHED",
        JobState::Failed => "FAILED",
        JobState::Cancelled => "CANCELLED",
        JobState::Interrupted => "INTERRUPTED — cut off by a restart, NOT finished",
        _ => "NO VERIFIED RESULT",
    };
    note.push_str(&format!("- {headline}: {}\n", job.label));

    if let Some(result) = job.result.as_deref().filter(|r| !r.trim().is_empty()) {
        note.push_str(&format!("  Result: {}\n", trimmed(result)));
    }
    if let Some(error) = job.error.as_deref().filter(|e| !e.trim().is_empty()) {
        note.push_str(&format!("  Error: {}\n", trimmed(error)));
    }
    if let Some(caveat) = completion_of(job).caveat() {
        note.push_str(&format!("  Caveat: {caveat}\n"));
    }
    if state == JobState::Interrupted {
        note.push_str(
            "  This job never ran to the end. Say so plainly; do not imply it is done, \
             and offer to start it again.\n",
        );
    }
}

fn render_live(job: &JobRow, now: f64, note: &mut String) {
    let mins = ((now - job.created_at).max(0.0) / 60.0) as u64;
    match job.deadline_at {
        Some(deadline) if now >= deadline => note.push_str(&format!(
            "- OVERDUE ({mins}m, past its deadline): {}\n",
            job.label
        )),
        _ => note.push_str(&format!("- STILL RUNNING ({mins}m): {}\n", job.label)),
    }
}

/// Builds the system note. Returns an empty string when there is nothing to
/// report, so the caller can leave the user's turn untouched.
#[must_use]
pub fn render_updates(finished: &[JobRow], live: &[JobRow]) -> String {
    if finished.is_empty() && live.is_empty() {
        return String::new();
    }
    let now = regent_store::infra::db::now_epoch();
    let mut note = String::new();
    for job in finished {
        render_finished(job, &mut note);
    }
    for job in live {
        render_live(job, now, &mut note);
    }
    note
}

#[cfg(test)]
#[path = "../tests/jobs_render.rs"]
mod tests;
