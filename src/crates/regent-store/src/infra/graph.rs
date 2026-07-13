//! Graph persistence primitives (dumb CRUD + FTS). All memory *semantics* —
//! budgets, provenance trust, retrieval scoring, write policy — live in the
//! `regent-graph` crate; this module only moves rows.

use crate::domain::entities::{EdgeRow, NeighborRow, NodeRow};
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use crate::infra::search::sanitize_fts5_query;
use rusqlite::{OptionalExtension, params};

const NODE_COLUMNS: &str = "id, kind, name, content, provenance, trust, session_id, \
                            created_at, updated_at, ttl_expires_at, access_count, content_hash";

fn row_to_node(row: &rusqlite::Row<'_>) -> Result<NodeRow, rusqlite::Error> {
    Ok(NodeRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        content: row.get(3)?,
        provenance: row.get(4)?,
        trust: row.get(5)?,
        session_id: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        ttl_expires_at: row.get(9)?,
        access_count: row.get(10)?,
        content_hash: row.get(11)?,
    })
}

impl Store {
    /// Inserts a node. Returns false (no-op) when an identical
    /// `content_hash` already exists — idempotent ingestion.
    pub fn insert_node(&self, node: &NodeRow) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let changed = tx.execute(
                "INSERT OR IGNORE INTO nodes
                 (id, kind, name, content, provenance, trust, session_id,
                  created_at, updated_at, ttl_expires_at, access_count, content_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    node.id,
                    node.kind,
                    node.name,
                    node.content,
                    node.provenance,
                    node.trust,
                    node.session_id,
                    node.created_at,
                    node.updated_at,
                    node.ttl_expires_at,
                    node.access_count,
                    node.content_hash,
                ],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn find_node(&self, id: &str) -> Result<Option<NodeRow>, StoreError> {
        self.with_read(|conn| {
            conn.query_row(
                &format!("SELECT {NODE_COLUMNS} FROM nodes WHERE id = ?1"),
                params![id],
                row_to_node,
            )
            .optional()
        })
    }

    pub fn nodes_by_kind(&self, kind: &str) -> Result<Vec<NodeRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {NODE_COLUMNS} FROM nodes WHERE kind = ?1 ORDER BY created_at, rowid"
            ))?;
            let rows = stmt.query_map(params![kind], row_to_node)?;
            rows.collect()
        })
    }

    pub fn update_node_content(
        &self,
        id: &str,
        content: &str,
        content_hash: &str,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE nodes SET content = ?1, content_hash = ?2, updated_at = ?3 WHERE id = ?4",
                params![content, content_hash, now_epoch(), id],
            )?;
            Ok(())
        })
    }

    /// Deletes a node and every edge touching it.
    pub fn delete_node(&self, id: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute("DELETE FROM edges WHERE src = ?1 OR dst = ?1", params![id])?;
            tx.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    /// Sets (or clears) a node's TTL. `None` pins it — exempt from the purge
    /// loop. Returns false when no node matched the id.
    pub fn set_node_ttl(&self, id: &str, ttl_expires_at: Option<f64>) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let changed = tx.execute(
                "UPDATE nodes SET ttl_expires_at = ?1, updated_at = ?2 WHERE id = ?3",
                params![ttl_expires_at, now_epoch(), id],
            )?;
            Ok(changed > 0)
        })
    }

    /// Most-recently-created nodes, newest first (for `memory list`).
    pub fn recent_nodes(&self, limit: u32) -> Result<Vec<NodeRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {NODE_COLUMNS} FROM nodes ORDER BY created_at DESC, rowid DESC LIMIT ?1"
            ))?;
            let rows = stmt.query_map(params![limit], row_to_node)?;
            rows.collect()
        })
    }

    /// All node rows, most-recently-updated first — the full-graph dump
    /// (`memory.graph`), capped at `limit`.
    pub fn list_nodes(&self, limit: u32) -> Result<Vec<NodeRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {NODE_COLUMNS} FROM nodes ORDER BY updated_at DESC, rowid DESC LIMIT ?1"
            ))?;
            let rows = stmt.query_map(params![limit], row_to_node)?;
            rows.collect()
        })
    }

    /// Edges whose `src` AND `dst` both fall within `ids` — the connected
    /// subgraph over a selected node set (pairs with [`Store::list_nodes`]).
    /// Strongest edges first. Empty `ids` → no edges.
    pub fn list_edges_among(&self, ids: &[String]) -> Result<Vec<EdgeRow>, StoreError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.with_read(|conn| {
            let placeholders = vec!["?"; ids.len()].join(",");
            let mut stmt = conn.prepare(&format!(
                "SELECT src, dst, relation, weight FROM edges
                 WHERE src IN ({placeholders}) AND dst IN ({placeholders})
                 ORDER BY weight DESC"
            ))?;
            // Positional params: the id list once for `src IN`, again for `dst IN`.
            let bind: Vec<&dyn rusqlite::ToSql> =
                ids.iter().chain(ids.iter()).map(|s| s as _).collect();
            let rows = stmt.query_map(bind.as_slice(), |row| {
                Ok(EdgeRow {
                    src: row.get(0)?,
                    dst: row.get(1)?,
                    relation: row.get(2)?,
                    weight: row.get(3)?,
                })
            })?;
            rows.collect()
        })
    }

    /// Inserts or refreshes an edge (last write wins on weight/provenance).
    pub fn upsert_edge(
        &self,
        src: &str,
        dst: &str,
        relation: &str,
        weight: f64,
        provenance: &str,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO edges (src, dst, relation, weight, provenance, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(src, dst, relation)
                 DO UPDATE SET weight = excluded.weight, provenance = excluded.provenance",
                params![src, dst, relation, weight, provenance, now_epoch()],
            )?;
            Ok(())
        })
    }

    /// Deletes every edge with `relation` — the rebuild-derived-edges sweep.
    pub fn delete_edges_with_relation(&self, relation: &str) -> Result<usize, StoreError> {
        self.with_write(|tx| {
            let n = tx.execute("DELETE FROM edges WHERE relation = ?1", params![relation])?;
            Ok(n)
        })
    }

    /// Neighbors in both directions, strongest edges first.
    pub fn neighbors(&self, id: &str, limit: u32) -> Result<Vec<NeighborRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT e.relation, e.weight,
                        n.id, n.kind, n.name, n.content, n.provenance, n.trust, n.session_id,
                        n.created_at, n.updated_at, n.ttl_expires_at, n.access_count,
                        n.content_hash
                 FROM edges e
                 JOIN nodes n ON n.id = CASE WHEN e.src = ?1 THEN e.dst ELSE e.src END
                 WHERE e.src = ?1 OR e.dst = ?1
                 ORDER BY e.weight DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![id, limit], |row| {
                Ok(NeighborRow {
                    relation: row.get(0)?,
                    weight: row.get(1)?,
                    node: NodeRow {
                        id: row.get(2)?,
                        kind: row.get(3)?,
                        name: row.get(4)?,
                        content: row.get(5)?,
                        provenance: row.get(6)?,
                        trust: row.get(7)?,
                        session_id: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                        ttl_expires_at: row.get(11)?,
                        access_count: row.get(12)?,
                        content_hash: row.get(13)?,
                    },
                })
            })?;
            rows.collect()
        })
    }

    /// FTS5 match over node name+content; returns ids best-rank-first.
    pub fn fts_nodes(&self, query: &str, limit: u32) -> Result<Vec<String>, StoreError> {
        let sanitized = sanitize_fts5_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT n.id FROM nodes_fts JOIN nodes n ON n.rowid = nodes_fts.rowid
                 WHERE nodes_fts MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![sanitized, limit], |row| row.get(0))?;
            rows.collect()
        })
    }

    /// Bumps access telemetry for retrieved nodes (feeds TTL/curation).
    pub fn touch_nodes(&self, ids: &[String]) -> Result<(), StoreError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.with_write(|tx| {
            let now = now_epoch();
            for id in ids {
                tx.execute(
                    "UPDATE nodes SET access_count = access_count + 1, last_accessed_at = ?1
                     WHERE id = ?2",
                    params![now, id],
                )?;
            }
            Ok(())
        })
    }

    /// Removes nodes whose TTL has passed. Returns how many were purged.
    pub fn purge_expired_nodes(&self) -> Result<usize, StoreError> {
        self.with_write(|tx| {
            let now = now_epoch();
            tx.execute(
                "DELETE FROM edges WHERE src IN (SELECT id FROM nodes WHERE ttl_expires_at < ?1)
                                     OR dst IN (SELECT id FROM nodes WHERE ttl_expires_at < ?1)",
                params![now],
            )?;
            let purged = tx.execute("DELETE FROM nodes WHERE ttl_expires_at < ?1", params![now])?;
            Ok(purged)
        })
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
