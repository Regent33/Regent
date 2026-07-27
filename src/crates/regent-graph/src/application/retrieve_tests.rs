//! Retrieval unit tests — the per-kind flood cap and recency decay.
//! Split from `retrieve.rs` (file-size rule); a child module of `retrieve`, so
//! `super::*` still reaches its private items.

use super::*;
use crate::domain::entities::Provenance;
use regent_store::Store;
use std::sync::Arc;

// The live failure shape: 18 pinned constitution chunks vs 2 real facts —
// a query matching everything must still surface the facts in top-k.
#[test]
fn document_chunks_cannot_flood_out_other_kinds() {
    let mem = GraphMemory::new(Arc::new(Store::open_in_memory().unwrap()));
    for i in 0..18 {
        mem.add_node(
            "constitution",
            &format!("constitution:{i}"),
            &format!("[Constitution §{i}] shared keyword regent section {i}"),
            Provenance::UserStated,
            None,
            None,
        )
        .unwrap();
    }
    mem.add_node(
        "fact",
        "project",
        "regent project fact: building the React constitution site",
        Provenance::UserStated,
        None,
        None,
    )
    .unwrap();
    mem.add_node(
        "fact",
        "path",
        "regent repo lives at D drive fact",
        Provenance::UserStated,
        None,
        None,
    )
    .unwrap();

    let got = mem.retrieve("regent", 10).unwrap();
    let facts = got.iter().filter(|r| r.node.kind == "fact").count();
    assert_eq!(
        facts, 2,
        "both real facts must surface (was 0 before the cap)"
    );
    assert_eq!(got.len(), 10, "backfill still fills k after the quota pass");
}

// A store holding only one kind still fills k — the cap never starves.
#[test]
fn homogeneous_store_still_fills_k() {
    let mem = GraphMemory::new(Arc::new(Store::open_in_memory().unwrap()));
    for i in 0..6 {
        mem.add_node(
            "constitution",
            &format!("c{i}"),
            &format!("regent section {i}"),
            Provenance::UserStated,
            None,
            None,
        )
        .unwrap();
    }
    let got = mem.retrieve("regent", 4).unwrap();
    assert_eq!(got.len(), 4, "backfill fills k from the only kind");
}

#[test]
fn pinned_nodes_never_decay_but_ttl_nodes_do() {
    // Pinned (constitution sections, user-pinned facts): full weight at any age.
    assert_eq!(recency_factor(None, 0.0), 1.0);
    assert_eq!(recency_factor(None, 180.0), 1.0);
    // TTL'd memories decay on the 30-day half-scale as before.
    assert_eq!(recency_factor(Some(1.0), 0.0), 1.0);
    let old = recency_factor(Some(1.0), 180.0);
    assert!(
        old < 0.2,
        "a 6-month-old memory should score ~0.14, got {old}"
    );
}

/// W3 step 5's premise, asserted rather than assumed: entry-scoped recall ranks
/// only over the kinds the always-injected block carries, so a canary deduped
/// against that block has nothing to add on an ordinary turn.
#[test]
fn entry_recall_returns_only_the_kinds_the_prompt_block_carries() {
    let mem = GraphMemory::new(Arc::new(Store::open_in_memory().unwrap()));
    for (kind, name) in [
        ("constitution", "c1"),
        ("memory", "m1"),
        ("user", "u1"),
        ("fact", "f1"),
    ] {
        mem.add_node(
            kind,
            name,
            &format!("{kind} node about regent"),
            Provenance::UserStated,
            None,
            None,
        )
        .unwrap();
    }

    let got = mem.entry_recall("regent", 10).unwrap();
    assert!(!got.is_empty(), "the memory/user nodes should match");
    assert!(
        got.iter()
            .all(|r| matches!(r.node.kind.as_str(), "memory" | "user")),
        "constitution and fact nodes reach the model by other routes and must not \
         be counted against the block's budget: {:?}",
        got.iter().map(|r| &r.node.kind).collect::<Vec<_>>()
    );
}
