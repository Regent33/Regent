//! The job lease (W9). Split from `jobs.rs` (file-size rule).
//!
//! These exist because boot recovery assumed a single deacon and there are
//! several: the CLI spawns one per command, beside the long-lived one the
//! voice server holds.

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

/// The bug this lease exists for, and it is not hypothetical: measured
/// 2026-07-29, a running job flipped to `interrupted` because someone ran
/// `regent status`. Boot recovery interrupted every running row, which is only
/// correct if one deacon can exist — and the CLI spawns one per command beside
/// the long-lived one the voice server holds. The real outcome was then refused
/// as stale by `finish_job`'s attempt guard, so a job that succeeded was
/// reported as interrupted and its result dropped, by a read-only command.
#[test]
fn a_second_process_booting_does_not_steal_a_live_job() {
    let s = store();
    claim_with(&s, "job-1", "build", 1);
    let attempt = s
        .start_job_attempt("job-1", Some("sess-a"))
        .unwrap()
        .unwrap();

    // Another deacon boots while the first is still working on it.
    assert_eq!(
        s.interrupt_running_jobs().unwrap(),
        0,
        "a heartbeating job belongs to whoever is running it"
    );
    assert_eq!(s.job("job-1").unwrap().unwrap().state, "running");

    // The owner is still able to close it, which is the part that was lost:
    // once the row said `interrupted`, the true outcome was refused as stale.
    s.heartbeat_job("job-1", attempt).unwrap();
    assert!(
        s.finish_job(
            "job-1",
            attempt,
            "succeeded",
            ["yes", "yes", "unknown", "unknown"],
            Some("built it"),
            None,
        )
        .unwrap(),
        "the owning attempt can still record its result"
    );
    assert_eq!(s.job("job-1").unwrap().unwrap().state, "succeeded");
}

/// The other half: a lease that has actually expired IS reclaimed, or a job
/// whose process really died would sit `running` forever.
#[test]
fn an_expired_lease_is_reclaimed() {
    let s = store();
    claim_with(&s, "job-1", "build", 1);
    s.start_job_attempt("job-1", Some("sess-a")).unwrap();
    assert_eq!(
        s.interrupt_abandoned_jobs(0.0).unwrap(),
        1,
        "nothing is heartbeating now, so the claim is gone"
    );
    assert_eq!(s.job("job-1").unwrap().unwrap().state, "interrupted");
}
