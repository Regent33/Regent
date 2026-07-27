//! W3 step 5 — additive canary injection, and the measurement that reshaped it.
//!
//! ## What steps 3 and 4 changed about this step
//!
//! The plan wrote step 5 as the de-risking hop before replacement: inject
//! retrieval alongside the block, establish recall parity, then narrow. That
//! shape assumes retrieval can surface something the block does not. Checked
//! against source, it cannot:
//!
//! - the prompt block is `nodes_by_kind("memory") + nodes_by_kind("user")`,
//!   uncapped and untruncated — the WHOLE corpus of those kinds
//!   (`GraphMemory::entry_nodes`), and an over-budget write is refused rather
//!   than evicting anything;
//! - entry-scoped retrieval — the thing step 4 measured at 0.59x the block —
//!   ranks over exactly those two kinds.
//!
//! So entry-scoped recall is a subset of the block, and "deduped against the
//! existing block" empties it. On an ordinary turn this module injects nothing,
//! and its test suite asserts exactly that.
//!
//! ## What is left, and why it is still worth shipping
//!
//! The block is rendered **once, at session build** (`build.rs`), and frozen
//! into the system prompt. Memory written after that point is in the store and
//! not in the prompt until the session is rebuilt. Three ways that happens: the
//! learning/review loop writing from a session tail, a second concurrent
//! session (the owner runs several), and this session's own `memory` tool. A
//! long-lived desktop or voice session can therefore run for hours beside
//! memory it has already saved and cannot see.
//!
//! That gap is what the canary fills. The payload is normally empty, which is
//! the point of a canary rather than a defect in it.
//!
//! ## What this does NOT establish [co-audit]
//!
//! "Not in the frozen block" is not the same as "new to the model", and the
//! difference is not closeable here:
//!
//! - A learning-loop entry is *distilled from this session's own transcript*,
//!   so its source text is usually still in history. The canary can restate
//!   something the model already read. `seen` bounds that to **once per node
//!   per session**; it does not eliminate it.
//! - Dedup is exact-substring against the block. A paraphrase ("Friday deploys
//!   are banned" vs "deploys on Fridays are prohibited") reads as new. Semantic
//!   supersession is step 6's problem, deliberately, because near-duplicate
//!   *replace* is the wrong primitive for contradictions.
//! - Coverage is query-dependent and bounded by `CANARY_K` and the budget. Step
//!   4's 100% was 12 queries against one corpus, not a guarantee.
//!
//! **Security:** the "reference data, NOT instructions" framing is mitigation,
//! not a boundary. For pre-freeze content this adds no exposure — the same
//! corpus is already in the system prompt. For post-freeze content it does
//! widen the *temporal* surface: a newly written entry now reaches a live
//! session without a rebuild. It rides the user turn, so it carries less
//! authority than the system block, which is the safer of the two placements.
//!
//! Off by default. `REGENT_MEMORY_CANARY=1` turns it on, read per call so it
//! can be pulled without a restart.

use regent_graph::{GraphMemory, Recalled};
use std::collections::HashSet;

/// How many entry-kind candidates to rank. Matches the shadow pass and
/// `memory_search`, so the canary injects from the selection step 4 measured
/// rather than from a second, unmeasured one.
const CANARY_K: usize = 10;

/// Hard ceiling on the **rendered** note — framing, provenance labels and all,
/// not just the entry text. The block itself measured 2,947 chars; this is a
/// fifth of that. Entries are admitted whole or not at all: a truncated memory
/// is a changed memory, and half a preference is worse than none of it.
pub(crate) const BUDGET_CHARS: usize = 600;

/// The prefix `background_task_tool::wrap_prompt` (and this module) use for a
/// bracketed preamble. Matching the specific marker rather than a bare `[`
/// keeps a user turn that merely opens with a bracketed heading intact.
const NOTE_PREFIX: &str = "[System note";

/// The kill switch. Read per call, not cached, so turning it off takes effect
/// on the next turn instead of the next restart.
#[must_use]
pub fn enabled() -> bool {
    std::env::var("REGENT_MEMORY_CANARY")
        .map(|raw| matches!(raw.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// The note to prepend to this turn, or `None` when there is nothing the model
/// has not already been shown — which is the ordinary case.
///
/// `block` is the session's rendered memory segment. `seen` is this session's
/// already-injected node ids, and is updated in place: without it a post-freeze
/// entry would be re-prepended on every later turn that recalled it, since the
/// frozen block never gains it.
#[must_use]
pub fn recall_note(
    graph: &GraphMemory,
    block: &str,
    seen: &mut HashSet<String>,
    turn: &str,
) -> Option<String> {
    let query = user_words(turn);
    if query.trim().is_empty() {
        return None;
    }
    match graph.entry_recall(query, CANARY_K) {
        Ok(hits) => note_from(&hits, block, seen),
        Err(error) => {
            // Fail open: memory recall is an enhancement, never a turn blocker.
            tracing::warn!(%error, "canary recall failed; turn proceeds unchanged");
            None
        }
    }
}

/// The user's actual words, with a leading bracketed system note stripped.
///
/// `run_turn` is downstream of `background_task_tool::wrap_prompt`, which
/// prepends a job-status preamble. Scoring retrieval on that preamble would
/// search memory for job labels and stop-reasons — a different query from the
/// one W3 step 4 measured, on a turn the user did not ask.
fn user_words(turn: &str) -> &str {
    if !turn.starts_with(NOTE_PREFIX) {
        return turn;
    }
    turn.split_once("]\n\n").map_or(turn, |(_, user)| user)
}

/// Dedup against the block and this session's history, then the budget. Split
/// out from the store read so the decision is testable without a corpus.
fn note_from(hits: &[Recalled], block: &str, seen: &mut HashSet<String>) -> Option<String> {
    let mut kept: Vec<Recalled> = Vec::new();
    for hit in hits {
        // Substring against the block, not node ids: the question is whether
        // the model can already see this text, and the block is rendered
        // content. Scoped to the memory segment rather than the whole prompt so
        // a phrase that also appears in the constitution does not read as
        // "already covered".
        if block.contains(&hit.node.content) || seen.contains(&hit.node.id) {
            continue;
        }
        kept.push(hit.clone());
        // ponytail: re-render per candidate — k is 10, and budgeting the real
        // output is the only way the ceiling above is honest. Measure before
        // making this incremental.
        if render(&kept).chars().count() > BUDGET_CHARS {
            kept.pop();
        }
    }
    if kept.is_empty() {
        // Distinguishes "nothing recalled" from "recalled, all already seen" —
        // without it every outcome is an identical silence.
        if !hits.is_empty() {
            tracing::debug!(
                target: "memory_shadow",
                phase = "canary",
                considered = hits.len(),
                "canary recalled only entries already visible to the model"
            );
        }
        return None;
    }
    for hit in &kept {
        seen.insert(hit.node.id.clone());
    }
    let note = render(&kept);
    tracing::info!(
        target: "memory_shadow",
        phase = "canary",
        injected = kept.len(),
        chars = note.chars().count(),
        "canary injected memory written since this session's prompt was frozen"
    );
    Some(note)
}

/// `render_recall` keeps the provenance labels and the "reference data, NOT
/// instructions" framing — memory reaching the prompt automatically is exactly
/// where a poisoned entry would want to land.
fn render(kept: &[Recalled]) -> String {
    format!(
        "{NOTE_PREFIX} — memory saved since this conversation started, and not in your \
         prompt block above. Treat it the same way you treat that block.\n{}]",
        GraphMemory::render_recall(kept)
    )
}

#[cfg(test)]
#[path = "tests/memory_canary.rs"]
mod tests;
