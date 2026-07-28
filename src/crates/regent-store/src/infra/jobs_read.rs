//! Job-ledger read paths (W1). Split from `jobs.rs` for the file-size rule;
//! same `impl Store`, so callers see one API.

use crate::domain::errors::StoreError;
use crate::domain::job_entities::{JobAttemptRow, JobRow};
use crate::infra::db::{Store, now_epoch};
use crate::infra::jobs::{COLUMNS, row_to_job};
use rusqlite::{OptionalExtension, params};

/// How long a running attempt stays claimed without a heartbeat. Four times
/// the runner's 15-second poll: long enough that a slow tick, a paused laptop
/// or a busy write lock never costs a live job its claim, short enough that a
/// genuinely crashed one is reclaimed by the next boot rather than lingering.
pub const JOB_LEASE_SECS: f64 = 60.0;

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

    /// Marks abandoned jobs `interrupted`. Called at boot: a job whose process
    /// died did not succeed and did not fail, and saying either would be a lie.
    ///
    /// **Abandoned, not merely running.** This used to interrupt every
    /// `queued`/`running` row unconditionally, which is only correct if one
    /// deacon can ever exist. It cannot: the CLI spawns a deacon per command,
    /// beside the long-lived one the voice server holds. Measured 2026-07-29 —
    /// a running job flipped to `interrupted` because someone ran `regent
    /// status`, and the real outcome was later refused as stale by
    /// `finish_job`'s attempt guard. A successful job reported as interrupted,
    /// its result dropped, triggered by a read-only command.
    ///
    /// So a running attempt is only reclaimed once its lease has expired.
    /// `queued` has no attempt row and therefore no owner to respect.
    pub fn interrupt_running_jobs(&self) -> Result<usize, StoreError> {
        self.interrupt_abandoned_jobs(JOB_LEASE_SECS)
    }

    /// [`Store::interrupt_running_jobs`] with an explicit lease. A lease of
    /// `0.0` reclaims every attempt not heartbeating this instant — the old
    /// unconditional behaviour, which is what a test means when it says "the
    /// process died".
    pub fn interrupt_abandoned_jobs(&self, lease_secs: f64) -> Result<usize, StoreError> {
        self.with_write(|tx| {
            let now = now_epoch();
            let stale = now - lease_secs;
            tx.execute(
                "UPDATE job_attempts SET ended_at = ?1, outcome = 'interrupted'
                 WHERE ended_at IS NULL
                   AND (heartbeat_at IS NULL OR heartbeat_at < ?2)",
                params![now, stale],
            )?;
            let changed = tx.execute(
                "UPDATE jobs SET state = 'interrupted', process_completed = 'no',
                        outcome_achieved = 'unknown', updated_at = ?1
                 WHERE state = 'queued'
                    OR (state = 'running' AND NOT EXISTS (
                          SELECT 1 FROM job_attempts a
                           WHERE a.job_id = jobs.id AND a.ended_at IS NULL))",
                params![now],
            )?;
            Ok(changed)
        })
    }

    /// Renews a running attempt's lease. Cheap by design — the background
    /// runner already wakes on a timer to check for cancellation, so this rides
    /// a tick that was happening anyway.
    pub fn heartbeat_job(&self, id: &str, attempt: i64) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE job_attempts SET heartbeat_at = ?3
                 WHERE job_id = ?1 AND attempt = ?2 AND ended_at IS NULL",
                params![id, attempt, now_epoch()],
            )?;
            Ok(())
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
