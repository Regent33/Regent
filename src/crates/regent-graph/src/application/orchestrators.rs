//! The memory orchestrator — owns provenance trust, write-through node
//! ingestion, linking, episode capture, and TTL purge. Bounded-store and
//! retrieval use cases extend this type from their own modules.

use crate::domain::entities::{MemoryTarget, Provenance};
use crate::domain::errors::GraphError;
use crate::domain::policy;
use regent_kernel::EmbeddingProvider;
use regent_store::{NodeRow, Store, now_epoch};
use std::sync::{Arc, OnceLock};

/// A committed memory node, surfaced by `memory list`.
pub struct MemoryNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub content: String,
    pub pinned: bool,
}

/// A graph edge, surfaced by the full-graph dump (`memory.graph`).
pub struct MemoryEdge {
    pub src: String,
    pub dst: String,
    pub relation: String,
    pub weight: f64,
}

/// The full knowledge-graph dump: the most-recently-updated nodes plus every
/// edge whose endpoints both fall within that node set.
pub struct MemoryGraph {
    pub nodes: Vec<MemoryNode>,
    pub edges: Vec<MemoryEdge>,
}

/// Default TTL re-applied on `unpin` (90 days), making the node purge-eligible again.
const DEFAULT_NODE_TTL_SECS: f64 = 90.0 * 24.0 * 3600.0;

pub struct GraphMemory {
    pub(crate) store: Arc<Store>,
    pub(crate) memory_budget: usize,
    pub(crate) user_budget: usize,
    /// Optional semantic lane, bound once (late-bindable so a long model load
    /// never blocks deacon boot — see `attach_embedder`). When present, node
    /// writes are embedded so retrieval fuses vector recall with FTS + graph;
    /// absent → memory still works on FTS + graph alone.
    pub(crate) embedder: Arc<OnceLock<Arc<dyn EmbeddingProvider>>>,
}

impl GraphMemory {
    /// Default budgets: memory 2,200 chars (~800 tok), user 1,375.
    #[must_use]
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            memory_budget: 2_200,
            user_budget: 1_375,
            embedder: Arc::new(OnceLock::new()),
        }
    }

    #[must_use]
    pub fn with_budgets(mut self, memory: usize, user: usize) -> Self {
        self.memory_budget = memory;
        self.user_budget = user;
        self
    }

    /// Attaches the semantic lane at construction (sync callers / tests).
    #[must_use]
    pub fn with_embedder(self, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        let _ = self.embedder.set(embedder);
        self
    }

    /// Binds the semantic lane after construction — lets the deacon serve
    /// immediately and attach the model from a background task once loaded.
    /// No-op if an embedder is already bound.
    pub fn attach_embedder(&self, embedder: Arc<dyn EmbeddingProvider>) {
        let _ = self.embedder.set(embedder);
    }

    pub(crate) fn budget(&self, target: MemoryTarget) -> usize {
        match target {
            MemoryTarget::Memory => self.memory_budget,
            MemoryTarget::User => self.user_budget,
        }
    }

    /// Generic node write (semantic facts, entities, intents). Validated,
    /// deduplicated by content hash; returns the node id (existing id when
    /// the write was a duplicate).
    pub fn add_node(
        &self,
        kind: &str,
        name: &str,
        content: &str,
        provenance: Provenance,
        session_id: Option<&str>,
        ttl_secs: Option<f64>,
    ) -> Result<String, GraphError> {
        policy::validate_content(content)?;
        let now = now_epoch();
        let node = NodeRow {
            id: new_node_id(),
            kind: kind.to_owned(),
            name: name.to_owned(),
            content: content.to_owned(),
            provenance: provenance.as_str().to_owned(),
            trust: provenance.trust(),
            session_id: session_id.map(ToOwned::to_owned),
            created_at: now,
            updated_at: now,
            ttl_expires_at: ttl_secs.map(|secs| now + secs),
            access_count: 0,
            content_hash: policy::content_hash(kind, name, content),
        };
        let inserted = self.store.insert_node(&node)?;
        if inserted {
            // Embed the fresh node; failure is non-fatal — the node is stored
            // and `backfill_embeddings` will retry it later.
            self.embed_and_store(&node.id, content);
            Ok(node.id)
        } else {
            // Idempotent ingestion: hand back the existing node's id.
            self.find_by_hash(&node.content_hash)
        }
    }

    /// Generates and persists a node's embedding. Best-effort: a transient
    /// embedder error never loses a memory write.
    fn embed_and_store(&self, node_id: &str, content: &str) {
        let Some(embedder) = self.embedder.get() else {
            return;
        };
        match embedder.embed(&[content.to_owned()]) {
            Ok(vectors) => {
                if let Some(vector) = vectors.first()
                    && let Err(error) =
                        self.store
                            .upsert_embedding(node_id, embedder.model_id(), vector)
                {
                    tracing::warn!(%error, node_id, "embedding persist failed; backfill will retry");
                }
            }
            Err(error) => {
                tracing::warn!(%error, node_id, "embedding generation failed; backfill will retry");
            }
        }
    }

    /// Embeds up to `batch` nodes that have no vector for the active model —
    /// the startup/background catch-up path for memory written before an
    /// embedder was attached (or while it was failing). Returns how many were
    /// embedded.
    pub fn backfill_embeddings(&self, batch: u32) -> Result<usize, GraphError> {
        let Some(embedder) = self.embedder.get() else {
            return Ok(0);
        };
        let pending = self
            .store
            .nodes_needing_embedding(embedder.model_id(), batch)?;
        if pending.is_empty() {
            return Ok(0);
        }
        let texts: Vec<String> = pending.iter().map(|(_, content)| content.clone()).collect();
        let vectors = embedder
            .embed(&texts)
            .map_err(|e| GraphError::Embedding(e.to_string()))?;
        let mut embedded = 0;
        for ((node_id, _), vector) in pending.iter().zip(vectors.iter()) {
            self.store
                .upsert_embedding(node_id, embedder.model_id(), vector)?;
            embedded += 1;
        }
        Ok(embedded)
    }

    pub fn link(
        &self,
        src_id: &str,
        dst_id: &str,
        relation: &str,
        weight: f64,
        provenance: Provenance,
    ) -> Result<(), GraphError> {
        self.store
            .upsert_edge(src_id, dst_id, relation, weight, provenance.as_str())?;
        Ok(())
    }

    /// One summary node per compressed/ended session — the episodic anchor.
    pub fn record_episode(&self, session_id: &str, summary: &str) -> Result<String, GraphError> {
        self.add_node(
            "episode",
            &format!("episode:{session_id}"),
            summary,
            Provenance::AgentInferred,
            Some(session_id),
            None,
        )
    }

    pub fn purge_expired(&self) -> Result<usize, GraphError> {
        Ok(self.store.purge_expired_nodes()?)
    }

    /// Pin a node: clear its TTL so the purge loop never reclaims it.
    /// Returns false when no node matched the id.
    pub fn pin(&self, id: &str) -> Result<bool, GraphError> {
        Ok(self.store.set_node_ttl(id, None)?)
    }

    /// Unpin: re-apply a default TTL from now, making the node purge-eligible again.
    pub fn unpin(&self, id: &str) -> Result<bool, GraphError> {
        let ttl = now_epoch() + DEFAULT_NODE_TTL_SECS;
        Ok(self.store.set_node_ttl(id, Some(ttl))?)
    }

    /// Forget a node outright (and its edges). Returns false when it didn't exist.
    pub fn forget(&self, id: &str) -> Result<bool, GraphError> {
        if self.store.find_node(id)?.is_none() {
            return Ok(false);
        }
        self.store.delete_node(id)?;
        Ok(true)
    }

    /// Recent committed memory nodes for `memory list` (pinned = no TTL).
    pub fn recent_nodes(&self, limit: u32) -> Result<Vec<MemoryNode>, GraphError> {
        Ok(self
            .store
            .recent_nodes(limit)?
            .into_iter()
            .map(|n| MemoryNode {
                pinned: n.ttl_expires_at.is_none(),
                id: n.id,
                kind: n.kind,
                name: n.name,
                content: n.content,
            })
            .collect())
    }

    /// Rebuilds the DERIVED edge set — nothing in the write path links nodes,
    /// so without this the graph page renders an unconnected starfield:
    /// - `similar_to`: each node's top-`k` cosine neighbors over the stored
    ///   embeddings (weight = similarity), canonical src<dst so a pair links once;
    /// - `from_session`: episode summary nodes → the nodes born in their session.
    ///   Swept and rebuilt in full each call so stale pairs never linger.
    ///   ponytail: O(n²) pairwise cosine — fine to ~5k nodes, ANN after that.
    pub fn rebuild_derived_edges(&self, k: usize) -> Result<usize, GraphError> {
        self.store.delete_edges_with_relation("similar_to")?;
        self.store.delete_edges_with_relation("from_session")?;
        let embeddings = self.store.all_embeddings()?;
        let mut added = 0;
        for (id, vector) in &embeddings {
            let mut scored: Vec<(&String, f64)> = embeddings
                .iter()
                .filter(|(other, v)| other != id && v.len() == vector.len())
                .map(|(other, v)| (other, f64::from(cosine(vector, v))))
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (other, sim) in scored.into_iter().take(k) {
                if sim <= 0.0 {
                    break;
                }
                let (src, dst) = if id < other { (id, other) } else { (other, id) };
                self.store.upsert_edge(
                    src,
                    dst,
                    "similar_to",
                    sim,
                    Provenance::AgentInferred.as_str(),
                )?;
                added += 1;
            }
        }
        // Episodes anchor their session's nodes.
        let rows = self.store.list_nodes(1_000)?;
        for episode in rows.iter().filter(|n| n.kind == "episode") {
            let Some(sid) = &episode.session_id else {
                continue;
            };
            for node in rows
                .iter()
                .filter(|n| n.id != episode.id && n.session_id.as_ref() == Some(sid))
            {
                self.store.upsert_edge(
                    &episode.id,
                    &node.id,
                    "from_session",
                    1.0,
                    Provenance::AgentInferred.as_str(),
                )?;
                added += 1;
            }
        }
        Ok(added)
    }

    /// Full knowledge-graph dump for the visualization page: the most-recently
    /// updated nodes (capped at `limit`) plus every edge whose `src` and `dst`
    /// are both within that node set (`pinned` = no TTL, as in `memory list`).
    pub fn graph_dump(&self, limit: u32) -> Result<MemoryGraph, GraphError> {
        let rows = self.store.list_nodes(limit)?;
        let ids: Vec<String> = rows.iter().map(|n| n.id.clone()).collect();
        let edges = self
            .store
            .list_edges_among(&ids)?
            .into_iter()
            .map(|e| MemoryEdge {
                src: e.src,
                dst: e.dst,
                relation: e.relation,
                weight: e.weight,
            })
            .collect();
        let nodes = rows
            .into_iter()
            .map(|n| MemoryNode {
                pinned: n.ttl_expires_at.is_none(),
                id: n.id,
                kind: n.kind,
                name: n.name,
                content: n.content,
            })
            .collect();
        Ok(MemoryGraph { nodes, edges })
    }

    fn find_by_hash(&self, hash: &str) -> Result<String, GraphError> {
        // Hash collisions across kinds are prevented by hashing kind+name+content.
        for kind in [
            "memory",
            "user",
            "entity",
            "fact",
            "episode",
            "intent",
            "constitution",
        ] {
            if let Some(node) = self
                .store
                .nodes_by_kind(kind)?
                .into_iter()
                .find(|n| n.content_hash == hash)
            {
                return Ok(node.id);
            }
        }
        Err(GraphError::Rejected(
            "duplicate node vanished during lookup".into(),
        ))
    }
}

pub(crate) fn new_node_id() -> String {
    format!("node_{}", uuid::Uuid::new_v4().simple())
}

/// Cosine similarity (the store's helper is private to its module).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod embedding_tests {
    use super::*;
    use regent_kernel::RegentError;

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
}
