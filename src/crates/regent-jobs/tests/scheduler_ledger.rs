//! The real composition: `Scheduler` → `LedgerCronRunner` → the work.
//!
//! The unit tests next to the decorator simulate the scheduler with a bare
//! `tokio::time::timeout`, which is only as good as my reading of what the
//! scheduler does. This drives the actual `Scheduler` over a real job file and
//! — where it matters — a real on-disk database, so the claims are tested
//! rather than assumed.

use async_trait::async_trait;
use regent_cron::{
    CronError, CronJob, FsJobRepository, JobRepository, JobRunner, Schedule, Scheduler,
    SchedulerConfig,
};
use regent_jobs::{CRON_BUDGET_SECS, CRON_WATCHDOG_SECS, JobLedger, LedgerCronRunner};
use regent_store::Store;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Slow(u64);

#[async_trait]
impl JobRunner for Slow {
    async fn run(&self, _job: &CronJob) -> Result<String, CronError> {
        tokio::time::sleep(std::time::Duration::from_secs(self.0)).await;
        Ok("finished".into())
    }
}

/// Counts its runs, so "did this execute twice" is answerable.
struct Counting(AtomicUsize);

#[async_trait]
impl JobRunner for Counting {
    async fn run(&self, _job: &CronJob) -> Result<String, CronError> {
        let n = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(format!("run {n}"))
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

/// The production pairing: the run owns the deadline, the scheduler is the
/// watchdog above it.
fn config() -> SchedulerConfig {
    SchedulerConfig {
        hard_timeout_secs: CRON_WATCHDOG_SECS,
        ..SchedulerConfig::default()
    }
}

fn scheduler_over(
    inner: Arc<dyn JobRunner>,
    ledger: &Arc<JobLedger>,
    dir: &tempfile::TempDir,
) -> Scheduler {
    let repo = Arc::new(FsJobRepository::new(dir.path().join("cron")).unwrap());
    repo.save(&[due_job()]).unwrap();
    let runner =
        Arc::new(LedgerCronRunner::new(inner, Arc::clone(ledger)).with_budget(CRON_BUDGET_SECS));
    Scheduler::new(repo, runner, config())
}

fn memory_ledger() -> Arc<JobLedger> {
    Arc::new(JobLedger::new(Arc::new(Store::open_in_memory().unwrap())))
}

/// A run that outlives its budget is closed BY ITS OWN DEADLINE, so the ledger
/// records `timed out` rather than the generic interruption a cancelled future
/// leaves behind.
///
/// Before the budget was passed in, the scheduler's watchdog was the shorter of
/// the two and always won — it dropped the run mid-await, no arm of the match
/// ran, and the attempt sat at `running` until a lease expiry reclaimed it.
#[tokio::test(start_paused = true)]
async fn a_run_that_outlives_its_budget_is_recorded_as_timed_out() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = memory_ledger();
    let scheduler = scheduler_over(Arc::new(Slow(CRON_BUDGET_SECS * 10)), &ledger, &dir);

    let outcomes = scheduler.tick(1.0).await.unwrap();
    assert_eq!(outcomes.len(), 1, "the job was due and ran");

    assert!(
        ledger.live().is_empty(),
        "no attempt is left open: {:?}",
        ledger.live().iter().map(|j| &j.state).collect::<Vec<_>>()
    );
    let note = ledger.updates();
    assert!(note.contains("deadline"), "the cause is recorded: {note}");
    assert!(
        !note.contains("FINISHED"),
        "a run nobody let finish is not a success: {note}"
    );
}

/// The same run under a caller whose own timeout is SHORTER than the budget —
/// the shape the guard exists for, now that the ordinary timeout path closes
/// its own row. The attempt must still be closed.
#[tokio::test(start_paused = true)]
async fn a_run_cancelled_from_outside_is_still_closed() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = memory_ledger();
    let scheduler = scheduler_over(Arc::new(Slow(CRON_BUDGET_SECS * 10)), &ledger, &dir);

    // Something above the scheduler gives up first — a shutdown, or a caller
    // with a tighter budget of its own.
    let cut_short = tokio::time::timeout(
        std::time::Duration::from_secs(CRON_BUDGET_SECS / 2),
        scheduler.tick(1.0),
    )
    .await;
    assert!(cut_short.is_err(), "the outer timeout won");

    assert!(
        ledger.live().is_empty(),
        "the guard closed it: {:?}",
        ledger.live().iter().map(|j| &j.state).collect::<Vec<_>>()
    );
    assert!(!ledger.updates().contains("FINISHED"));
}

/// The ordinary path — a run that beats the deadline is recorded, and recorded
/// without claiming an outcome nobody verified.
#[tokio::test(start_paused = true)]
async fn a_run_that_completes_in_time_is_recorded_with_its_caveat() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = memory_ledger();
    let scheduler = scheduler_over(Arc::new(Slow(1)), &ledger, &dir);

    scheduler.tick(1.0).await.unwrap();

    assert!(ledger.live().is_empty(), "the attempt is closed");
    let note = ledger.updates();
    assert!(note.contains("finished"), "the report is carried: {note}");
    assert!(
        note.contains("NO VERIFIED RESULT"),
        "nothing on this path validated the outcome: {note}"
    );
}

/// **Recurring cron must stay recurring.**
///
/// The ledger's idempotency index is what stops a tick stacking a second copy
/// of a running job. If it spanned terminal rows too, the first execution would
/// consume the key forever and every later occurrence would be refused as "the
/// previous execution is still running" — a recurring job silently demoted to a
/// one-shot. The index is partial (`WHERE state IN ('queued','running')`), and
/// this is the test that says so out loud.
#[tokio::test(start_paused = true)]
async fn a_recurring_job_runs_again_on_its_next_occurrence() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = memory_ledger();
    let counter = Arc::new(Counting(AtomicUsize::new(0)));
    let scheduler = scheduler_over(Arc::clone(&counter) as Arc<dyn JobRunner>, &ledger, &dir);

    scheduler.tick(1.0).await.unwrap();
    // Just past the next hourly slot. Not *far* past: beyond the catch-up
    // window the scheduler skips forward without running, and a skipped tick
    // would let this test pass while proving nothing.
    let outcomes = scheduler.tick(3_700.0).await.unwrap();

    assert_eq!(outcomes.len(), 1, "the second occurrence was due");
    assert_eq!(
        outcomes[0].status,
        regent_cron::RunStatus::Ok,
        "it actually ran rather than being skipped forward: {}",
        outcomes[0].summary
    );
    assert_eq!(
        counter.0.load(Ordering::SeqCst),
        2,
        "the work ran twice, not once — the terminal row released the key"
    );
    let note = ledger.updates();
    assert!(
        note.contains("run 2"),
        "the second execution is on the ledger: {note}"
    );
}

/// The gateway and the deacon are separate processes over ONE `state.db`. The
/// in-memory store used above cannot show that, so this opens the same file
/// twice — two `Store`s, two ledgers, as the two processes really are.
#[tokio::test(start_paused = true)]
async fn two_processes_over_one_database_see_each_other_s_executions() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.db");

    // The deacon's side runs a job to completion.
    let deacon = Arc::new(JobLedger::new(Arc::new(Store::open(&db).unwrap())));
    let scheduler = scheduler_over(Arc::new(Slow(1)), &deacon, &dir);
    scheduler.tick(1.0).await.unwrap();

    // The gateway opens the same file and must see it.
    let gateway = JobLedger::new(Arc::new(Store::open(&db).unwrap()));
    let note = gateway.updates();
    assert!(
        note.contains("nightly digest") || note.contains("finished"),
        "a second process over the same db sees the execution: {note}"
    );
    assert!(
        gateway.live().is_empty(),
        "and sees it as closed, not as somebody else's live work"
    );
}
