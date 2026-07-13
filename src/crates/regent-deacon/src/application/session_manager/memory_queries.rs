//! Memory-surface queries: the pending-write approval queue and the
//! committed-graph accessors. Split from `queries.rs` (file-size rule).

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_kernel::RegentError;
use regent_store::PendingWriteRow;

impl SessionManager {
    pub fn pending_memory_writes(&self, limit: u32) -> Result<Vec<PendingWriteRow>, DeaconError> {
        self.graph
            .pending_writes(limit)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn approve_memory_write(&self, id: &str) -> Result<Option<String>, DeaconError> {
        // SPL P5 (§3.6): a Distiller persona rewrite commits through the
        // BUDGETED persona path — never the graph-node path — and the old
        // content is backed up into graph memory first, so a bulk rewrite is
        // a relocation (retrievable via memory_search), never a loss.
        let is_persona_rewrite = self
            .store
            .list_pending_writes(500)
            .ok()
            .into_iter()
            .flatten()
            .any(|w| w.id == id && w.kind == crate::application::distiller::PERSONA_REWRITE_KIND);
        if is_persona_rewrite {
            let Some(row) = self
                .store
                .take_pending_write(id)
                .map_err(DeaconError::Store)?
            else {
                return Ok(None);
            };
            let old = self
                .store
                .get_persona(&row.name)
                .map_err(DeaconError::Store)?;
            if !old.trim().is_empty() {
                // Backup rides a non-rendering persona row (DB — personas never
                // live in plaintext files; graph nodes cap at 2k chars). One
                // backup per store, overwritten per distill; unbudgeted because
                // pre-rewrite content is exactly what can exceed the budget.
                self.store
                    .set_persona_unbudgeted(&format!("backup.{}", row.name), &old)
                    .map_err(DeaconError::Store)?;
            }
            self.store
                .set_persona(&row.name, &row.content)
                .map_err(DeaconError::Store)?;
            return Ok(Some(format!("persona:{}", row.name)));
        }
        self.graph
            .approve_write(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn reject_memory_write(&self, id: &str) -> Result<bool, DeaconError> {
        self.graph
            .reject_write(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    /// Auto-rejects writes whose approval TTL elapsed; returns how many.
    pub fn expire_memory_writes(&self) -> Result<usize, DeaconError> {
        self.graph
            .expire_pending_writes()
            .map(|expired| expired.len())
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    // ── Committed-memory lifecycle (`memory list/pin/unpin/forget`) ─────────

    pub fn list_memory(&self, limit: u32) -> Result<Vec<regent_graph::MemoryNode>, DeaconError> {
        self.graph
            .recent_nodes(limit)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    /// Full knowledge-graph dump (nodes + edges) for the visualization page.
    /// Derived edges (cosine top-k + episode/session links) are rebuilt first —
    /// nothing in the write path links nodes, so without this the page shows an
    /// unconnected starfield. Best-effort: a rebuild failure never blocks the dump.
    pub fn memory_graph(&self, limit: u32) -> Result<regent_graph::MemoryGraph, DeaconError> {
        if let Err(error) = self.graph.rebuild_derived_edges(3) {
            tracing::warn!(%error, "derived-edge rebuild failed; dumping existing edges");
        }
        self.graph
            .graph_dump(limit)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn pin_memory(&self, id: &str) -> Result<bool, DeaconError> {
        self.graph
            .pin(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn unpin_memory(&self, id: &str) -> Result<bool, DeaconError> {
        self.graph
            .unpin(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn forget_memory(&self, id: &str) -> Result<bool, DeaconError> {
        self.graph
            .forget(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }
}
