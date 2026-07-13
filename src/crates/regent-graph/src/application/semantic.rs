//! The semantic lane of [`GraphMemory`]: embedding writes/backfill and the
//! derived-edge rebuild. Split from `orchestrators.rs` (file-size rule) —
//! same type, extension `impl` block.

use super::orchestrators::GraphMemory;
use crate::domain::entities::Provenance;
use crate::domain::errors::GraphError;

impl GraphMemory {
    /// Generates and persists a node's embedding. Best-effort: a transient
    /// embedder error never loses a memory write.
    pub(super) fn embed_and_store(&self, node_id: &str, content: &str) {
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
#[path = "semantic_tests.rs"]
mod embedding_tests;
