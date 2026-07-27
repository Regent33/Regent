//! What the model is told about finished work. These assertions are about
//! wording on purpose: the note IS the interface to the model, and the outage
//! it prevents was the model relaying "done" for work that never happened.

use super::*;
use regent_store::now_epoch;

fn job(label: &str, state: &str) -> JobRow {
    let now = now_epoch();
    JobRow {
        id: format!("job-{label}"),
        kind: "background".into(),
        label: label.into(),
        task: "do the thing".into(),
        idempotency_key: format!("background:{label}"),
        state: state.into(),
        session_id: Some("sess-1".into()),
        attempts: 1,
        max_attempts: 1,
        deadline_at: None,
        cancel_requested: false,
        process_completed: "unknown".into(),
        artifact_produced: "unknown".into(),
        result_validated: "unknown".into(),
        outcome_achieved: "unknown".into(),
        result: None,
        error: None,
        delivered_at: None,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn nothing_to_report_renders_nothing() {
    assert_eq!(render_updates(&[], &[]), "");
}

/// The restart case, in the words the user eventually sees. An interrupted job
/// must not read as either finished or failed.
#[test]
fn an_interrupted_job_is_never_dressed_up() {
    let mut interrupted = job("build the app", "interrupted");
    interrupted.process_completed = "no".into();

    let note = render_updates(&[interrupted], &[]);
    assert!(note.contains("INTERRUPTED"), "{note}");
    assert!(note.contains("cut off by a restart"), "{note}");
    assert!(
        note.contains("do not imply it is done"),
        "the instruction has to be explicit: {note}"
    );
    assert!(
        note.contains("offer to start it again"),
        "the user is owed a way forward: {note}"
    );
    assert!(!note.contains("FINISHED"), "{note}");
}

/// A job that ran to the end but proved nothing gets no success language.
#[test]
fn an_unverified_outcome_is_reported_as_unverified() {
    let mut ran = job("research the market", "inconclusive");
    ran.process_completed = "yes".into();
    ran.result = Some("I looked into it.".into());

    let note = render_updates(&[ran], &[]);
    assert!(note.contains("NO VERIFIED RESULT"), "{note}");
    assert!(note.contains("Caveat:"), "{note}");
    assert!(
        note.contains("unverified"),
        "the doubt travels with the result: {note}"
    );
    assert!(note.contains("I looked into it."), "{note}");
}

/// A self-reported success still says it was not independently checked.
#[test]
fn a_self_reported_success_carries_its_caveat() {
    let mut done = job("make the deck", "succeeded");
    done.process_completed = "yes".into();
    done.artifact_produced = "yes".into();
    done.outcome_achieved = "yes".into();
    done.result = Some("Deck at artifacts/deck.pptx".into());

    let note = render_updates(&[done.clone()], &[]);
    assert!(note.contains("FINISHED: make the deck"), "{note}");
    assert!(note.contains("not independently verified"), "{note}");

    // Validated: the one case that claims cleanly, with no hedge attached.
    let mut verified = done;
    verified.result_validated = "yes".into();
    let note = render_updates(&[verified], &[]);
    assert!(note.contains("FINISHED"), "{note}");
    assert!(
        !note.contains("Caveat:"),
        "a proven result needs no hedge: {note}"
    );
}

#[test]
fn a_failure_reports_its_error() {
    let mut failed = job("build the app", "failed");
    failed.process_completed = "no".into();
    failed.outcome_achieved = "no".into();
    failed.error = Some("provider returned 429".into());

    let note = render_updates(&[failed], &[]);
    assert!(note.contains("FAILED"), "{note}");
    assert!(note.contains("provider returned 429"), "{note}");
}

#[test]
fn a_live_job_shows_its_age_and_flags_an_overdue_one() {
    let mut running = job("build the app", "running");
    running.created_at = now_epoch() - 300.0;
    let note = render_updates(&[], &[running.clone()]);
    assert!(note.contains("STILL RUNNING (5m)"), "{note}");

    let mut overdue = running;
    overdue.deadline_at = Some(now_epoch() - 1.0);
    let note = render_updates(&[], &[overdue]);
    assert!(note.contains("OVERDUE"), "{note}");
    assert!(note.contains("past its deadline"), "{note}");
}
