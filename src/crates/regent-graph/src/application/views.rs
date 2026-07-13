//! Read-model views of [`GraphMemory`]: the node list and the full graph
//! dump. Split from `orchestrators.rs` (file-size rule); the types re-export
//! from there so `orchestrators::MemoryNode` paths stay valid.

use super::orchestrators::GraphMemory;
use crate::domain::errors::GraphError;

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

impl GraphMemory {
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
}
