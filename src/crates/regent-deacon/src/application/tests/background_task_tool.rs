//! The claim discipline at the tool boundary: what a finished detached run is
//! allowed to assert, and that a job update never silently vanishes.

use super::*;
// `assess`/`take_slot` moved to the runner module in the file-size split.
use crate::application::background_task_run::{assess, take_slot};
use crate::application::jobs::JobLedger;
use crate::domain::job::Fact;
use crate::domain::job::JobState;
use regent_store::Store;

/// W9's concurrency cap. Nothing bounded this before: the idempotency key stops
/// the *same* job twice, not six different ones, so a model emitting a batch of
/// parallel `background_task` calls fanned out unboundedly — each one a full
/// agent session against the same provider quota.
#[test]
fn only_max_concurrent_jobs_get_a_slot_and_a_refusal_says_so() {
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_JOBS));
    let held: Vec<_> = (0..MAX_CONCURRENT_JOBS)
        .map(|i| take_slot(&slots).unwrap_or_else(|_| panic!("slot {i} must be available")))
        .collect();

    let refusal = take_slot(&slots).expect_err("the cap must bite");
    let parsed: serde_json::Value = serde_json::from_str(&refusal).unwrap();
    assert_eq!(
        parsed["started"], false,
        "a refused job must not read as started"
    );
    let note = parsed["note"].as_str().unwrap();
    assert!(
        note.contains("NOT started"),
        "the model has to be told plainly: {note}"
    );
    assert!(
        note.contains("queued"),
        "and told NOT to call it queued, since nothing is queueing it: {note}"
    );

    // Slots are returned, or capacity would erode to zero over a session.
    drop(held);
    assert!(take_slot(&slots).is_ok(), "a finished job frees its slot");
}

fn ledger() -> JobLedger {
    JobLedger::new(Arc::new(Store::open_in_memory().unwrap()))
}

/// A run that produced nothing cannot claim the outcome, no matter how
/// confident its prose. This is the "I'll report back" failure in miniature:
/// the agent narrates success it has no evidence for.
#[test]
fn a_run_that_produced_nothing_cannot_claim_success() {
    let confident = assess("All done! The app is built and working perfectly.", 0);
    assert_eq!(confident.process_completed, Fact::Yes, "it did run");
    assert_eq!(confident.artifact_produced, Fact::No, "but made nothing");
    assert_eq!(
        confident.outcome_achieved,
        Fact::Unknown,
        "so the outcome is unproven, whatever the report says"
    );
    assert_eq!(confident.verdict(), JobState::Inconclusive);
}

/// Producing an artifact is evidence that something was made — NOT that the
/// task was accomplished. An earlier draft upgraded `outcome_achieved` to `Yes`
/// here; a co-review called it the original defect wearing a new name, and it
/// was right: an artifact can be partial or irrelevant, and the report next to
/// it can describe a failure.
#[test]
fn producing_an_artifact_is_not_achieving_the_outcome() {
    let produced = assess("Deck written to artifacts/deck.pptx", 1);
    assert_eq!(produced.process_completed, Fact::Yes);
    assert_eq!(produced.artifact_produced, Fact::Yes, "something was made");
    assert_eq!(
        produced.result_validated,
        Fact::Unknown,
        "nothing on this path checks the output"
    );
    assert_eq!(
        produced.outcome_achieved,
        Fact::Unknown,
        "and nothing on this path judges whether the goal was met"
    );
    assert_eq!(produced.verdict(), JobState::Inconclusive);
}

/// The failure this most directly prevents: a job that reports its own failure
/// while having written a file must not read as success.
#[test]
fn a_failure_narrative_beside_an_artifact_is_not_success() {
    let confused = assess("I could not reach the API, so the deck is incomplete.", 1);
    assert_eq!(confused.verdict(), JobState::Inconclusive);
    assert!(
        confused.caveat().unwrap().contains("unverified"),
        "and it is reported as unproven"
    );
}

/// No path through `assess` can reach `Succeeded` today, and that is the
/// deliberate position: nothing here validates anything. This test exists to
/// fail loudly the day someone wires a validator in without revisiting it.
#[test]
fn no_unvalidated_run_can_reach_succeeded() {
    for (report, artifacts) in [("all done!", 0), ("all done!", 3), ("", 0), ("   ", 2)] {
        assert_ne!(
            assess(report, artifacts).verdict(),
            JobState::Succeeded,
            "unvalidated run claimed success: {report:?} / {artifacts} artifacts"
        );
    }
}

#[test]
fn wrap_prompt_leaves_a_turn_alone_when_there_is_no_news() {
    let led = ledger();
    assert_eq!(
        wrap_prompt(&led, "what's the weather"),
        "what's the weather",
        "an empty ledger must not touch the user's turn"
    );
}

/// The note has to instruct the model not to launder an interrupted job into a
/// finished one — the wording is the safeguard.
#[test]
fn wrap_prompt_forbids_reporting_unfinished_work_as_done() {
    let led = ledger();
    let (id, _) = led
        .claim(
            "background",
            "build the app",
            "build it",
            crate::application::jobs::JobLimits::default(),
        )
        .unwrap();
    led.start(&id, None);
    led.recover_abandoned(0.0); // the owning process is gone, lease and all

    let wrapped = wrap_prompt(&led, "how's it going?");
    assert!(wrapped.contains("INTERRUPTED"), "{wrapped}");
    assert!(
        wrapped.contains("do not describe an interrupted or unverified job as finished"),
        "{wrapped}"
    );
    assert!(wrapped.ends_with("how's it going?"), "{wrapped}");
}
