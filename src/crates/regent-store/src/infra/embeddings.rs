//! Vector lane persistence (the semantic seed source for tri-modal recall).
//! Embeddings are stored as little-endian f32 BLOBs keyed by `model_id`;
//! search is brute-force cosine in Rust — at personal-agent scale (thousands
//! of nodes) this is sub-millisecond and needs no C ANN extension. All vector
//! *semantics* (which model, when to embed, fusion weights) live in
//! `regent-graph`; this module only moves rows and ranks by cosine.

use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::params;
use std::cmp::Ordering;

impl Store {
    /// Stores (or replaces) a node's embedding for `model_id`.
    pub fn upsert_embedding(
        &self,
        node_id: &str,
        model_id: &str,
        vector: &[f32],
    ) -> Result<(), StoreError> {
        let blob = vec_to_blob(vector);
        let dim = i64::try_from(vector.len()).unwrap_or(0);
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO node_embeddings (node_id, model_id, dim, vector, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(node_id) DO UPDATE SET
                     model_id = excluded.model_id, dim = excluded.dim,
                     vector = excluded.vector, created_at = excluded.created_at",
                params![node_id, model_id, dim, blob, now_epoch()],
            )?;
            Ok(())
        })
    }

    /// Brute-force cosine search: returns `(node_id, similarity)` best-first.
    /// Vectors whose dimension differs from the query (e.g. left over from a
    /// previous embedding model) are skipped, never silently mis-ranked.
    pub fn vector_search(
        &self,
        query: &[f32],
        model_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, f32)>, StoreError> {
        let query_norm = norm(query);
        if query_norm == 0.0 {
            return Ok(Vec::new());
        }
        // Pull rows under the lock; score outside it to keep the hold brief.
        let raw: Vec<(String, Vec<u8>)> = self.with_read(|conn| {
            let mut stmt =
                conn.prepare("SELECT node_id, vector FROM node_embeddings WHERE model_id = ?1")?;
            stmt.query_map(params![model_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?
            .collect()
        })?;

        let mut scored: Vec<(String, f32)> = raw
            .iter()
            .filter_map(|(id, blob)| {
                let v = blob_to_vec(blob);
                (v.len() == query.len()).then(|| (id.clone(), cosine(query, query_norm, &v)))
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }

    /// Nodes with no embedding for `model_id` yet — the backfill work list.
    /// Returns `(node_id, content)` oldest-first.
    pub fn nodes_needing_embedding(
        &self,
        model_id: &str,
        limit: u32,
    ) -> Result<Vec<(String, String)>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT n.id, n.content FROM nodes n
                 LEFT JOIN node_embeddings e ON e.node_id = n.id AND e.model_id = ?1
                 WHERE e.node_id IS NULL ORDER BY n.created_at, n.rowid LIMIT ?2",
            )?;
            stmt.query_map(params![model_id, limit], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect()
        })
    }

    /// Every stored embedding as `(node_id, vector)` — the pairwise-similarity
    /// work list (derived graph edges). Callers must compare only same-length
    /// vectors; mixed dimensions mean a model change mid-store.
    pub fn all_embeddings(&self) -> Result<Vec<(String, Vec<f32>)>, StoreError> {
        let raw: Vec<(String, Vec<u8>)> = self.with_read(|conn| {
            let mut stmt = conn.prepare("SELECT node_id, vector FROM node_embeddings")?;
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?
            .collect()
        })?;
        Ok(raw
            .into_iter()
            .map(|(id, blob)| (id, blob_to_vec(&blob)))
            .collect())
    }

    /// Count of stored embeddings for `model_id` (diagnostics / tests).
    pub fn embedding_count(&self, model_id: &str) -> Result<usize, StoreError> {
        self.with_read(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM node_embeddings WHERE model_id = ?1",
                params![model_id],
                |r| r.get(0),
            )?;
            Ok(usize::try_from(n).unwrap_or(0))
        })
    }
}

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Cosine similarity with the query norm precomputed (the query is reused
/// across every candidate, so its norm is computed once).
fn cosine(query: &[f32], query_norm: f32, v: &[f32]) -> f32 {
    let dot: f32 = query.iter().zip(v).map(|(a, b)| a * b).sum();
    let vn = norm(v);
    if vn == 0.0 {
        0.0
    } else {
        dot / (query_norm * vn)
    }
}

#[cfg(test)]
#[path = "embeddings_tests.rs"]
mod tests;
