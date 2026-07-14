//! Edge persistence + graph-neighborhood queries. Split from `graph.rs`
//! (file-size rule) — extension impl on the same Store.

use crate::domain::entities::{EdgeRow, NeighborRow, NodeRow};
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::params;

impl Store {
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
}
