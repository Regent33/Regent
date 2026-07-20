//! Unit tests for `titling` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::{SessionManager, clean_title, exchange_snippet, strip_think};

#[test]
fn snippet_carries_both_sides_and_truncates() {
    // A bare-greeting opener still yields a topical source: the reply.
    let s = exchange_snippet(
        "boss.",
        "Two python voice servers are running; stopping both.",
    );
    assert_eq!(
        s,
        "User: boss.\nAssistant: Two python voice servers are running; stopping both."
    );
    // Each side is capped on a char boundary (multibyte-safe).
    let long = "é".repeat(1_000);
    let capped = exchange_snippet(&long, &long);
    assert_eq!(capped.matches('é').count(), 800);
}

#[test]
fn think_blocks_never_reach_the_title() {
    // Inline reasoning is removed; the title after it survives.
    assert_eq!(
        clean_title(&strip_think(
            "<think>The user wants a trip plan…</think>\nPlan the road trip"
        )),
        "Plan the road trip"
    );
    // Truncated (unterminated) thinking yields nothing — not a garbage title.
    assert_eq!(clean_title(&strip_think("<think>Okay, the user wants")), "");
    // No think tags → unchanged.
    assert_eq!(
        clean_title(&strip_think("Deploy the API")),
        "Deploy the API"
    );
}

#[test]
fn title_gate_only_fires_untitled_first_turn() {
    assert!(SessionManager::should_generate_title(false, 0));
    // Already titled → never.
    assert!(!SessionManager::should_generate_title(true, 0));
    // Has prior user turns → not the first turn.
    assert!(!SessionManager::should_generate_title(false, 1));
    assert!(!SessionManager::should_generate_title(true, 3));
}

#[test]
fn clean_title_trims_and_caps() {
    assert_eq!(clean_title("\"Fix the login bug\""), "Fix the login bug");
    assert_eq!(
        clean_title("Plan a road trip across seven states now"),
        "Plan a road trip across seven"
    );
    assert_eq!(clean_title("Deploy the API!"), "Deploy the API");
    assert_eq!(clean_title("  \n  Refactor  \n more"), "Refactor");
    assert_eq!(clean_title("   "), "");
    assert_eq!(clean_title("`quarterly report`"), "quarterly report");
}
