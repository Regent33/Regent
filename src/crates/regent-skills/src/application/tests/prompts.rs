//! What the reviewer prompt must keep saying. These are contract assertions,
//! not prose review: the measured failure was a reviewer that declined 215
//! times and saved 9 things across 1,186 sessions, and every assertion here
//! pins one of the properties that failure traced back to.
//!
//! Deliberately matched on SHORT phrases that contain no line break. An
//! earlier draft asserted on `"...required\nconfiguration"`, which pinned the
//! exact column the prompt happened to wrap at — a reflow no one thought was a
//! behavior change would have failed the suite.

use super::REVIEW_SYSTEM_PROMPT;

#[test]
fn the_save_side_names_its_categories() {
    // The old prompt left "learning" undefined and then spent four vivid
    // paragraphs on what NOT to save. Naming the categories is the fix.
    for signal in [
        "Decisions:",
        "Constraints and conventions:",
        "Corrections:",
        "Stable environment facts:",
        "User facts and preferences:",
        "Reusable technical lessons:",
    ] {
        assert!(
            REVIEW_SYSTEM_PROMPT.contains(signal),
            "reviewer prompt no longer names the durable category {signal:?}"
        );
    }
}

#[test]
fn a_single_mention_is_enough_to_be_durable() {
    // A decision stated once is still a decision. Requiring recurrence is what
    // silently discards most of what a session is actually worth.
    assert!(REVIEW_SYSTEM_PROMPT.contains("Do not require a fact to have appeared repeatedly."));
    assert!(REVIEW_SYSTEM_PROMPT.contains("durable the first time it is said"));
}

#[test]
fn the_hard_won_prohibitions_survive() {
    // These came from real false limits the agent later cited against itself.
    // Loosening the save bar must never quietly drop them.
    for prohibition in [
        "A tool that failed today is not a tool that is broken.",
        "Environment-dependent failures",
        "A single request is not a class of work.",
        "save the remedy instead of the failure",
    ] {
        assert!(
            REVIEW_SYSTEM_PROMPT.contains(prohibition),
            "reviewer prompt dropped the prohibition {prohibition:?}"
        );
    }
}

#[test]
fn skills_are_no_longer_pushed_on_every_session() {
    // The old text said "be ACTIVE — most sessions produce at least one skill
    // update", which spent the reviewer's effort on speculative skills instead
    // of on memory. Skills are meant to be the rare artifact.
    assert!(!REVIEW_SYSTEM_PROMPT.contains("most sessions produce at least one skill update"));
    assert!(REVIEW_SYSTEM_PROMPT.contains("Skills are rarer"));
}

#[test]
fn the_decline_verdict_stays_backward_compatible() {
    // `Nothing to save.` is what the existing learning-loop tests script and
    // what the deacon logs as `outcome=`. The new counted verdict is additive;
    // removing the old form would break both.
    assert!(REVIEW_SYSTEM_PROMPT.contains("Nothing to save."));
    assert!(REVIEW_SYSTEM_PROMPT.contains("Review complete: candidates_considered=<N> saved=<N>"));
}
