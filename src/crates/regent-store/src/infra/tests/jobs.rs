//! Job-ledger persistence tests. The invariants here are the ones that failed
//! in production as an in-memory board: twins for the same work, and jobs that
//! vanished on restart while the user waited for a report.

use super::*;

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn claim(s: &Store, id: &str, key: &str) -> (String, bool) {
    claim_with(s, id, key, 1)
}

fn claim_with(s: &Store, id: &str, key: &str, max_attempts: i64) -> (String, bool) {
    s.claim_job(
        id,
        "background",
        "build the app",
        "do it",
        key,
        max_attempts,
        None,
    )
    .unwrap()
}

/// The doom-loop shape: the model re-fires the same work while it is running.
/// A second row is not created, and the caller is handed the original id.
#[test]
fn a_live_job_holds_its_idempotency_key() {
    let s = store();
    let (first, created) = claim(&s, "job-1", "build");
    assert_eq!(first, "job-1");
    assert!(created, "first claim creates");

    let (second, created_again) = claim(&s, "job-2", "build");
    assert_eq!(second, "job-1", "re-firing returns the original job");
    assert!(!created_again, "no twin is created");
    assert_eq!(s.live_jobs().unwrap().len(), 1, "one row, not two");

    // Once it reaches a terminal state the key is free — asking for the same
    // work again later is a NEW job, not a resurrection of the old one.
    s.start_job_attempt("job-1", Some("sess-a")).unwrap();
    s.finish_job("job-1", 1, "succeeded", ["yes"; 4], Some("done"), None)
        .unwrap();
    let (third, created_third) = claim(&s, "job-3", "build");
    assert_eq!(third, "job-3", "a finished key can be claimed again");
    assert!(created_third);
}

/// Two workers racing for the same job: exactly one starts it.
#[test]
fn starting_an_attempt_is_exclusive() {
    let s = store();
    claim(&s, "job-1", "build");
    assert_eq!(
        s.start_job_attempt("job-1", Some("sess-a")).unwrap(),
        Some(1),
        "first worker gets attempt 1"
    );
    assert_eq!(
        s.start_job_attempt("job-1", Some("sess-b")).unwrap(),
        None,
        "a running job cannot be started twice"
    );
    let job = s.job("job-1").unwrap().unwrap();
    assert_eq!(job.state, "running");
    assert_eq!(
        job.session_id.as_deref(),
        Some("sess-a"),
        "the transcript pointer is the evidence trail"
    );
}

/// The restart case. A job left running is `interrupted` — never `succeeded`,
/// never `failed`, because the process died without producing either answer.
#[test]
fn a_restart_interrupts_rather_than_guessing() {
    let s = store();
    claim_with(&s, "job-1", "build", 2); // room for the retry below
    s.start_job_attempt("job-1", Some("sess-a")).unwrap();

    assert_eq!(s.interrupt_abandoned_jobs(0.0).unwrap(), 1);

    let job = s.job("job-1").unwrap().unwrap();
    assert_eq!(job.state, "interrupted");
    assert_eq!(job.process_completed, "no", "the process did NOT complete");
    assert_eq!(
        job.outcome_achieved, "unknown",
        "and we cannot claim to know whether the work landed"
    );
    assert!(
        job.delivered_at.is_none(),
        "still owed to the user — this is the report that never came"
    );
    assert_eq!(
        s.undelivered_jobs().unwrap().len(),
        1,
        "an interrupted job is deliverable news, not a dropped row"
    );

    // The attempt is closed too, so attempt history does not show it open forever.
    let attempts = s.job_attempts("job-1").unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome.as_deref(), Some("interrupted"));
    assert!(attempts[0].ended_at.is_some());

    // An interrupted job is retryable: attempt 2 opens without a new row.
    assert_eq!(
        s.start_job_attempt("job-1", Some("sess-b")).unwrap(),
        Some(2)
    );
    assert_eq!(s.job_attempts("job-1").unwrap().len(), 2, "history is kept");
}

/// A retry must not overwrite what happened the first time.
#[test]
fn attempt_history_survives_a_retry() {
    let s = store();
    claim(&s, "job-1", "build");
    s.start_job_attempt("job-1", Some("sess-a")).unwrap();
    s.finish_job(
        "job-1",
        1,
        "failed",
        ["yes", "no", "no", "no"],
        None,
        Some("provider 429"),
    )
    .unwrap();

    let job = s.job("job-1").unwrap().unwrap();
    assert_eq!(job.state, "failed");
    assert_eq!(job.error.as_deref(), Some("provider 429"));

    let attempts = s.job_attempts("job-1").unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].error.as_deref(), Some("provider 429"));
    assert_eq!(attempts[0].session_id.as_deref(), Some("sess-a"));
}

/// The four facts are stored independently. A job can complete its process and
/// still be unable to claim the outcome — that pairing is the whole point.
#[test]
fn completion_is_four_facts_not_one_boolean() {
    let s = store();
    claim(&s, "job-1", "build");
    s.start_job_attempt("job-1", None).unwrap();
    s.finish_job(
        "job-1",
        1,
        "inconclusive",
        ["yes", "no", "unknown", "unknown"],
        Some("ran, produced nothing I can point at"),
        None,
    )
    .unwrap();

    let job = s.job("job-1").unwrap().unwrap();
    assert_eq!(job.state, "inconclusive", "a legal terminal state");
    assert_eq!(job.process_completed, "yes");
    assert_eq!(job.artifact_produced, "no");
    assert_eq!(job.result_validated, "unknown");
    assert_eq!(job.outcome_achieved, "unknown");
}

#[test]
fn artifacts_and_cancellation_are_recorded() {
    let s = store();
    claim(&s, "job-1", "build");
    s.start_job_attempt("job-1", None).unwrap();

    s.record_job_artifact("job-1", "artifacts/deck.pptx")
        .unwrap();
    s.record_job_artifact("job-1", "artifacts/notes.md")
        .unwrap();
    assert_eq!(
        s.job_artifacts("job-1").unwrap(),
        vec!["artifacts/deck.pptx", "artifacts/notes.md"]
    );

    assert!(s.request_job_cancel("job-1").unwrap(), "a live job cancels");
    assert!(s.job("job-1").unwrap().unwrap().cancel_requested);

    s.finish_job(
        "job-1",
        1,
        "cancelled",
        ["no", "yes", "no", "no"],
        None,
        None,
    )
    .unwrap();
    assert!(
        !s.request_job_cancel("job-1").unwrap(),
        "a finished job cannot be cancelled"
    );
}

/// Delivery is tracked so a result is relayed once, not on every turn forever.
#[test]
fn a_result_is_delivered_once() {
    let s = store();
    claim(&s, "job-1", "build");
    s.start_job_attempt("job-1", None).unwrap();
    s.finish_job("job-1", 1, "succeeded", ["yes"; 4], Some("built"), None)
        .unwrap();

    assert_eq!(s.undelivered_jobs().unwrap().len(), 1);
    s.mark_job_delivered("job-1").unwrap();
    assert!(
        s.undelivered_jobs().unwrap().is_empty(),
        "delivered results stop repeating"
    );
    assert!(
        s.job("job-1").unwrap().unwrap().result.is_some(),
        "but the record itself is kept"
    );
}
