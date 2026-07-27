//! The rule under test is a claim discipline, not arithmetic: what a set of
//! observations entitles the system to tell the user.

use super::*;

#[test]
fn succeeded_requires_being_able_to_claim_the_outcome() {
    // The exact shape of the old bug: the process returned, so it was called
    // done. Running to the end is not achieving the goal.
    let ran_only = Completion {
        process_completed: Fact::Yes,
        artifact_produced: Fact::Unknown,
        result_validated: Fact::Unknown,
        outcome_achieved: Fact::Unknown,
    };
    assert_eq!(ran_only.verdict(), JobState::Inconclusive);
    assert!(
        ran_only.caveat().unwrap().contains("unverified"),
        "the report must carry the doubt"
    );

    // Even producing an artifact is not enough on its own.
    let made_something = Completion {
        artifact_produced: Fact::Yes,
        ..ran_only
    };
    assert_eq!(
        made_something.verdict(),
        JobState::Inconclusive,
        "a file existing does not mean the task was accomplished"
    );

    // Only the outcome fact unlocks success.
    let achieved = Completion {
        outcome_achieved: Fact::Yes,
        ..made_something
    };
    assert_eq!(achieved.verdict(), JobState::Succeeded);
}

#[test]
fn a_verified_success_is_the_only_one_reported_without_a_caveat() {
    let self_reported = Completion {
        process_completed: Fact::Yes,
        artifact_produced: Fact::Yes,
        result_validated: Fact::Unknown,
        outcome_achieved: Fact::Yes,
    };
    assert_eq!(self_reported.verdict(), JobState::Succeeded);
    assert!(
        self_reported
            .caveat()
            .unwrap()
            .contains("not independently"),
        "an unvalidated success still says so"
    );

    let verified = Completion {
        result_validated: Fact::Yes,
        ..self_reported
    };
    assert_eq!(verified.caveat(), None, "only this one claims cleanly");
}

#[test]
fn unknown_is_never_silently_upgraded() {
    assert_eq!(Completion::unknown().verdict(), JobState::Inconclusive);
    assert!(
        Completion::unknown()
            .caveat()
            .unwrap()
            .contains("do not describe this as finished"),
        "with nothing observed, the instruction is explicit"
    );
    assert_eq!(Fact::parse("maybe"), Fact::Unknown, "junk reads as unknown");
    assert_eq!(Fact::parse("yes"), Fact::Yes);
}

/// A process error is evidence about the PROCESS only.
///
/// An earlier draft set `outcome_achieved = No` here, which claims we know the
/// goal was missed. A job can write the file and then fall over while
/// reporting; asserting `No` is the same overreach as inferring `Yes` from a
/// bare return, pointed the other way. Caught in co-review.
#[test]
fn a_dead_process_claims_nothing_downstream() {
    let failed = Completion::process_failed();
    assert_eq!(failed.process_completed, Fact::No, "this much we know");
    assert_eq!(
        failed.artifact_produced,
        Fact::Unknown,
        "a crash mid-write leaves artifacts genuinely unknown, not 'no'"
    );
    assert_eq!(
        failed.outcome_achieved,
        Fact::Unknown,
        "an error does not prove the work did not happen"
    );
    // The job row is still stamped `failed` by the runtime — the process died,
    // and that is a fact about the process. The FACTS just don't pretend to
    // know more than that.
    assert_eq!(failed.verdict(), JobState::Inconclusive);
}

#[test]
fn interrupted_is_neither_success_nor_failure_and_can_be_retried() {
    assert!(JobState::Interrupted.is_terminal(), "the attempt is over");
    assert!(JobState::Interrupted.is_retryable(), "the job is not");

    // Everything else is settled: it has a recorded outcome, so an automatic
    // rerun would redo work the user may already have. `Failed` included —
    // rerunning a failure is the caller's decision, not the ledger's.
    for settled in [
        JobState::Succeeded,
        JobState::Failed,
        JobState::Cancelled,
        JobState::TimedOut,
        JobState::Inconclusive,
    ] {
        assert!(!settled.is_retryable(), "{settled} must not auto-retry");
        assert!(settled.is_terminal(), "{settled}");
    }

    assert!(!JobState::Queued.is_terminal());
    assert!(!JobState::Running.is_terminal());
}

/// `TimedOut` is not `Failed`: we know it did not finish in time, not that the
/// work was wrong. Keeping them distinct is why `stop` takes a reason.
#[test]
fn a_stop_reason_cannot_stamp_a_success() {
    assert_eq!(StopReason::Cancelled.state(), JobState::Cancelled);
    assert_eq!(StopReason::TimedOut.state(), JobState::TimedOut);
    assert_eq!(StopReason::Interrupted.state(), JobState::Interrupted);
    for reason in [
        StopReason::Cancelled,
        StopReason::TimedOut,
        StopReason::Interrupted,
    ] {
        assert_ne!(
            reason.state(),
            JobState::Succeeded,
            "no runtime stop may report success"
        );
    }
}

#[test]
fn state_and_fact_survive_a_round_trip_through_the_database() {
    for state in [
        JobState::Queued,
        JobState::Running,
        JobState::Succeeded,
        JobState::Failed,
        JobState::Inconclusive,
        JobState::Interrupted,
        JobState::Cancelled,
        JobState::TimedOut,
    ] {
        assert_eq!(JobState::parse(state.as_str()), state, "{state}");
    }
    let c = Completion {
        process_completed: Fact::Yes,
        artifact_produced: Fact::No,
        result_validated: Fact::Unknown,
        outcome_achieved: Fact::Yes,
    };
    assert_eq!(c.as_row(), ["yes", "no", "unknown", "yes"]);
}
