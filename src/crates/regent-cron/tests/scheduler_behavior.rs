//! Scheduler hardening contract: tick lock exclusion, hard timeout,
//! catch-up clamp, one-shot retirement — against the real fs repository.

use async_trait::async_trait;
use regent_cron::{
    CronError, CronJob, FsJobRepository, JobRepository, JobRunner, RunStatus, Schedule, Scheduler,
    SchedulerConfig,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

struct CountingRunner {
    runs: AtomicU32,
    sleep_secs: u64,
    fail: bool,
}

impl CountingRunner {
    fn quick() -> Arc<Self> {
        Arc::new(Self {
            runs: AtomicU32::new(0),
            sleep_secs: 0,
            fail: false,
        })
    }

    fn slow(sleep_secs: u64) -> Arc<Self> {
        Arc::new(Self {
            runs: AtomicU32::new(0),
            sleep_secs,
            fail: false,
        })
    }

    fn failing() -> Arc<Self> {
        Arc::new(Self {
            runs: AtomicU32::new(0),
            sleep_secs: 0,
            fail: true,
        })
    }

    fn count(&self) -> u32 {
        self.runs.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl JobRunner for CountingRunner {
    async fn run(&self, job: &CronJob) -> Result<String, CronError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        if self.sleep_secs > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(self.sleep_secs)).await;
        }
        if self.fail {
            return Err(CronError::RunFailed("synthetic failure".into()));
        }
        Ok(format!("ran {}", job.name))
    }
}

fn config_with_timeout(secs: u64) -> SchedulerConfig {
    SchedulerConfig {
        hard_timeout_secs: secs,
        ..SchedulerConfig::default()
    }
}

fn seed_job(repo: &FsJobRepository, schedule: Schedule, now: f64) -> CronJob {
    let job = CronJob::new("test-job", schedule, "do the thing", now).unwrap();
    repo.save(std::slice::from_ref(&job)).unwrap();
    job
}

#[tokio::test]
async fn due_job_fires_once_and_is_blocked_by_the_tick_lock() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Arc::new(FsJobRepository::new(dir.path()).unwrap());
    let runner = CountingRunner::quick();
    seed_job(&repo, Schedule::Every { seconds: 60 }, 0.0);
    let scheduler = Scheduler::new(repo.clone(), runner.clone(), config_with_timeout(5));

    // While another process holds the tick lock, nothing runs.
    let foreign_lock = repo.try_lock_tick().unwrap().expect("lock acquirable");
    assert!(scheduler.tick(61.0).await.unwrap().is_empty());
    assert_eq!(runner.count(), 0);
    drop(foreign_lock);

    // Lock released → due job fires exactly once and reschedules.
    let outcomes = scheduler.tick(61.0).await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].status, RunStatus::Ok);
    assert_eq!(runner.count(), 1);
    let saved = repo.load().unwrap();
    assert_eq!(saved[0].next_run_at, 121.0);
    assert_eq!(saved[0].last_run_at, Some(61.0));

    // Same instant again → not due anymore.
    assert!(scheduler.tick(61.0).await.unwrap().is_empty());
    assert_eq!(runner.count(), 1);
}

#[tokio::test]
async fn hard_timeout_aborts_runaway_runs() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Arc::new(FsJobRepository::new(dir.path()).unwrap());
    let runner = CountingRunner::slow(30);
    seed_job(&repo, Schedule::Every { seconds: 60 }, 0.0);
    let scheduler = Scheduler::new(repo.clone(), runner.clone(), config_with_timeout(1));

    let started = std::time::Instant::now();
    let outcomes = scheduler.tick(61.0).await.unwrap();
    assert!(
        started.elapsed().as_secs() < 10,
        "timeout must abort the run"
    );
    assert_eq!(outcomes[0].status, RunStatus::TimedOut);
    // The job still advances — a stuck job cannot wedge the schedule.
    assert_eq!(repo.load().unwrap()[0].next_run_at, 121.0);
}

#[tokio::test]
async fn missed_beyond_catchup_window_skips_forward_without_running() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Arc::new(FsJobRepository::new(dir.path()).unwrap());
    let runner = CountingRunner::quick();
    // 60s period → window clamps to min 120s. 10_000s late ≫ window.
    seed_job(&repo, Schedule::Every { seconds: 60 }, 0.0);
    let scheduler = Scheduler::new(repo.clone(), runner.clone(), config_with_timeout(5));

    let outcomes = scheduler.tick(10_060.0).await.unwrap();
    assert_eq!(outcomes[0].status, RunStatus::SkippedCatchup);
    assert_eq!(runner.count(), 0, "must not run a long-missed job");
    assert_eq!(repo.load().unwrap()[0].next_run_at, 10_120.0);
}

#[tokio::test]
async fn corrupt_jobs_file_recovers_from_bak_instead_of_erroring() {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsJobRepository::new(dir.path()).unwrap();
    let job = seed_job(&repo, Schedule::Every { seconds: 60 }, 0.0);
    // Second save keeps the first good file as jobs.json.bak.
    repo.save(std::slice::from_ref(&job)).unwrap();

    std::fs::write(dir.path().join("jobs.json"), "{ torn wri").unwrap();
    let recovered = repo.load().expect("corrupt jobs.json must not error");
    assert_eq!(recovered.len(), 1, "recovered from .bak");
    assert_eq!(recovered[0].id, job.id);

    // Corrupt file AND corrupt/missing .bak → empty, still never an Err.
    std::fs::write(dir.path().join("jobs.json.bak"), "also bad").unwrap();
    assert!(repo.load().unwrap().is_empty());
}

/// A `cron add` landing while a tick is mid-run must survive the tick's
/// final persist (the tick merges its changes; it doesn't blind-save).
struct AddingRunner {
    repo: Arc<FsJobRepository>,
}

#[async_trait]
impl JobRunner for AddingRunner {
    async fn run(&self, job: &CronJob) -> Result<String, CronError> {
        let new = CronJob::new(
            "added-mid-tick",
            Schedule::Every { seconds: 3600 },
            "hi",
            61.0,
        )
        .unwrap();
        self.repo
            .mutate(&mut |jobs| jobs.push(new.clone()))
            .unwrap();
        Ok(format!("ran {}", job.name))
    }
}

#[tokio::test]
async fn concurrent_add_during_tick_is_not_lost() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Arc::new(FsJobRepository::new(dir.path()).unwrap());
    let seeded = seed_job(&repo, Schedule::Every { seconds: 60 }, 0.0);
    let runner = Arc::new(AddingRunner {
        repo: Arc::clone(&repo),
    });
    let scheduler = Scheduler::new(repo.clone(), runner, config_with_timeout(5));

    scheduler.tick(61.0).await.unwrap();

    let saved = repo.load().unwrap();
    assert_eq!(saved.len(), 2, "both the tick's update and the add persist");
    let ran = saved.iter().find(|j| j.id == seeded.id).unwrap();
    assert_eq!(ran.next_run_at, 121.0, "tick's reschedule persisted");
    assert!(
        saved.iter().any(|j| j.name == "added-mid-tick"),
        "concurrent add persisted"
    );
}

#[tokio::test]
async fn one_shot_runs_once_then_retires_and_failures_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Arc::new(FsJobRepository::new(dir.path()).unwrap());
    let runner = CountingRunner::failing();
    seed_job(&repo, Schedule::OneShot { at_epoch: 100.0 }, 0.0);
    let scheduler = Scheduler::new(repo.clone(), runner.clone(), config_with_timeout(5));

    // Within the 120s grace → runs (and reports the failure as an outcome).
    let outcomes = scheduler.tick(150.0).await.unwrap();
    assert_eq!(outcomes[0].status, RunStatus::Failed);
    assert_eq!(runner.count(), 1);
    let saved = repo.load().unwrap();
    assert!(!saved[0].enabled, "one-shot retires after firing");

    // Retired job never fires again.
    assert!(scheduler.tick(300.0).await.unwrap().is_empty());
    assert_eq!(runner.count(), 1);
}
