//! The tick scheduler — all the hardening invariants live here, in
//! harness code: tick lock, catch-up clamp, hard run timeout, one-shot
//! retirement. The runner only ever sees one due job at a time.

use crate::domain::contracts::{JobRepository, JobRunner};
use crate::domain::entities::{CronJob, RunOutcome, RunStatus};
use crate::domain::errors::CronError;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// 3-minute hard interrupt on cron sessions.
    pub hard_timeout_secs: u64,
    /// Catch-up window clamp bounds (half the period, clamped to these).
    pub catchup_min_secs: u64,
    pub catchup_max_secs: u64,
    /// Grace for one-shot jobs whose fire time was missed.
    pub oneshot_grace_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            hard_timeout_secs: 180,
            catchup_min_secs: 120,
            catchup_max_secs: 7_200,
            oneshot_grace_secs: 120,
        }
    }
}

pub struct Scheduler {
    repository: Arc<dyn JobRepository>,
    runner: Arc<dyn JobRunner>,
    config: SchedulerConfig,
}

impl Scheduler {
    #[must_use]
    pub fn new(
        repository: Arc<dyn JobRepository>,
        runner: Arc<dyn JobRunner>,
        config: SchedulerConfig,
    ) -> Self {
        Self { repository, runner, config }
    }

    /// One tick: under the tick lock, run every due job (bounded by the
    /// hard timeout each) and persist updated fire times. Returns the
    /// outcomes; an empty vec when nothing was due or the lock was held
    /// elsewhere.
    pub async fn tick(&self, now: f64) -> Result<Vec<RunOutcome>, CronError> {
        let Some(_guard) = self.repository.try_lock_tick()? else {
            tracing::debug!("tick lock held elsewhere; skipping");
            return Ok(Vec::new());
        };
        let mut jobs = self.repository.load()?;
        let mut outcomes = Vec::new();
        let mut dirty = false;

        for job in jobs.iter_mut().filter(|j| j.enabled) {
            if job.next_run_at > now {
                continue;
            }
            let lateness = now - job.next_run_at;
            if lateness > self.catchup_window(job) {
                tracing::warn!(job = job.name, lateness, "missed beyond catch-up window; skipping forward");
                outcomes.push(RunOutcome {
                    job_id: job.id.clone(),
                    job_name: job.name.clone(),
                    status: RunStatus::SkippedCatchup,
                    summary: format!("missed by {lateness:.0}s — skipped"),
                });
                Self::advance(job, now);
                dirty = true;
                continue;
            }

            let status_summary = match tokio::time::timeout(
                Duration::from_secs(self.config.hard_timeout_secs),
                self.runner.run(job),
            )
            .await
            {
                Err(_) => (
                    RunStatus::TimedOut,
                    format!("hard timeout after {}s (run aborted)", self.config.hard_timeout_secs),
                ),
                Ok(Err(error)) => (RunStatus::Failed, error.to_string()),
                Ok(Ok(summary)) => (RunStatus::Ok, summary),
            };
            tracing::info!(job = job.name, status = ?status_summary.0, "cron job ran");
            outcomes.push(RunOutcome {
                job_id: job.id.clone(),
                job_name: job.name.clone(),
                status: status_summary.0,
                summary: status_summary.1,
            });
            job.last_run_at = Some(now);
            Self::advance(job, now);
            dirty = true;
        }

        if dirty {
            self.repository.save(&jobs)?;
        }
        Ok(outcomes)
    }

    fn catchup_window(&self, job: &CronJob) -> f64 {
        match job.schedule.period_seconds() {
            Some(period) => {
                (period / 2).clamp(self.config.catchup_min_secs, self.config.catchup_max_secs) as f64
            }
            None => self.config.oneshot_grace_secs as f64,
        }
    }

    fn advance(job: &mut CronJob, now: f64) {
        match job.schedule.next_after(now) {
            Some(next) => job.next_run_at = next,
            None => {
                // Exhausted one-shot retires; rows are never deleted.
                job.enabled = false;
            }
        }
    }
}
