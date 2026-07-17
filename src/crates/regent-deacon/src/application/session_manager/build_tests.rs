//! Unit tests for `build` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).
//!
//! The "light defs are strictly smaller than full" and "light prompt bytes
//! == full prompt bytes" acceptance checks (ADR-038 P0(b)) live in
//! `tests/deacon_basics/tiering.rs` instead of here: proving them needs a
//! real `SessionManager` (store, skills library, provider) built from the
//! `make_session_manager`/`ScriptedProvider` fixture that only the
//! integration-test crate has — this unit-test module has no DB/skills
//! fixture of its own. `LIGHT_PINNED`'s own shape is checked here, pure.

use super::{LIGHT_PINNED, TIER1_CEILING_CHARS, cap_tier1};
use crate::domain::ledger::{Segment, Tier};

// ADR-038 P0(b): the light profile's pinned set must be a strict subset of
// what a real config-pinned session keeps resident — narrower on purpose
// (that's the whole savings) — and must always include the escalation
// trigger so P2's routing (not yet built) has something to route on.
#[test]
fn light_pinned_is_the_minimal_escalation_safe_set() {
    assert!(LIGHT_PINNED.contains(&"code_task"), "escalation trigger stays visible");
    assert!(LIGHT_PINNED.len() <= 6, "P0(b)'s light set targets ~6 pinned tools");
    assert!(
        !LIGHT_PINNED.contains(&"load_tools"),
        "load_tools is auto-injected by the catalog's tiering, never listed here"
    );
}

// SPL §3.4: three maxed stores can't stack — the SESSION tier is capped,
// trimming from the end (memory before skills before persona), Tier-0
// segments untouched, and a marker names the trim.
#[test]
fn tier1_ceiling_trims_from_the_end_and_spares_tier0() {
    let capped = cap_tier1(vec![
        Segment::tier0("system_prompt", "S".repeat(90_000)),
        Segment::tier1("persona", "P".repeat(28_000)),
        Segment::tier1("skills_index", "K".repeat(6_000)),
        Segment::tier1("memory", "M".repeat(9_000)),
    ]);
    assert_eq!(capped[0].text.len(), 90_000, "Tier 0 is never trimmed");
    assert_eq!(capped[1].text.len(), 28_000, "persona is trimmed last");
    // 43k of Tier 1 → 7k over: memory absorbs the whole trim (9k → 2k +
    // marker), skills survive intact.
    assert_eq!(capped[2].text.len(), 6_000);
    assert!(capped[3].text.starts_with("MM"));
    assert!(capped[3].text.contains("trimmed at the Tier-1 ceiling"));
    let tier1: usize = capped
        .iter()
        .filter(|s| s.tier == Tier::Session)
        .map(|s| s.text.len())
        .sum();
    assert!(
        tier1 <= TIER1_CEILING_CHARS + 200,
        "within ceiling (+marker): {tier1}"
    );

    // Under the ceiling nothing changes.
    let untouched = cap_tier1(vec![Segment::tier1("persona", "p".repeat(100))]);
    assert_eq!(untouched[0].text.len(), 100);
}
