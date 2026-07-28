//! The job ledger (W1): one durable record per unit of work that outlives the
//! turn that started it.
//!
//! Replaces a process-global `Vec<Task>` that lost every running job on
//! restart, while the user had already been told "I'll report back". The three
//! things this layer owns:
//!
//! - **Durability + recovery.** [`JobLedger::recover`] runs at boot and turns
//!   every job left running into `interrupted` — reported once, honestly.
//! - **Idempotency.** Re-firing the same work returns the original job id.
//! - **Honest reporting.** [`JobLedger::render_updates`] renders outcomes with
//!   the caveat their evidence supports, so "done" cannot mean "the process
//!   exited".
//!
//! Deliberately NOT here: how a job is executed, retry policy, scheduling. This
//! is the place a god abstraction would grow, so it stays a ledger.

mod cron;
mod live;
mod render;

use crate::domain::job::{Completion, JobState, StopReason};
use regent_store::Store;
use std::sync::Arc;

pub use cron::LedgerCronRunner;
pub use render::render_updates;

/// Bounds a single job. `None` deadline = no timeout (the historical
/// behaviour); a stalled job is then only visible, never stopped.
#[derive(Debug, Clone, Copy)]
pub struct JobLimits {
    pub max_attempts: i64,
    pub timeout_secs: Option<u64>,
}

impl Default for JobLimits {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            timeout_secs: None,
        }
    }
}

pub struct JobLedger {
    store: Arc<Store>,
}

impl JobLedger {
    #[must_use]
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Boot recovery. A job left running by a process that is **gone** is
    /// marked `interrupted` so it is delivered as news rather than silently
    /// lost. Returns how many were recovered.
    ///
    /// "Gone" is decided by the lease, not by this process starting up: several
    /// deacons run at once here (the CLI spawns one per command), so a boot is
    /// no evidence that anyone else died. See
    /// [`regent_store::Store::interrupt_running_jobs`].
    pub fn recover(&self) -> usize {
        self.recover_abandoned(regent_store::infra::jobs_read::JOB_LEASE_SECS)
    }

    /// [`JobLedger::recover`] with an explicit lease; `0.0` reclaims everything
    /// not heartbeating right now, which is what "the process died" means.
    pub fn recover_abandoned(&self, lease_secs: f64) -> usize {
        match self.store.interrupt_abandoned_jobs(lease_secs) {
            Ok(n) => {
                if n > 0 {
                    tracing::info!(jobs = n, "recovered jobs interrupted by a restart");
                }
                n
            }
            Err(error) => {
                tracing::warn!(%error, "job recovery failed");
                0
            }
        }
    }

    /// Registers a job, or hands back the live one already doing this work.
    /// `Ok((id, false))` means an identical job was already in flight.
    pub fn claim(
        &self,
        kind: &str,
        label: &str,
        task: &str,
        limits: JobLimits,
    ) -> Result<(String, bool), regent_store::domain::errors::StoreError> {
        // The key is the work, not the wording: same kind + same label is the
        // same job. `label` is what the model names the work, and it re-fires
        // with the same one.
        //
        // The id must NOT be derived from the key alone. The key is released
        // when a job reaches a terminal state (that is the point of the partial
        // index), so the next legitimate run of the same work needs a fresh
        // row — a key-derived id collided on the primary key instead, which
        // wedged a cron schedule permanently after its first failure.
        let key = format!("{kind}:{label}");
        let id = format!("job-{}", next_job_id(&key));
        let deadline = limits
            .timeout_secs
            .map(|secs| regent_store::infra::db::now_epoch() + secs as f64);
        self.store
            .claim_job(&id, kind, label, task, &key, limits.max_attempts, deadline)
    }

    /// Opens an attempt. `Ok(None)` = somebody else already has it, or it is
    /// not in a startable state.
    pub fn start(&self, id: &str, session_id: Option<&str>) -> Option<i64> {
        match self.store.start_job_attempt(id, session_id) {
            Ok(attempt) => attempt,
            Err(error) => {
                tracing::warn!(%error, job = id, "could not start job attempt");
                None
            }
        }
    }

    /// Closes a job. The state is DERIVED from the completion facts (see
    /// [`Completion::verdict`]) rather than asserted by the caller — that
    /// derivation is what stops a returning process from meaning success.
    pub fn finish(&self, id: &str, attempt: i64, completion: Completion, result: Option<&str>) {
        let state = completion.verdict();
        self.close(id, attempt, state, completion, result, None);
    }

    /// Closes a job that died. Distinct from [`JobLedger::finish`] because an
    /// error is evidence about the process, not about the outcome.
    pub fn fail(&self, id: &str, attempt: i64, error: &str) {
        self.close(
            id,
            attempt,
            JobState::Failed,
            Completion::process_failed(),
            None,
            Some(error),
        );
    }

    /// Closes a job the RUNTIME stopped — cancelled, or past its deadline.
    ///
    /// Takes a [`StopReason`] rather than a free `JobState` so a caller cannot
    /// stamp `Succeeded` on a job nobody let finish. The completion facts are
    /// all unknown except `process_completed = No`: stopping a job tells us it
    /// did not finish, and nothing at all about whether the goal was met.
    pub fn stop(&self, id: &str, attempt: i64, reason: StopReason, detail: &str) {
        let mut completion = Completion::unknown();
        completion.process_completed = crate::domain::job::Fact::No;
        self.close(id, attempt, reason.state(), completion, None, Some(detail));
    }

    #[allow(clippy::too_many_arguments)]
    fn close(
        &self,
        id: &str,
        attempt: i64,
        state: JobState,
        completion: Completion,
        result: Option<&str>,
        error: Option<&str>,
    ) {
        match self.store.finish_job(
            id,
            attempt,
            state.as_str(),
            completion.as_row(),
            result,
            error,
        ) {
            // Refused means this worker is stale: the job moved on (another
            // boot interrupted it, another attempt owns it now). Dropping the
            // outcome is correct — writing it would clobber the live attempt.
            Ok(false) => tracing::warn!(
                job = id,
                attempt,
                "stale job outcome refused; a newer attempt owns this job"
            ),
            Ok(true) => {}
            Err(error) => tracing::warn!(%error, job = id, "could not record job outcome"),
        }
    }

    /// Records the transcript a job is running in — the evidence behind any
    /// later claim about it.
    pub fn attach_session(&self, id: &str, session_id: &str) {
        if let Err(error) = self.store.attach_job_session(id, session_id) {
            tracing::warn!(%error, job = id, "could not attach job session");
        }
    }

    /// The text prepended to a real user turn: finished outcomes (delivered
    /// once) plus what is still in flight. Empty when there is nothing to say.
    pub fn updates(&self) -> String {
        let finished = self.store.undelivered_jobs().unwrap_or_default();
        let live = self.store.live_jobs().unwrap_or_default();
        let note = render_updates(&finished, &live);
        for job in &finished {
            if let Err(error) = self.store.mark_job_delivered(&job.id) {
                tracing::warn!(%error, job = %job.id, "could not mark job delivered");
            }
        }
        note
    }
}

/// A fresh id for a new job row. Readable (it carries a hash of the work) but
/// always distinct — identity is enforced by the DB's unique index on the
/// idempotency key, never by this string.
fn next_job_id(key: &str) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // Epoch too, so ids stay distinct across restarts of the same process.
    let epoch = regent_store::infra::db::now_epoch() as u64;
    format!("{hash:016x}-{epoch:x}-{seq:x}")
}

#[cfg(test)]
#[path = "../tests/jobs.rs"]
mod tests;
