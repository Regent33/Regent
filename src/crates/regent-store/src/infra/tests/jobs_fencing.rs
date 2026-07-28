//! Job-ledger concurrency and constraint tests: fencing, attempt budgets, the
//! write-once evidence pointer, and the CHECK constraints. Split from
//! `jobs.rs` for the file-size rule.

use super::*;

fn store() -> Store {
    Store::open_in_memory().unwrap()
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

fn claim(s: &Store, id: &str, key: &str) -> (String, bool) {
    claim_with(s, id, key, 1)
}

/// Fencing. A worker that was declared interrupted must not be able to return
/// late and overwrite the attempt that replaced it — that would report a stale
/// `succeeded` over work another attempt is still doing.
#[test]
fn a_stale_worker_cannot_close_a_newer_attempt() {
    let s = store();
    claim_with(&s, "job-1", "build", 3);
    let first = s
        .start_job_attempt("job-1", Some("sess-a"))
        .unwrap()
        .unwrap();

    // A restart interrupts it and a second attempt takes over.
    s.interrupt_abandoned_jobs(0.0).unwrap();
    let second = s
        .start_job_attempt("job-1", Some("sess-b"))
        .unwrap()
        .unwrap();
    assert_eq!((first, second), (1, 2));

    // Attempt 1 finally returns. It is refused.
    assert!(
        !s.finish_job("job-1", first, "succeeded", ["yes"; 4], Some("stale"), None)
            .unwrap(),
        "the stale attempt must not be able to close the job"
    );
    let job = s.job("job-1").unwrap().unwrap();
    assert_eq!(job.state, "running", "attempt 2 still owns it");
    assert!(job.result.is_none(), "and its result was not written");

    // Attempt 2 closes normally.
    assert!(
        s.finish_job("job-1", second, "succeeded", ["yes"; 4], Some("real"), None)
            .unwrap()
    );
    assert_eq!(
        s.job("job-1").unwrap().unwrap().result.as_deref(),
        Some("real")
    );
}

/// `max_attempts` is enforced by the DB, not trusted to a caller.
#[test]
fn max_attempts_is_enforced_on_start() {
    let s = store();
    claim_with(&s, "job-1", "build", 2);

    assert_eq!(s.start_job_attempt("job-1", None).unwrap(), Some(1));
    s.interrupt_abandoned_jobs(0.0).unwrap();
    assert_eq!(s.start_job_attempt("job-1", None).unwrap(), Some(2));
    s.interrupt_abandoned_jobs(0.0).unwrap();
    assert_eq!(
        s.start_job_attempt("job-1", None).unwrap(),
        None,
        "the third start is refused — the budget is spent"
    );
}

/// The evidence pointer is set once and never overwritten by a retry.
#[test]
fn the_session_pointer_is_write_once() {
    let s = store();
    claim_with(&s, "job-1", "build", 2);
    s.start_job_attempt("job-1", None).unwrap();

    s.attach_job_session("job-1", "sess-a").unwrap();
    s.attach_job_session("job-1", "sess-b").unwrap();
    assert_eq!(
        s.job("job-1").unwrap().unwrap().session_id.as_deref(),
        Some("sess-a"),
        "a later attach must not erase the first transcript"
    );
    assert_eq!(
        s.job_attempts("job-1").unwrap()[0].session_id.as_deref(),
        Some("sess-a")
    );
}

/// The schema refuses states and facts outside the state machine, so a bad
/// write surfaces instead of being silently parsed back as `queued`/`unknown`.
#[test]
fn the_database_rejects_states_and_facts_outside_the_machine() {
    let s = store();
    claim(&s, "job-1", "build");
    s.start_job_attempt("job-1", None).unwrap();

    assert!(
        s.finish_job("job-1", 1, "definitely-done", ["yes"; 4], None, None)
            .is_err(),
        "an invented state must be refused by the CHECK constraint"
    );
    assert!(
        s.finish_job(
            "job-1",
            1,
            "succeeded",
            ["probably", "yes", "yes", "yes"],
            None,
            None
        )
        .is_err(),
        "an invented fact value must be refused too"
    );
}
