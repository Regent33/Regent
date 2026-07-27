//! Memory-surface queries: the pending-write approval queue and the
//! committed-graph accessors. Split from `queries.rs` (file-size rule).

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_kernel::{RegentError, SessionId};
use regent_store::PendingWriteRow;

impl SessionManager {
    /// W3 step 5: `text` with the memory canary's note prepended, or `text`
    /// unchanged — which is what happens on an ordinary turn, and always when
    /// the flag is off.
    ///
    /// The dedup runs against the session's **live memory segment**, read fresh
    /// here rather than snapshotted at build, so a profile escalation (which
    /// rebuilds the prompt) cannot leave the canary deduping against a block
    /// the model is no longer being shown.
    ///
    /// Borrows in every case but the one that injects, so the flag being off
    /// costs nothing — not even a copy of a long pasted turn.
    pub(super) async fn canary_note<'a>(
        &self,
        id: &SessionId,
        text: &'a str,
    ) -> std::borrow::Cow<'a, str> {
        use std::borrow::Cow;
        if !crate::application::memory_canary::enabled() {
            return Cow::Borrowed(text);
        }
        let Some((ledger, seen)) = self.entries.lock().await.get(id).map(|e| {
            (
                std::sync::Arc::clone(&e.ledger),
                std::sync::Arc::clone(&e.canary_seen),
            )
        }) else {
            return Cow::Borrowed(text);
        };
        // Dedup against the MEMORY SEGMENT, not the whole system prompt: a
        // phrase that also occurs in the constitution or the skills index is
        // not the same as the memory entry being present [co-audit]. `rebase`
        // (resume) collapses the segments, so fall back to the full render —
        // over-broad in that one case, which errs toward injecting less.
        let block = ledger
            .segments()
            .iter()
            .find(|s| s.name == "memory")
            .map_or_else(|| ledger.render(), |s| s.text.clone());
        let note = {
            let Ok(mut seen) = seen.lock() else {
                return Cow::Borrowed(text);
            };
            crate::application::memory_canary::recall_note(&self.graph, &block, &mut seen, text)
        };
        match note {
            Some(note) => Cow::Owned(format!("{note}\n\n{text}")),
            None => Cow::Borrowed(text),
        }
    }

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
