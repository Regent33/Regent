//! Unit tests for `prompt_lines` — the Tier-1 ceiling and its trim
//! (extracted for the file-size rule; same module tree via #[path]).

use super::{TIER1_CEILING_BYTES, cap_tier1};
use crate::domain::ledger::{Segment, Tier};
use regent_graph::GraphMemory;
use regent_skills::{SKILLS_INDEX_HOOK_CHARS, SKILLS_INDEX_MAX};
use regent_store::{ABOUT_SECTIONS, Store, persona_budget};
use std::sync::Arc;

/// Longest skill name `validate_name` accepts, and the `- ` / `: ` / `\n`
/// around it on an index line.
const SKILL_NAME_CHARS: usize = 64;
const SKILL_LINE_FRAMING: usize = 5;
/// The index's own `## Skills` preamble, `<available_skills>` tags, the
/// "…and K more" overflow line, and the segment's leading blank line.
const SKILLS_INDEX_FRAMING: usize = 400;
/// The persona block's section headings (`## Your constitution …`, `## Your
/// persona …`, `## About the person you're helping`, five `### Facet`), which
/// ride along with the budgeted content.
const PERSONA_HEADING_CHARS: usize = 400;

/// THE invariant the ceiling's doc comment asserts, redone from the stores'
/// own constants rather than trusted.
///
/// The comment used to claim personas cost 28k against a 36,000 ceiling. They
/// cost 36,000 — `persona_budget` grants constitution 12k, soul 8k, about 6k
/// and 2k to each of the five facets — so a persona written to exactly the
/// limit the write gate advertises filled Tier 1 by itself, and `cap_tier1`
/// then deleted the skills index and the memory block with no marker and no
/// log. Nobody would have connected "Regent forgot everything" to a persona
/// edit that the CLI accepted without complaint.
///
/// So: raise a store budget and this goes red until the ceiling follows.
#[test]
fn the_tier1_ceiling_covers_every_store_budget_at_once() {
    let persona: usize = [
        "constitution".to_owned(),
        "soul".to_owned(),
        "about".to_owned(),
    ]
    .into_iter()
    .chain(
        ABOUT_SECTIONS
            .iter()
            .map(|(slug, _)| format!("about.{slug}")),
    )
    .map(|key| persona_budget(&key))
    .sum::<usize>()
        + PERSONA_HEADING_CHARS;

    // Graph budgets live in `GraphMemory::new`, not in a constant, and the
    // block frames every store with two 46-char rules and a percentage banner
    // — so read both off a real empty block instead of guessing. `m.limit` is
    // BYTES since the graph store was converted; before that this line summed
    // char limits into a byte total and under-counted the store threefold for
    // any non-Latin memory.
    let graph = GraphMemory::new(Arc::new(Store::open_in_memory().unwrap()));
    let memory: usize = graph
        .block_metrics()
        .unwrap()
        .iter()
        .map(|m| m.limit)
        .sum::<usize>()
        + graph.render_prompt_block().unwrap().len();

    let skills = SKILLS_INDEX_MAX
        * (SKILL_NAME_CHARS + SKILL_LINE_FRAMING + SKILLS_INDEX_HOOK_CHARS)
        + SKILLS_INDEX_FRAMING;

    let stacked = persona + memory + skills;
    assert!(
        TIER1_CEILING_BYTES >= stacked,
        "the Tier-1 ceiling ({TIER1_CEILING_BYTES}) is BELOW the sum of the budgets it \
         claims to sit above ({stacked} = persona {persona} + memory {memory} + skills \
         index {skills}). At those budgets cap_tier1 silently deletes the memory block \
         and the skills index. Raise the ceiling or lower a store budget."
    );
}

// SPL §3.4: three maxed stores can't stack — the SESSION tier is capped,
// trimming from the end (memory before skills before persona), Tier-0
// segments untouched, and a marker names the trim.
#[test]
fn tier1_ceiling_trims_from_the_end_and_spares_tier0() {
    let persona = format!("CONSTITUTION_ALWAYS\n{}", "P".repeat(36_000));
    let capped = cap_tier1(vec![
        Segment::tier0("system_prompt", "S".repeat(90_000)),
        Segment::tier0("capabilities", "C".repeat(90_000)),
        Segment::tier1("persona", persona.clone()),
        Segment::tier1("skills_index", "K".repeat(6_000)),
        Segment::tier1("memory", "M".repeat(9_000)),
    ]);
    assert_eq!(
        capped[0].text.len(),
        90_000,
        "system prompt is never trimmed"
    );
    assert_eq!(
        capped[1].text.len(),
        90_000,
        "capabilities are never trimmed"
    );
    assert_eq!(capped[2].text, persona, "persona is trimmed last");
    assert!(
        capped[2].text.starts_with("CONSTITUTION_ALWAYS"),
        "the constitution stays at the front of the always-present persona"
    );
    // 51k of Tier 1 against a 48k ceiling → 3k over: memory absorbs the whole
    // trim (9k → 6k + marker), skills survive intact.
    assert_eq!(capped[3].text.len(), 6_000);
    assert!(capped[4].text.starts_with("MM"));
    assert!(capped[4].text.contains("trimmed at the Tier-1 ceiling"));
    let tier1: usize = capped
        .iter()
        .filter(|s| s.tier == Tier::Session)
        .map(|s| s.text.len())
        .sum();
    assert!(
        tier1 <= TIER1_CEILING_BYTES + 200,
        "within ceiling (+marker): {tier1}"
    );

    // Even when memory + skills are fully removed and persona itself must be
    // trimmed, trimming is from its tail: the constitutional prefix survives.
    let hard_persona = format!("CONSTITUTION_ALWAYS\n{}", "P".repeat(50_000));
    let hard = cap_tier1(vec![
        Segment::tier1("persona", hard_persona.clone()),
        Segment::tier1("skills_index", "K".repeat(6_000)),
        Segment::tier1("memory", "M".repeat(9_000)),
    ]);
    assert!(hard[0].text.len() < hard_persona.len());
    assert!(hard[0].text.starts_with("CONSTITUTION_ALWAYS"));
    assert!(hard[1].text.is_empty() && hard[2].text.is_empty());

    // Under the ceiling nothing changes.
    let untouched = cap_tier1(vec![Segment::tier1("persona", "p".repeat(100))]);
    assert_eq!(untouched[0].text.len(), 100);
}
