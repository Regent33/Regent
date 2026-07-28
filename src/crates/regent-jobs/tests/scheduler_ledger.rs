//! The real composition: `Scheduler` → `LedgerCronRunner` → the work.
//!
//! The unit tests next to the decorator simulate the scheduler with a bare
//! `tokio::time::timeout`, which is only as good as my reading of what the
//! scheduler does. This drives the actual `Scheduler` over a real job file, so
//! the premise is tested rather than assumed: its `hard_timeout_secs` fires
//! before the decorator's own deadline and **drops** the run mid-await.

use async_trait::async_trait;
use regent_cron::{
    CronError, CronJob, FsJobRepository, JobRepository, JobRunner, Schedule, Scheduler,
    SchedulerConfig,
};
use regent_jobs::{JobLedger, LedgerCronRunner};
use regent_store::Store;
use std::sync::Arc;

struct Slow(u64);

#[async_trait]
impl JobRunner for Slow {
    async fn run(&self, _job: &CronJob) -> Result<String, CronError> {
        tokio::time::sleep(std::time::Duration::from_secs(self.0)).await;
        Ok("finished".into())
    }
}

fn due_job() -> CronJob {
    CronJob {
        id: "job-nightly".into(),
        name: "nightly digest".into(),
        schedule: Schedule::Every { seconds: 3_600 },
        prompt: "summarise the day".into(),
        enabled: true,
        last_run_at: None,
        next_run_at: 0.0,
        created_at: 0.0,
        target: None,
    }
}

fn harness(inner: Arc<dyn JobRunner>, dir: &tempfile::TempDir) -> (Scheduler, Arc<JobLedger>) {
    let repo = Arc::new(FsJobRepository::new(dir.path().join("cron")).unwrap());
    repo.save(&[due_job()]).unwrap();
    let ledger = Arc::new(JobLedger::new(Arc::new(Store::open_in_memory().unwrap())));
    let runner = Arc::new(LedgerCronRunner::new(inner, Arc::clone(&ledger)));
    let scheduler = Scheduler::new(repo, runner, SchedulerConfig::default());
    (scheduler, ledger)
}

/// A run the scheduler gives up on must still leave a closed ledger row.
///
/// This is the case that leaked. The scheduler's hard timeout is far shorter
/// than the decorator's own deadline, so in production it is *always* the one
/// that fires — and cancelling a future runs none of the code that closes the
/// row. Every cron run slower than 180s left an attempt stuck at `running`
/// until a lease expiry reclaimed it, and `regent jobs` reported it as live.
#[tokio::test(start_paused = true)]
async fn a_run_the_scheduler_abandons_is_closed_not_left_running() {
    let dir = tempfile::tempdir().unwrap();
    let hard = SchedulerConfig::default().hard_timeout_secs;
    let (scheduler, ledger) = harness(Arc::new(Slow(hard * 10)), &dir);

    let outcomes = scheduler.tick(1.0).await.unwrap();
    assert_eq!(outcomes.len(), 1, "the job was due and ran");

    assert!(
        ledger.live().is_empty(),
        "no attempt is left open: {:?}",
        ledger.live().iter().map(|j| &j.state).collect::<Vec<_>>()
    );
    let note = ledger.updates();
    assert!(
        note.contains("nightly digest") || note.contains("job-nightly"),
        "{note}"
    );
    assert!(
        !note.contains("FINISHED"),
        "an abandoned run is not a success: {note}"
    );
}

/// The ordinary path, through the same composition — a run that beats the
/// deadline is recorded, and recorded without claiming an outcome nobody
/// verified.
#[tokio::test(start_paused = true)]
async fn a_run_that_completes_in_time_is_recorded_with_its_caveat() {
    let dir = tempfile::tempdir().unwrap();
    let (scheduler, ledger) = harness(Arc::new(Slow(1)), &dir);

    scheduler.tick(1.0).await.unwrap();

    assert!(ledger.live().is_empty(), "the attempt is closed");
    let note = ledger.updates();
    assert!(note.contains("finished"), "the report is carried: {note}");
    assert!(
        note.contains("NO VERIFIED RESULT"),
        "nothing on this path validated the outcome: {note}"
    );
}
