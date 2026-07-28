//! Job-ledger persistence (W1). Dumb CRUD with two invariants that belong in
//! the DB rather than in a caller's head:
//!
//! 1. **Idempotency is a unique index**, not a scan-then-insert. `claim_job`
//!    returns the existing job's id when one is already live for the same key,
//!    so the model re-firing `background_task` cannot create twins.
//! 2. **Starting an attempt is a single conditional UPDATE**, so two workers
//!    never run the same job.
//!
//! All policy (what the states mean, when to retry, how to judge completion)
//! lives above this layer.

use crate::domain::errors::StoreError;
use crate::domain::job_entities::JobRow;
use crate::infra::db::{Store, now_epoch};
use rusqlite::{OptionalExtension, params};

pub(crate) const COLUMNS: &str = "id, kind, label, task, idempotency_key, state, session_id, attempts, \
     max_attempts, deadline_at, cancel_requested, process_completed, artifact_produced, \
     result_validated, outcome_achieved, result, error, delivered_at, created_at, updated_at";

pub(crate) fn row_to_job(row: &rusqlite::Row<'_>) -> Result<JobRow, rusqlite::Error> {
    Ok(JobRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        label: row.get(2)?,
        task: row.get(3)?,
        idempotency_key: row.get(4)?,
        state: row.get(5)?,
        session_id: row.get(6)?,
        attempts: row.get(7)?,
        max_attempts: row.get(8)?,
        deadline_at: row.get(9)?,
        cancel_requested: row.get::<_, i64>(10)? != 0,
        process_completed: row.get(11)?,
        artifact_produced: row.get(12)?,
        result_validated: row.get(13)?,
        outcome_achieved: row.get(14)?,
        result: row.get(15)?,
        error: row.get(16)?,
        delivered_at: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
    })
}

impl Store {
    /// Registers a job, or returns the id of the live one holding the same
    /// idempotency key. `Ok((id, true))` means this call created it.
    // Eight plain columns in, one row out. A params struct here would add a
    // type whose only job is to be destructured immediately — the repo's other
    // wide constructors (`SessionManager::new`) take the same call.
    #[allow(clippy::too_many_arguments)]
    pub fn claim_job(
        &self,
        id: &str,
        kind: &str,
        label: &str,
        task: &str,
        idempotency_key: &str,
        max_attempts: i64,
        deadline_at: Option<f64>,
    ) -> Result<(String, bool), StoreError> {
        self.with_write(|tx| {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM jobs WHERE idempotency_key = ?1
                     AND state IN ('queued', 'running')",
                    params![idempotency_key],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(id) = existing {
                return Ok((id, false));
            }
            let now = now_epoch();
            tx.execute(
                "INSERT INTO jobs (id, kind, label, task, idempotency_key, state, max_attempts,
                                   deadline_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6, ?7, ?8, ?8)",
                params![
                    id,
                    kind,
                    label,
                    task,
                    idempotency_key,
                    max_attempts,
                    deadline_at,
                    now
                ],
            )?;
            Ok((id.to_owned(), true))
        })
    }

    /// Moves a job to `running` and opens an attempt row. Conditional on the
    /// job still being startable AND having attempts left, so a second worker
    /// gets `Ok(None)` and `max_attempts` is enforced here rather than trusted
    /// to a caller. Returns the attempt number, which the caller must present
    /// to [`Store::finish_job`] — that pairing is what stops a stale worker
    /// from closing a newer attempt.
    pub fn start_job_attempt(
        &self,
        id: &str,
        session_id: Option<&str>,
    ) -> Result<Option<i64>, StoreError> {
        self.with_write(|tx| {
            let now = now_epoch();
            let changed = tx.execute(
                "UPDATE jobs SET state = 'running', attempts = attempts + 1,
                        session_id = COALESCE(?2, session_id), updated_at = ?3
                 WHERE id = ?1 AND state IN ('queued', 'interrupted')
                   AND attempts < max_attempts",
                params![id, session_id, now],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            let attempt: i64 = tx.query_row(
                "SELECT attempts FROM jobs WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )?;
            // The lease starts the moment the attempt does, so a job younger
            // than one heartbeat interval is never mistaken for abandoned.
            tx.execute(
                "INSERT INTO job_attempts (job_id, attempt, session_id, started_at, heartbeat_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![id, attempt, session_id, now],
            )?;
            Ok(Some(attempt))
        })
    }

    /// Points a job at the transcript it is running in. Set as soon as the
    /// session exists, so an interrupted or killed job still has its evidence
    /// pointer. Only ever fills a NULL — a retry does not erase attempt 1's.
    pub fn attach_job_session(&self, id: &str, session_id: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE jobs SET session_id = ?2, updated_at = ?3
                 WHERE id = ?1 AND session_id IS NULL",
                params![id, session_id, now_epoch()],
            )?;
            tx.execute(
                "UPDATE job_attempts SET session_id = ?2
                 WHERE job_id = ?1 AND ended_at IS NULL AND session_id IS NULL",
                params![id, session_id],
            )?;
            Ok(())
        })
    }

    /// Closes a job in a terminal state and stamps the four completion facts.
    ///
    /// FENCED on `attempt`: a worker may only close the attempt it started, and
    /// only while the job is still running. Without this a process that was
    /// declared interrupted at another boot could return late and overwrite a
    /// newer attempt's outcome — reporting `succeeded` over work that a second
    /// worker was still doing. Returns whether the close was accepted.
    #[allow(clippy::too_many_arguments)]
    pub fn finish_job(
        &self,
        id: &str,
        attempt: i64,
        state: &str,
        facts: [&str; 4],
        result: Option<&str>,
        error: Option<&str>,
    ) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let now = now_epoch();
            let changed = tx.execute(
                "UPDATE jobs SET state = ?2, process_completed = ?3, artifact_produced = ?4,
                        result_validated = ?5, outcome_achieved = ?6, result = ?7, error = ?8,
                        updated_at = ?9
                 WHERE id = ?1 AND state = 'running' AND attempts = ?10",
                params![
                    id, state, facts[0], facts[1], facts[2], facts[3], result, error, now, attempt
                ],
            )?;
            if changed == 0 {
                return Ok(false);
            }
            tx.execute(
                "UPDATE job_attempts SET ended_at = ?4, outcome = ?3, error = ?5
                 WHERE job_id = ?1 AND attempt = ?2 AND ended_at IS NULL",
                params![id, attempt, state, now, error],
            )?;
            Ok(true)
        })
    }
}

#[cfg(test)]
#[path = "tests/jobs.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/jobs_fencing.rs"]
mod fencing_tests;

#[cfg(test)]
#[path = "tests/jobs_lease.rs"]
mod lease_tests;
