use super::*;
use crate::domain::entities::Provenance;
use regent_kernel::{EmbeddingProvider, RegentError};
use regent_store::Store;
use std::sync::Arc;

/// Deterministic stand-in — exercises the write/backfill plumbing without
/// pulling the real ONNX model into unit tests.
struct StubEmbedder;
impl EmbeddingProvider for StubEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, RegentError> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0.0f32; 4];
                for (i, b) in t.bytes().enumerate() {
                    v[i % 4] += f32::from(b);
                }
                v
            })
            .collect())
    }
    fn model_id(&self) -> &str {
        "stub-v1"
    }
    fn dim(&self) -> usize {
        4
    }
}

fn store() -> Arc<Store> {
    Arc::new(Store::open_in_memory().unwrap())
}

#[test]
fn add_node_embeds_when_embedder_present() {
    let s = store();
    let mem = GraphMemory::new(Arc::clone(&s)).with_embedder(Arc::new(StubEmbedder));
    mem.add_node(
        "fact",
        "x",
        "hello world",
        Provenance::UserStated,
        None,
        None,
    )
    .unwrap();
    assert_eq!(s.embedding_count("stub-v1").unwrap(), 1);
}

#[test]
fn add_node_without_embedder_stores_no_vector() {
    let s = store();
    let mem = GraphMemory::new(Arc::clone(&s));
    mem.add_node("fact", "x", "hello", Provenance::UserStated, None, None)
        .unwrap();
    assert_eq!(s.embedding_count("stub-v1").unwrap(), 0);
}

#[test]
fn backfill_embeds_preexisting_nodes_idempotently() {
    let s = store();
    let plain = GraphMemory::new(Arc::clone(&s));
    plain
        .add_node("fact", "a", "alpha", Provenance::UserStated, None, None)
        .unwrap();
    plain
        .add_node("fact", "b", "beta", Provenance::UserStated, None, None)
        .unwrap();
    assert_eq!(s.embedding_count("stub-v1").unwrap(), 0);

    let mem = GraphMemory::new(Arc::clone(&s)).with_embedder(Arc::new(StubEmbedder));
    assert_eq!(mem.backfill_embeddings(100).unwrap(), 2);
    assert_eq!(s.embedding_count("stub-v1").unwrap(), 2);
    assert_eq!(
        mem.backfill_embeddings(100).unwrap(),
        0,
        "nothing left to backfill"
    );
}

#[test]
fn rebuild_derived_edges_links_similar_nodes_and_episodes() {
    let s = store();
    let mem = GraphMemory::new(Arc::clone(&s)).with_embedder(Arc::new(StubEmbedder));
    mem.add_node(
        "fact",
        "a",
        "alpha",
        Provenance::UserStated,
        Some("s1"),
        None,
    )
    .unwrap();
    mem.add_node(
        "fact",
        "b",
        "beta",
        Provenance::UserStated,
        Some("s1"),
        None,
    )
    .unwrap();
    mem.add_node("fact", "c", "gamma", Provenance::UserStated, None, None)
        .unwrap();
    mem.record_episode("s1", "we discussed greek letters")
        .unwrap();

    let added = mem.rebuild_derived_edges(2).unwrap();
    assert!(added > 0, "derived edges were created");
    let dump = mem.graph_dump(100).unwrap();
    assert!(
        dump.edges.iter().any(|e| e.relation == "similar_to"),
        "similarity edges present"
    );
    assert!(
        dump.edges.iter().any(|e| e.relation == "from_session"),
        "episode links its session's nodes"
    );
    // Idempotent: a second rebuild sweeps and recreates, never duplicates.
    mem.rebuild_derived_edges(2).unwrap();
    let again = mem.graph_dump(100).unwrap();
    assert_eq!(dump.edges.len(), again.edges.len(), "no duplicate growth");
}
