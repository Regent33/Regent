//! Job-ledger read paths (W1). Split from `jobs.rs` for the file-size rule;
//! same `impl Store`, so callers see one API.

use crate::domain::errors::StoreError;
use crate::domain::job_entities::{JobAttemptRow, JobRow};
use crate::infra::db::{Store, now_epoch};
use crate::infra::jobs::{COLUMNS, row_to_job};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Jobs in a terminal state whose outcome has not been relayed yet.
    pub fn undelivered_jobs(&self) -> Result<Vec<JobRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS} FROM jobs
                 WHERE delivered_at IS NULL AND state NOT IN ('queued', 'running')
                 ORDER BY updated_at, id"
            ))?;
            stmt.query_map([], row_to_job)?.collect()
        })
    }

    /// Jobs still `queued` or `running`.
    pub fn live_jobs(&self) -> Result<Vec<JobRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS} FROM jobs WHERE state IN ('queued', 'running')
                 ORDER BY created_at, id"
            ))?;
            stmt.query_map([], row_to_job)?.collect()
        })
    }

    pub fn job(&self, id: &str) -> Result<Option<JobRow>, StoreError> {
        self.with_read(|conn| {
            conn.query_row(
                &format!("SELECT {COLUMNS} FROM jobs WHERE id = ?1"),
                params![id],
                row_to_job,
            )
            .optional()
        })
    }

    pub fn mark_job_delivered(&self, id: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE jobs SET delivered_at = ?2 WHERE id = ?1",
                params![id, now_epoch()],
            )?;
            Ok(())
        })
    }

    /// Requests cancellation. The runner observes the flag; this only records
    /// the intent, so a wedged job still needs its own deadline to stop.
    pub fn request_job_cancel(&self, id: &str) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let changed = tx.execute(
                "UPDATE jobs SET cancel_requested = 1, updated_at = ?2
                 WHERE id = ?1 AND state IN ('queued', 'running')",
                params![id, now_epoch()],
            )?;
            Ok(changed > 0)
        })
    }

    /// Marks every job left `running` as `interrupted`. Called once at boot:
    /// a job whose process died did not succeed and did not fail, and saying
    /// either would be a lie.
    pub fn interrupt_running_jobs(&self) -> Result<usize, StoreError> {
        self.with_write(|tx| {
            let now = now_epoch();
            tx.execute(
                "UPDATE job_attempts SET ended_at = ?1, outcome = 'interrupted'
                 WHERE ended_at IS NULL",
                params![now],
            )?;
            let changed = tx.execute(
                "UPDATE jobs SET state = 'interrupted', process_completed = 'no',
                        outcome_achieved = 'unknown', updated_at = ?1
                 WHERE state IN ('queued', 'running')",
                params![now],
            )?;
            Ok(changed)
        })
    }

    pub fn record_job_artifact(&self, job_id: &str, path: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO job_artifacts (job_id, path, recorded_at) VALUES (?1, ?2, ?3)",
                params![job_id, path, now_epoch()],
            )?;
            Ok(())
        })
    }

    pub fn job_artifacts(&self, job_id: &str) -> Result<Vec<String>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn
                .prepare("SELECT path FROM job_artifacts WHERE job_id = ?1 ORDER BY recorded_at")?;
            stmt.query_map(params![job_id], |r| r.get(0))?.collect()
        })
    }

    pub fn job_attempts(&self, job_id: &str) -> Result<Vec<JobAttemptRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT job_id, attempt, session_id, started_at, ended_at, outcome, error
                 FROM job_attempts WHERE job_id = ?1 ORDER BY attempt",
            )?;
            stmt.query_map(params![job_id], |row| {
                Ok(JobAttemptRow {
                    job_id: row.get(0)?,
                    attempt: row.get(1)?,
                    session_id: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    outcome: row.get(5)?,
                    error: row.get(6)?,
                })
            })?
            .collect()
        })
    }
}
