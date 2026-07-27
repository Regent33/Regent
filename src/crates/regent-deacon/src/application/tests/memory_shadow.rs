//! The two properties that make this measurement worth trusting: it must not
//! change what it measures, and it must be off unless asked for.

use super::*;
use regent_graph::MemoryTarget;
use regent_store::Store;

fn graph_with_entries() -> (Arc<GraphMemory>, Arc<Store>) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let graph = Arc::new(GraphMemory::new(Arc::clone(&store)));
    graph
        .add_entry(MemoryTarget::Memory, "owner prefers tabs over spaces")
        .unwrap();
    graph
        .add_entry(MemoryTarget::User, "works on a Rust agent called Regent")
        .unwrap();
    (graph, store)
}

/// Access counts for the memory store, which is what a shadow pass must leave
/// untouched.
fn access_counts(store: &Store) -> Vec<i64> {
    store
        .nodes_by_kind(MemoryTarget::Memory.kind())
        .unwrap()
        .iter()
        .map(|n| n.access_count)
        .collect()
}

/// The property the whole exercise depends on. `retrieve` records an access;
/// a shadow pass must NOT, or it manufactures the exposure-feedback loop it
/// exists to detect — injected entries accrue hits, hits read as relevance,
/// and the ranking then justifies itself.
#[test]
fn shadow_recall_does_not_touch_what_it_measures() {
    let (graph, store) = graph_with_entries();

    let before = access_counts(&store);
    assert!(!before.is_empty(), "fixture has entries to measure");

    for _ in 0..5 {
        graph.shadow_recall("tabs or spaces", 10).unwrap();
    }

    assert_eq!(
        access_counts(&store),
        before,
        "five shadow passes must leave no trace"
    );

    // The contrast: real retrieval DOES record the access. If this ever stops
    // being true the test above is proving nothing.
    graph.retrieve("tabs or spaces", 10).unwrap();
    assert_ne!(
        access_counts(&store),
        before,
        "real retrieval still records the access"
    );
}

/// The candidate set is reported, not just the selection: "the top 5 looked
/// fine" says nothing about what ranked sixth, and the plan requires the
/// denominator.
#[test]
fn shadow_recall_reports_the_candidate_set_not_just_the_selection() {
    let (graph, _store) = graph_with_entries();
    let shadow = graph.shadow_recall("Regent tabs", 1).unwrap();

    assert!(shadow.selected.len() <= 1, "k caps the selection");
    assert!(
        shadow.considered >= shadow.selected.len(),
        "considered ({}) must cover selected ({})",
        shadow.considered,
        shadow.selected.len()
    );
    assert!(
        shadow.would_inject_chars > 0,
        "the would-be injection is sized so it can be compared to the block"
    );
}

/// Step 1's baseline, and the number that makes narrowing urgent: the live
/// store was at 75% of its ceiling with six entries.
#[test]
fn block_metrics_report_the_per_turn_cost_of_the_static_block() {
    let (graph, _store) = graph_with_entries();
    let metrics = graph.block_metrics().unwrap();

    assert_eq!(metrics.len(), 2, "both stores are measured");
    for m in &metrics {
        assert!(m.entries > 0, "{:?} has entries", m.target);
        assert!(m.chars > 0, "and they cost characters every turn");
        assert!(m.limit > 0);
        assert!(m.percent_full() <= 100);
    }
}

/// Inert by default: an operator who has not opted in pays nothing and gets
/// nothing.
#[test]
fn measurement_is_off_unless_asked_for() {
    // The fixed-name env var makes this inherently racy with other tests, so
    // it is asserted only in the direction that is safe: absent means off.
    if std::env::var("REGENT_MEMORY_SHADOW").is_err() {
        assert!(!enabled(), "no env var means no measurement");
    }
}
