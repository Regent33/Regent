//! Proves the semantic (vector) lane recalls memory the lexical (FTS) lane
//! alone cannot — the concrete "superior recall" claim of tri-modal memory.
//! Uses a controllable map-embedder so the assertion is deterministic and
//! offline (the real model is exercised by regent-embed's ignored test).

use regent_graph::{GraphMemory, Provenance};
use regent_kernel::{EmbeddingProvider, RegentError};
use regent_store::Store;
use std::collections::HashMap;
use std::sync::Arc;

/// Returns a fixed vector per known text, zeros otherwise — lets a test pin a
/// query's embedding to a node's embedding with zero lexical overlap.
struct MapEmbedder {
    map: HashMap<String, Vec<f32>>,
    dim: usize,
}

impl EmbeddingProvider for MapEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, RegentError> {
        Ok(texts
            .iter()
            .map(|t| self.map.get(t).cloned().unwrap_or_else(|| vec![0.0; self.dim]))
            .collect())
    }
    fn model_id(&self) -> &str {
        "map-v1"
    }
    fn dim(&self) -> usize {
        self.dim
    }
}

fn embedder() -> Arc<MapEmbedder> {
    let mut map = HashMap::new();
    map.insert("alpha apple".to_owned(), vec![1.0, 0.0]);
    map.insert("beta banana".to_owned(), vec![0.0, 1.0]);
    // The query: semantically aligned with "alpha apple" but no shared words.
    map.insert("zzz".to_owned(), vec![1.0, 0.0]);
    Arc::new(MapEmbedder { map, dim: 2 })
}

#[test]
fn vector_lane_recalls_a_node_fts_would_miss() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let mem = GraphMemory::new(Arc::clone(&store)).with_embedder(embedder());
    mem.add_node("fact", "a", "alpha apple", Provenance::UserStated, None, None).unwrap();
    mem.add_node("fact", "b", "beta banana", Provenance::UserStated, None, None).unwrap();

    // "zzz" lexically matches nothing; the vector lane must surface node "a".
    let hits = mem.retrieve("zzz", 5).unwrap();
    assert!(!hits.is_empty(), "vector lane should recall a node FTS misses");
    assert_eq!(hits[0].node.name, "a", "the semantically-closest node ranks first");
}

#[test]
fn without_embedder_a_lexically_unrelated_query_recalls_nothing() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let mem = GraphMemory::new(store); // no embedder → FTS + graph only
    mem.add_node("fact", "a", "alpha apple", Provenance::UserStated, None, None).unwrap();

    let hits = mem.retrieve("zzz", 5).unwrap();
    assert!(hits.is_empty(), "FTS-only cannot recall a lexically-unrelated query");
}

#[test]
fn lexical_query_still_works_with_the_vector_lane_attached() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let mem = GraphMemory::new(Arc::clone(&store)).with_embedder(embedder());
    mem.add_node("fact", "a", "alpha apple", Provenance::UserStated, None, None).unwrap();
    mem.add_node("fact", "b", "beta banana", Provenance::UserStated, None, None).unwrap();

    // A lexical hit is preserved (and reinforced) — fusion never regresses FTS.
    let hits = mem.retrieve("apple", 5).unwrap();
    assert_eq!(hits[0].node.name, "a");
}
