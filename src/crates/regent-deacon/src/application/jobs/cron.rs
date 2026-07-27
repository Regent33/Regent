//! Cron executions on the job ledger (W1 / W7).
//!
//! A decorator, not a second ledger. `AgentJobRunner` lives in `regent-agent`,
//! which cannot see this crate, so the recording is composed in at the
//! composition root instead of reaching down the dependency graph.
//!
//! What cron gains: a durable row per execution, attempt history, the same
//! four completion facts as every other job, and — because the ledger's
//! idempotency key is enforced by a unique index — the guarantee that a tick
//! firing while the previous execution is still running cannot start a second
//! copy of the same job.

use super::{JobLedger, JobLimits};
use crate::domain::job::{Completion, Fact, StopReason};
use async_trait::async_trait;
use regent_cron::{CronError, CronJob, JobRunner};
use std::sync::Arc;

/// A cron execution is stopped after this long. Enforced with a real timeout,
/// not just recorded: a wedged run would otherwise hold its key forever.
const CRON_TIMEOUT_SECS: u64 = 30 * 60;

pub struct LedgerCronRunner {
    inner: Arc<dyn JobRunner>,
    ledger: Arc<JobLedger>,
}

impl LedgerCronRunner {
    #[must_use]
    pub fn new(inner: Arc<dyn JobRunner>, ledger: Arc<JobLedger>) -> Self {
        Self { inner, ledger }
    }
}

#[async_trait]
impl JobRunner for LedgerCronRunner {
    async fn run(&self, job: &CronJob) -> Result<String, CronError> {
        let limits = JobLimits {
            max_attempts: 1,
            timeout_secs: Some(CRON_TIMEOUT_SECS),
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
            return Err(CronError::RunFailed("job could not be started".into()));
        };

        // The deadline is enforced here, not merely recorded. Without this the
        // stored `deadline_at` was decorative: a wedged run held its
        // idempotency key forever and silently suppressed every later tick.
        let timeout = std::time::Duration::from_secs(CRON_TIMEOUT_SECS);
        match tokio::time::timeout(timeout, self.inner.run(job)).await {
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
                self.ledger.finish(&id, attempt, completion, Some(&report));
                Ok(report)
            }
            Ok(Err(error)) => {
                self.ledger.fail(&id, attempt, &error.to_string());
                Err(error)
            }
            Err(_elapsed) => {
                let detail = format!(
                    "exceeded its {}s deadline and was stopped",
                    CRON_TIMEOUT_SECS
                );
                self.ledger
                    .stop(&id, attempt, StopReason::TimedOut, &detail);
                Err(CronError::RunFailed(detail))
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/jobs_cron.rs"]
mod tests;
