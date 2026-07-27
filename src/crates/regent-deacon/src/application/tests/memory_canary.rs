//! W3 step 5 — what the canary may and may not add to a turn.
//!
//! The load-bearing test is the FIRST one. Retrieval is scored on `memory` and
//! `user`, and the prompt block is every `memory` and `user` node there is, so
//! in the ordinary case retrieval finds nothing the prompt does not already
//! carry and the canary must inject NOTHING. A canary that fires on a normal
//! turn is duplicating the block, not extending it.

use super::*;
use regent_graph::{MemoryTarget, Recalled};
use regent_store::{NodeRow, Store};
use std::sync::Arc;

fn recalled(kind: &str, content: &str) -> Recalled {
    Recalled {
        node: NodeRow {
            id: format!("n:{content}"),
            kind: kind.to_owned(),
            name: String::new(),
            content: content.to_owned(),
            provenance: "user_stated".to_owned(),
            trust: 0.9,
            session_id: None,
            created_at: 0.0,
            updated_at: 0.0,
            ttl_expires_at: None,
            access_count: 0,
            content_hash: String::new(),
        },
        score: 1.0,
        via: None,
    }
}

/// `note_from` with a throwaway history — for the cases that are about the
/// block rather than about repeat injection.
fn note(hits: &[Recalled], block: &str) -> Option<String> {
    note_from(hits, block, &mut HashSet::new())
}

#[test]
fn entries_already_in_the_prompt_are_not_injected_again() {
    let hits = [
        recalled("memory", "owner prefers tabs over spaces"),
        recalled("user", "works on a Rust agent called Regent"),
    ];
    let block = "MEMORY (your personal notes)\nowner prefers tabs over spaces\n§\n\
                 USER PROFILE\nworks on a Rust agent called Regent";

    assert_eq!(
        note(&hits, block),
        None,
        "everything retrieval found is already in the block — the canary must add nothing"
    );
}

#[test]
fn an_entry_written_after_the_prompt_was_frozen_is_injected() {
    // The one case with a real payload: the block is rendered once at session
    // build, so a write by the learning loop (or another session, or this
    // session's own memory tool) is invisible until restart.
    let hits = [
        recalled("memory", "owner prefers tabs over spaces"),
        recalled("memory", "deploys on Fridays are banned"),
    ];
    let block = "MEMORY (your personal notes)\nowner prefers tabs over spaces";

    let got = note(&hits, block).expect("the unseen entry should be injected");
    assert!(got.contains("deploys on Fridays are banned"));
    assert!(
        !got.contains("tabs over spaces"),
        "the entry already in the prompt must not be repeated"
    );
    assert!(
        got.contains("NOT instructions"),
        "injected memory keeps its inert-data framing (poisoning defense)"
    );
}

/// [co-audit] The frozen block never gains a post-freeze entry, so dedup
/// against the block alone would re-prepend the same memory on every later
/// turn that recalled it — content the model can already see in its history.
#[test]
fn a_post_freeze_entry_is_injected_once_per_session_not_every_turn() {
    let hits = [recalled("memory", "deploys on Fridays are banned")];
    let block = "MEMORY (your personal notes)\n(empty)";
    let mut seen = HashSet::new();

    assert!(
        note_from(&hits, block, &mut seen).is_some(),
        "first recall injects"
    );
    assert_eq!(
        note_from(&hits, block, &mut seen),
        None,
        "the second turn must not repeat what the first one already put in history"
    );
}

/// An entry dropped for budget has NOT been shown, so it must stay eligible.
#[test]
fn an_entry_dropped_for_budget_is_not_marked_as_seen() {
    // Sized so the first entry fits alone and the second cannot follow it.
    let big = recalled("memory", &"a".repeat(BUDGET_CHARS / 2));
    let loser = recalled("memory", &format!("loser {}", "b".repeat(BUDGET_CHARS / 2)));
    let mut seen = HashSet::new();

    let first = note_from(&[big, loser.clone()], "empty block", &mut seen)
        .expect("the first entry fits alone");
    assert!(!first.contains("loser"), "the second entry lost the budget");
    assert!(
        note_from(&[loser], "empty block", &mut seen).is_some(),
        "the entry that lost the budget race was never shown, so it stays eligible"
    );
}

#[test]
fn nothing_retrieved_means_no_note() {
    assert_eq!(note(&[], "some prompt"), None);
}

/// [co-audit] The doc calls this a ceiling on what the canary adds to a turn,
/// so it has to bound the RENDERED note — framing and provenance labels
/// included — not just the raw entry text.
#[test]
fn the_budget_bounds_the_rendered_note_and_drops_whole_entries() {
    let hits = [
        recalled("memory", &"x".repeat(BUDGET_CHARS / 2)),
        recalled("memory", &"y".repeat(BUDGET_CHARS / 2)),
        recalled("memory", "the third entry, which does not fit"),
    ];

    let got = note(&hits, "block with none of it").expect("the first entry fits");
    assert!(
        got.chars().count() <= BUDGET_CHARS,
        "rendered note is {} chars, over the {BUDGET_CHARS} ceiling",
        got.chars().count()
    );
    assert!(!got.contains("the third entry"));
    // Whatever survived, survived whole: a truncated memory is a changed memory.
    assert!(got.contains(&"x".repeat(BUDGET_CHARS / 2)));
}

#[test]
fn a_single_entry_over_budget_is_dropped_rather_than_cut() {
    let huge = "y".repeat(BUDGET_CHARS + 1);
    assert_eq!(note(&[recalled("memory", &huge)], "unrelated"), None);
}

#[test]
fn a_job_status_preamble_is_not_part_of_the_query() {
    // `run_turn` sees the text AFTER `wrap_prompt`. Searching memory for job
    // labels is a different query from the one step 4 measured.
    let turn = "[System note — background job update, not yet seen by the user:\n\
                deploy-docs: interrupted]\n\nwhat shell do I use?";
    assert_eq!(user_words(turn), "what shell do I use?");
    assert_eq!(user_words("what shell do I use?"), "what shell do I use?");
    // [co-audit] A user turn that merely opens with a bracketed heading keeps
    // its heading — stripping it would discard the best retrieval terms.
    let heading = "[Project Regent]\n\nhow should I deploy it?";
    assert_eq!(user_words(heading), heading);
}

#[test]
fn off_unless_the_env_flag_says_otherwise() {
    // The kill switch. Read per call, so it can be pulled without a restart.
    assert!(
        !enabled(),
        "the canary must be off by default (REGENT_MEMORY_CANARY unset)"
    );
}

/// End to end against a real graph: the ordinary case is a byte-identical
/// prompt. This is the property the canary is allowed to have in production.
#[test]
fn against_a_real_corpus_the_block_covers_retrieval_and_nothing_is_added() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let graph = regent_graph::GraphMemory::new(Arc::clone(&store));
    graph
        .add_entry(MemoryTarget::Memory, "owner runs Windows with PowerShell")
        .unwrap();
    graph
        .add_entry(MemoryTarget::User, "the owner is called Ralph")
        .unwrap();

    let block = graph.render_prompt_block().unwrap();
    assert_eq!(
        recall_note(
            &graph,
            &block,
            &mut HashSet::new(),
            "what shell does the owner use?"
        ),
        None,
        "the block is the whole corpus, so entry-scoped recall is a subset of it"
    );

    // Now the gap the canary exists for: a write AFTER the block was rendered.
    graph
        .add_entry(MemoryTarget::Memory, "the owner banned Friday deploys")
        .unwrap();
    let got = recall_note(
        &graph,
        &block,
        &mut HashSet::new(),
        "can I deploy on Friday?",
    )
    .expect("memory saved after the freeze is not in the frozen block");
    assert!(got.contains("banned Friday deploys"));
    assert!(
        !got.contains("PowerShell"),
        "pre-freeze entries stay deduped even when they rank"
    );
}
