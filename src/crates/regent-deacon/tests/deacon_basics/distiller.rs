//! SPL P5 (§3.6): the Distiller — fill-ratio watcher → ONE consolidation
//! model call → a staged, HUMAN-GATED proposal. Nothing lands without
//! approval; approval applies through the budgeted persona path with a graph
//! backup of the old content.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_deacon::distill_once;
use tempfile::TempDir;

#[tokio::test]
async fn over_full_store_stages_a_gated_proposal_and_nothing_lands_unapproved() {
    let dir = TempDir::new().unwrap();
    // One scripted reply = exactly one consolidation call: the under-trigger
    // rows must not spend model calls (the script would exhaust otherwise).
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply(
        "Consolidated soul, nothing semantic lost.",
    )]);
    let (sm, _rx) = make_session_manager(&dir, provider.clone());
    let store = sm.store_handle();

    // soul at ~81% of its 8k budget → trigger; about stays far under → skip.
    let fat = "I like ravioli. ".repeat(405); // 6480 chars
    store.set_persona("soul", &fat).unwrap();
    store.set_persona("about", "Small note.").unwrap();

    let proposed = distill_once(store, provider.as_ref()).await;
    assert_eq!(proposed, 1, "exactly the over-full row proposes");

    // Human-gated: the store is UNCHANGED until approval; the proposal sits
    // in the same pending queue the memory approval UI reads.
    assert_eq!(store.get_persona("soul").unwrap(), fat);
    let pending = store.list_pending_writes(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "distill:soul");
    assert_eq!(pending[0].kind, "persona_rewrite");

    // A second pass while one is pending must not stack a duplicate (stable
    // id hits the primary key) — and must not spend another model call.
    let again = distill_once(store, provider.as_ref()).await;
    assert_eq!(again, 0);
    assert_eq!(store.list_pending_writes(10).unwrap().len(), 1);

    // Approval applies through the budgeted persona path, with the old
    // content backed up first (a bulk rewrite is a relocation, not a loss).
    let applied = sm.approve_memory_write("distill:soul").unwrap();
    assert_eq!(applied.as_deref(), Some("persona:soul"));
    assert_eq!(
        store.get_persona("soul").unwrap(),
        "Consolidated soul, nothing semantic lost."
    );
    assert_eq!(store.get_persona("backup.soul").unwrap(), fat);
    assert!(store.list_pending_writes(10).unwrap().is_empty());
}

#[tokio::test]
async fn rejecting_a_distill_proposal_keeps_the_store_intact() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("Shorter.")]);
    let (sm, _rx) = make_session_manager(&dir, provider.clone());
    let store = sm.store_handle();

    let fat = "Boundary: never autopost. ".repeat(380); // ~9.9k of 12k budget
    store.set_persona("constitution", &fat).unwrap();
    assert_eq!(distill_once(store, provider.as_ref()).await, 1);

    assert!(sm.reject_memory_write("distill:constitution").unwrap());
    assert_eq!(store.get_persona("constitution").unwrap(), fat);
    assert!(store.list_pending_writes(10).unwrap().is_empty());
}
