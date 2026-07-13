//! Unit tests for `embeddings` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use crate::domain::entities::NodeRow;
use crate::infra::db::Store;

fn insert_node(store: &Store, id: &str) {
    store
        .insert_node(&NodeRow {
            id: id.to_owned(),
            kind: "fact".to_owned(),
            name: id.to_owned(),
            content: format!("content for {id}"),
            provenance: "user_stated".to_owned(),
            trust: 1.0,
            session_id: None,
            created_at: 1.0,
            updated_at: 1.0,
            ttl_expires_at: None,
            access_count: 0,
            content_hash: format!("hash-{id}"),
        })
        .unwrap();
}

#[test]
fn vector_search_ranks_by_cosine() {
    let store = Store::open_in_memory().unwrap();
    for id in ["a", "b", "c"] {
        insert_node(&store, id);
    }
    store.upsert_embedding("a", "m", &[1.0, 0.0]).unwrap();
    store.upsert_embedding("b", "m", &[0.0, 1.0]).unwrap();
    store.upsert_embedding("c", "m", &[0.9, 0.1]).unwrap();

    let hits = store.vector_search(&[1.0, 0.0], "m", 2).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].0, "a", "exact match ranks first");
    assert_eq!(hits[1].0, "c", "nearest neighbour second");
    assert_eq!(store.embedding_count("m").unwrap(), 3);
}

#[test]
fn upsert_replaces_existing_vector() {
    let store = Store::open_in_memory().unwrap();
    insert_node(&store, "a");
    store.upsert_embedding("a", "m", &[1.0, 0.0]).unwrap();
    store.upsert_embedding("a", "m", &[0.0, 1.0]).unwrap();
    assert_eq!(store.embedding_count("m").unwrap(), 1);
    let hits = store.vector_search(&[0.0, 1.0], "m", 1).unwrap();
    assert!((hits[0].1 - 1.0).abs() < 1e-5, "reflects the latest vector");
}

#[test]
fn backfill_list_finds_only_unembedded_nodes() {
    let store = Store::open_in_memory().unwrap();
    insert_node(&store, "a");
    insert_node(&store, "b");
    store.upsert_embedding("a", "m", &[1.0, 0.0]).unwrap();
    let need = store.nodes_needing_embedding("m", 10).unwrap();
    assert_eq!(need.len(), 1);
    assert_eq!(need[0].0, "b");
}

#[test]
fn dimension_mismatch_is_skipped() {
    let store = Store::open_in_memory().unwrap();
    insert_node(&store, "a");
    store.upsert_embedding("a", "m", &[1.0, 0.0, 0.0]).unwrap(); // dim 3
    let hits = store.vector_search(&[1.0, 0.0], "m", 5).unwrap(); // query dim 2
    assert!(
        hits.is_empty(),
        "stale-dimension vectors are excluded, not mis-scored"
    );
}

#[test]
fn deleting_a_node_cascades_to_its_embedding() {
    let store = Store::open_in_memory().unwrap();
    insert_node(&store, "a");
    store.upsert_embedding("a", "m", &[1.0, 0.0]).unwrap();
    store.delete_node("a").unwrap();
    assert_eq!(store.embedding_count("m").unwrap(), 0);
}
