//! Ledger behaviour end to end against a real store — the durability and
//! honesty properties the in-memory board could not provide.

use super::*;
use crate::domain::job::{Fact, StopReason};

fn ledger() -> JobLedger {
    JobLedger::new(Arc::new(Store::open_in_memory().unwrap()))
}

/// The report that never came. A job in flight when the process dies is
/// recovered as news, not silently dropped.
#[test]
fn a_job_lost_to_a_restart_is_recovered_and_reported() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let before = JobLedger::new(Arc::clone(&store));
    let (id, created) = before
        .claim(
            "background",
            "build the app",
            "build it",
            JobLimits::default(),
        )
        .unwrap();
    assert!(created);
    before.start(&id, Some("sess-1"));

    // The process dies here. A new ledger opens over the same store.
    let after = JobLedger::new(store);
    // A real restart: the previous process is gone, so nothing is renewing
    // the lease. `recover()` itself would (correctly) leave a job alone that
    // another live deacon is still heartbeating.
    assert_eq!(
        after.recover_abandoned(0.0),
        1,
        "the in-flight job is found"
    );

    let note = after.updates();
    assert!(note.contains("INTERRUPTED"), "{note}");
    assert!(note.contains("build the app"), "{note}");

    // Delivered once: the next turn is not spammed with the same news.
    assert_eq!(after.updates(), "", "delivered news stops repeating");
}

/// Re-firing the same work returns the original job instead of a twin. Twins
/// are what kept reporting "still running" after the real job had finished.
#[test]
fn identical_work_is_one_job() {
    let led = ledger();
    let (first, created) = led
        .claim(
            "background",
            "build the app",
            "build it",
            JobLimits::default(),
        )
        .unwrap();
    assert!(created);
    led.start(&first, None);

    let (second, created_again) = led
        .claim(
            "background",
            "build the app",
            "build it",
            JobLimits::default(),
        )
        .unwrap();
    assert_eq!(second, first, "same work, same job");
    assert!(!created_again);

    assert_eq!(
        led.updates().matches("build the app").count(),
        1,
        "one row in the note, not two"
    );
}

/// Idempotency releases the key on a terminal state — so the SAME work must be
/// claimable again afterwards. Deriving the row id from the key alone collided
/// on the primary key here and wedged a cron schedule permanently after its
/// first failure.
#[test]
fn the_same_work_can_run_again_once_it_has_finished() {
    let led = ledger();
    let (first, _) = led
        .claim("cron", "morning digest", "summarise", JobLimits::default())
        .unwrap();
    led.start(&first, None);
    led.fail(&first, 1, "provider 429");

    let (second, created) = led
        .claim("cron", "morning digest", "summarise", JobLimits::default())
        .unwrap();
    assert!(created, "a finished job does not block the next run");
    assert_ne!(second, first, "and it gets its own row, not a collision");
    assert!(
        led.start(&second, None).is_some(),
        "the new run actually starts"
    );
}

/// The core claim discipline: a process that returns is not a job that
/// succeeded, and the ledger derives the state rather than trusting the caller.
#[test]
fn the_verdict_is_derived_from_evidence_not_asserted() {
    let led = ledger();
    let (id, _) = led
        .claim(
            "background",
            "research",
            "look into it",
            JobLimits::default(),
        )
        .unwrap();
    led.start(&id, Some("sess-1"));

    // It ran to the end. That is ALL we know.
    let mut completion = Completion::unknown();
    completion.process_completed = Fact::Yes;
    led.finish(&id, 1, completion, Some("I had a look."));

    let note = led.updates();
    assert!(note.contains("NO VERIFIED RESULT"), "{note}");
    assert!(
        !note.contains("FINISHED"),
        "a bare return is not success: {note}"
    );
}

#[test]
fn a_dead_process_is_a_failure_with_its_error() {
    let led = ledger();
    let (id, _) = led
        .claim("background", "build", "build it", JobLimits::default())
        .unwrap();
    led.start(&id, None);
    led.fail(&id, 1, "provider returned 429");

    let note = led.updates();
    assert!(note.contains("FAILED"), "{note}");
    assert!(note.contains("provider returned 429"), "{note}");
}

#[test]
fn cancellation_is_observable_by_the_running_job() {
    let led = ledger();
    let (id, _) = led
        .claim("background", "build", "build it", JobLimits::default())
        .unwrap();
    led.start(&id, None);

    assert!(!led.cancel_requested(&id), "not cancelled yet");
    assert!(led.request_cancel(&id));
    assert!(led.cancel_requested(&id), "the runner can see the request");

    led.stop(&id, 1, StopReason::Cancelled, "cancelled by the user");
    let note = led.updates();
    assert!(note.contains("CANCELLED"), "{note}");
    assert!(
        !led.request_cancel(&id),
        "a finished job cannot be cancelled again"
    );
}

/// What `job.list` reads. Until W9 wired it, this ledger could answer the
/// question and nothing asked: job state reached the user only by riding
/// `wrap_prompt` into their next turn, and `request_cancel` had no caller
/// outside the test above — so the runner's poll could never fire in
/// production.
#[test]
fn live_jobs_are_listable_and_show_a_pending_cancellation() {
    let led = ledger();
    let (id, _) = led
        .claim("background", "build", "build it", JobLimits::default())
        .unwrap();
    led.start(&id, None);

    let live = led.live();
    assert_eq!(live.len(), 1, "a running job is listable");
    assert_eq!(live[0].id, id);
    assert_eq!(live[0].label, "build");
    assert!(!live[0].cancel_requested);

    // Cancellation is cooperative: the flag is visible on the listing BEFORE
    // the job notices, which is exactly what the CLI reports as "cancelling".
    assert!(led.request_cancel(&id));
    assert!(led.live()[0].cancel_requested, "shown as cancelling");

    // ...and once it stops, it is no longer live.
    led.stop(&id, 1, StopReason::Cancelled, "cancelled by the user");
    assert!(led.live().is_empty(), "a stopped job leaves the live set");
}

/// A deadline makes a stalled job visible as overdue instead of it sitting in
/// "still running" forever.
#[test]
fn a_deadline_surfaces_an_overdue_job() {
    let led = ledger();
    let (id, _) = led
        .claim(
            "background",
            "wedged job",
            "hang forever",
            JobLimits {
                max_attempts: 1,
                timeout_secs: Some(0),
            },
        )
        .unwrap();
    led.start(&id, None);

    let overdue = led.overdue();
    assert_eq!(overdue.len(), 1, "the deadline has passed");
    assert_eq!(overdue[0].id, id);
    assert!(led.updates().contains("OVERDUE"));
}

/// Artifacts are the evidence behind `artifact_produced`. An empty set is
/// itself a finding — it is what forbids claiming the job produced anything.
#[test]
fn artifacts_are_recorded_against_the_job() {
    let led = ledger();
    let (id, _) = led
        .claim("background", "make the deck", "deck", JobLimits::default())
        .unwrap();
    led.start(&id, None);
    assert!(led.artifacts(&id).is_empty(), "nothing produced yet");

    led.record_artifact(&id, "artifacts/deck.pptx");
    led.record_artifact(&id, "artifacts/notes.md");
    assert_eq!(
        led.artifacts(&id),
        vec!["artifacts/deck.pptx", "artifacts/notes.md"],
        "recorded in order, and retrievable as proof"
    );
}
