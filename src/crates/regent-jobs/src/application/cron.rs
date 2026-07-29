//! Cron executions on the job ledger (W1 / W7).
//!
//! A decorator, not a second ledger. `AgentJobRunner` lives in `regent-agent`,
//! which cannot see this crate, so the recording is composed in at the
//! composition root instead of reaching down the dependency graph.
//!
//! What cron gains: a durable row per execution, attempt history, the same
//! four completion facts as every other job, and the guarantee that a tick
//! firing while the previous execution is still running cannot start a second
//! copy. That last one comes from the ledger's idempotency index, which is
//! **partial** — `WHERE state IN ('queued','running')` — so a terminal row
//! releases the key and the next occurrence of a recurring job gets a fresh
//! row of its own. One row per *execution*, not per cron definition.

use super::ledger::{JobLedger, JobLimits};
use crate::domain::job::{Completion, Fact, StopReason};
use async_trait::async_trait;
use regent_cron::{CronError, CronJob, JobRunner};
use std::sync::Arc;

/// How often a live cron run renews its lease. Matches `background_task`'s
/// cancel poll so both paths sit the same distance inside `JOB_LEASE_SECS`.
const HEARTBEAT_SECS: u64 = 15;

/// Used when a caller does not pass a budget. Deliberately generous: a caller
/// that has not thought about a deadline should not have one imposed that is
/// shorter than its work.
const DEFAULT_BUDGET_SECS: u64 = 30 * 60;

/// What one cron execution gets. Unchanged from the deadline that governed
/// before — the scheduler's `hard_timeout_secs` default — because this fixes
/// *who records the outcome*, not how long a job may run.
pub const CRON_BUDGET_SECS: u64 = 180;

/// The scheduler's watchdog, which must sit **strictly above** the budget.
///
/// Both composition roots read these two rather than picking their own, so
/// there is one timeout policy instead of two that drift. If the watchdog were
/// equal, whichever timer won would be a race and the recorded reason would
/// depend on it; if it were shorter, the run would be cancelled before its own
/// deadline and the cause would be lost again — which is the bug that was here.
pub const CRON_WATCHDOG_SECS: u64 = CRON_BUDGET_SECS + 30;

// Enforced at compile time rather than in a test: getting this pair backwards
// reintroduces the exact defect, and a build failure is a better place to find
// that out than a run.
const _: () = assert!(CRON_WATCHDOG_SECS > CRON_BUDGET_SECS);

/// A safety net for a run that is dropped before it settles — **not** the
/// normal close path.
///
/// Every arm of `run`'s match closes the row, but only if `run` is still
/// running to reach them. A cancelled future runs no further code, so the
/// attempt sat at `running` until a lease expiry reclaimed it. That used to be
/// the *ordinary* case, because the scheduler's watchdog fired before this
/// runner's own deadline; the budget is now passed in from the composition root
/// so the run's own timeout wins and records `TimedOut` with the reason known.
/// What is left for this guard is genuine outside cancellation: shutdown, or a
/// caller that wraps the runner in a shorter timeout of its own.
struct OpenAttempt {
    ledger: Arc<JobLedger>,
    id: String,
    attempt: i64,
    settled: bool,
}

impl OpenAttempt {
    /// Disarms the guard, and **only** after the durable write is on disk.
    ///
    /// Ordering is the whole point: disarming first would mean `settled` read
    /// as "we are about to try to close this", so a write that failed — or a
    /// panic between the two — left the row open with nothing left to notice.
    /// Closing twice is safe to fall back on: `finish_job` matches on
    /// `state = 'running' AND attempts = ?`, so the second write is refused and
    /// logged rather than clobbering a newer attempt.
    fn settled(&mut self, close: impl FnOnce()) {
        close();
        self.settled = true;
    }
}

impl Drop for OpenAttempt {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        // Interrupted, not TimedOut: the run's own deadline records TimedOut
        // itself, so reaching this guard means something *outside* stopped us
        // and this layer cannot see what. Naming the likely causes beats
        // asserting one.
        //
        // `catch_unwind` because a `Drop` that panics during an unwind aborts
        // the process. The store's write path takes a mutex with `expect`, so a
        // panic that poisoned it would otherwise turn one failed job into a
        // dead deacon. A lost ledger row is recoverable; an abort is not.
        let closed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.ledger.stop(
                &self.id,
                self.attempt,
                StopReason::Interrupted,
                "stopped before it finished — shutdown, or a cancel from outside the run",
            );
        }));
        if closed.is_err() {
            tracing::error!(
                job = %self.id,
                attempt = self.attempt,
                "panicked while closing an abandoned job; the lease will reclaim it"
            );
        }
    }
}

pub struct LedgerCronRunner {
    inner: Arc<dyn JobRunner>,
    ledger: Arc<JobLedger>,
    budget_secs: u64,
}

impl LedgerCronRunner {
    #[must_use]
    pub fn new(inner: Arc<dyn JobRunner>, ledger: Arc<JobLedger>) -> Self {
        Self {
            inner,
            ledger,
            budget_secs: DEFAULT_BUDGET_SECS,
        }
    }

    /// How long one execution gets, and what the ledger stores as its deadline.
    ///
    /// Pass the same number the scheduler above was configured from, less its
    /// watchdog margin. Two timeout policies is how the stored `deadline_at`
    /// came to say thirty minutes for a run the scheduler killed after three:
    /// the deadline was real, it just was not the one that governed.
    #[must_use]
    pub fn with_budget(mut self, secs: u64) -> Self {
        self.budget_secs = secs;
        self
    }
}

#[async_trait]
impl JobRunner for LedgerCronRunner {
    async fn run(&self, job: &CronJob) -> Result<String, CronError> {
        let limits = JobLimits {
            max_attempts: 1,
            timeout_secs: Some(self.budget_secs),
        };
        // Keyed on the cron job's own id, so overlapping ticks collapse onto
        // one execution rather than racing.
        let claimed = self
            .ledger
            .claim("cron", &job.id, &job.prompt, limits)
            .map_err(|e| CronError::Storage(e.to_string()))?;
        let (id, created) = claimed;
        if !created {
            tracing::warn!(job = %job.id, "cron tick skipped: the previous run is still going");
            return Err(CronError::RunFailed(
                "the previous execution of this job is still running".into(),
            ));
        }
        let Some(attempt) = self.ledger.start(&id, None) else {
            // The row was just created and is `queued`, which the partial
            // idempotency index counts as live — so it holds the key and every
            // later tick is refused as "still running" until boot recovery
            // reclaims it. Nothing here can close it: `finish_job` only matches
            // a row that reached `running`. Named loudly with the id so the
            // wedge is diagnosable rather than mysterious.
            tracing::error!(
                job = %id,
                cron = %job.id,
                "claimed a cron job but could not start it; the key is held until recovery"
            );
            return Err(CronError::RunFailed(format!(
                "job {id} could not be started"
            )));
        };
        let mut open = OpenAttempt {
            ledger: Arc::clone(&self.ledger),
            id: id.clone(),
            attempt,
            settled: false,
        };

        // The deadline is enforced here AND stored, from one number, so the
        // `deadline_at` a reader sees is the one that actually governs.
        let timeout = std::time::Duration::from_secs(self.budget_secs);
        // Renew the lease while the run is in flight. Unlike `background_task`
        // this path has no poll loop of its own, so a cron run outliving the
        // lease would be reclaimed as abandoned by any deacon that happened to
        // boot — and the CLI boots one per command.
        let outcome = {
            let run = tokio::time::timeout(timeout, self.inner.run(job));
            tokio::pin!(run);
            let mut beat = tokio::time::interval(std::time::Duration::from_secs(HEARTBEAT_SECS));
            beat.tick().await; // the first tick is immediate
            loop {
                tokio::select! {
                    settled = &mut run => break settled,
                    _ = beat.tick() => self.ledger.heartbeat(&id, attempt),
                }
            }
        };
        match outcome {
            Ok(Ok(report)) => {
                // A cron run that returned is a completed process. Whether it
                // achieved anything is NOT something this layer can see — there
                // is no validator on this path — so `outcome_achieved` stays
                // unknown. Treating any non-empty reply as success would make a
                // job that returns "I couldn't reach the API" a success, which
                // is precisely the defect this ledger exists to remove.
                let completion = Completion {
                    process_completed: Fact::Yes,
                    artifact_produced: if self.ledger.artifacts(&id).is_empty() {
                        Fact::No
                    } else {
                        Fact::Yes
                    },
                    result_validated: Fact::Unknown,
                    outcome_achieved: Fact::Unknown,
                };
                open.settled(|| self.ledger.finish(&id, attempt, completion, Some(&report)));
                Ok(report)
            }
            Ok(Err(error)) => {
                open.settled(|| self.ledger.fail(&id, attempt, &error.to_string()));
                Err(error)
            }
            Err(_elapsed) => {
                let detail = format!(
                    "exceeded its {}s deadline and was stopped",
                    self.budget_secs
                );
                open.settled(|| {
                    self.ledger
                        .stop(&id, attempt, StopReason::TimedOut, &detail)
                });
                Err(CronError::RunFailed(detail))
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/cron.rs"]
mod tests;
