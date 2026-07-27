//! Cron on the shared ledger. The property that matters most is overlap: a
//! tick firing while the previous execution is still going must not start a
//! second copy of the same job.

use super::*;
use regent_cron::{Schedule, SchedulerConfig};
use regent_store::Store;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scripted {
    runs: AtomicUsize,
    reply: Mutex<Result<String, String>>,
}

#[async_trait]
impl JobRunner for Scripted {
    async fn run(&self, _job: &CronJob) -> Result<String, CronError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let reply = self.reply.lock().unwrap().clone();
        match reply {
            Ok(text) => Ok(text),
            Err(e) => Err(CronError::RunFailed(e)),
        }
    }
}

fn scripted(reply: Result<String, String>) -> Arc<Scripted> {
    Arc::new(Scripted {
        runs: AtomicUsize::new(0),
        reply: Mutex::new(reply),
    })
}

fn cron_job() -> CronJob {
    CronJob {
        id: "job-abc".into(),
        name: "morning digest".into(),
        schedule: Schedule::Every { seconds: 60 },
        prompt: "summarise the news".into(),
        enabled: true,
        last_run_at: None,
        next_run_at: 0.0,
        created_at: 0.0,
        target: None,
    }
}

fn ledger() -> Arc<JobLedger> {
    Arc::new(JobLedger::new(Arc::new(Store::open_in_memory().unwrap())))
}

/// A cron run that returns is a completed PROCESS. Nothing on this path checks
/// what it produced, so the outcome stays unproven and the report says so.
///
/// An earlier draft treated any non-empty reply as success, which made a job
/// returning "I couldn't reach the API" a success — the original defect,
/// reintroduced for cron. Caught in co-review.
#[tokio::test]
async fn a_run_that_returns_is_recorded_but_not_claimed_as_success() {
    let led = ledger();
    let runner = LedgerCronRunner::new(scripted(Ok("3 headlines".into())), Arc::clone(&led));

    let out = runner.run(&cron_job()).await.unwrap();
    assert_eq!(out, "3 headlines", "the report passes through untouched");

    let note = led.updates();
    assert!(note.contains("NO VERIFIED RESULT"), "{note}");
    assert!(
        note.contains("3 headlines"),
        "the report is still shown: {note}"
    );
    assert!(
        !note.contains("FINISHED"),
        "but it is not called finished: {note}"
    );
}

/// The exact laundering case, named so it cannot come back.
#[tokio::test]
async fn a_run_reporting_its_own_failure_is_not_a_success() {
    let led = ledger();
    let runner = LedgerCronRunner::new(
        scripted(Ok(
            "I couldn't reach the API, so there is no digest today.".into()
        )),
        Arc::clone(&led),
    );
    runner.run(&cron_job()).await.unwrap();

    let note = led.updates();
    assert!(!note.contains("FINISHED"), "{note}");
    assert!(note.contains("NO VERIFIED RESULT"), "{note}");
}

/// The overlap guard. Without it a slow job fires again every tick and stacks
/// executions — the cron analogue of the background twins.
#[tokio::test]
async fn an_overlapping_tick_does_not_start_a_second_execution() {
    let led = ledger();
    let job = cron_job();

    // An execution is already in flight — claimed and started, not yet
    // finished. This is exactly the state a slow job sits in when the next
    // tick fires.
    let (id, created) = led
        .claim("cron", &job.id, &job.prompt, JobLimits::default())
        .unwrap();
    assert!(created);
    assert!(led.start(&id, None).is_some());

    let inner = scripted(Ok("done".into()));
    let runner = LedgerCronRunner::new(Arc::clone(&inner) as Arc<dyn JobRunner>, Arc::clone(&led));

    let second = runner.run(&job).await;
    assert!(second.is_err(), "the overlapping tick is refused");
    assert!(
        second.unwrap_err().to_string().contains("still running"),
        "and it says why"
    );
    assert_eq!(
        inner.runs.load(Ordering::SeqCst),
        0,
        "the underlying job was never invoked a second time"
    );
}

#[tokio::test]
async fn a_failed_run_records_the_error_and_frees_the_key() {
    let led = ledger();
    let runner = LedgerCronRunner::new(scripted(Err("provider 429".into())), Arc::clone(&led));

    assert!(runner.run(&cron_job()).await.is_err());
    let note = led.updates();
    assert!(note.contains("FAILED"), "{note}");
    assert!(note.contains("provider 429"), "{note}");

    // The key is free again, so the next scheduled tick can run.
    let ok = LedgerCronRunner::new(scripted(Ok("recovered".into())), Arc::clone(&led));
    assert!(
        ok.run(&cron_job()).await.is_ok(),
        "a finished failure does not wedge the schedule"
    );
}

/// The deadline is enforced, not merely recorded. Without this a wedged run
/// held its idempotency key forever and silently suppressed every later tick.
#[tokio::test(start_paused = true)]
async fn a_wedged_run_is_stopped_at_its_deadline_and_frees_the_schedule() {
    struct Wedged;
    #[async_trait]
    impl JobRunner for Wedged {
        async fn run(&self, _job: &CronJob) -> Result<String, CronError> {
            // Longer than any cron deadline; the timeout must win.
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60 * 24)).await;
            Ok("never".into())
        }
    }

    let led = ledger();
    let runner = LedgerCronRunner::new(Arc::new(Wedged), Arc::clone(&led));
    let err = runner.run(&cron_job()).await.unwrap_err();
    assert!(err.to_string().contains("deadline"), "{err}");

    let note = led.updates();
    assert!(note.contains("deadline"), "the user is told why: {note}");
    assert!(!note.contains("FINISHED"), "{note}");

    // The key is released, so the next tick is not blocked forever.
    let ok = LedgerCronRunner::new(scripted(Ok("recovered".into())), Arc::clone(&led));
    assert!(
        ok.run(&cron_job()).await.is_ok(),
        "a timed-out run must not wedge the schedule"
    );
}

/// Guards the ownership rule this decorator must not disturb: schedulers only
/// run jobs they own, so the ledger sees one process's executions, not two.
#[test]
fn scheduler_ownership_is_unchanged_by_the_decorator() {
    let local = SchedulerConfig::default();
    let mut job = cron_job();
    assert!(local.owns(&job), "a local job stays local");
    job.target = Some("telegram:123".into());
    assert!(!local.owns(&job), "a chat-owned job is not ours to run");
}
