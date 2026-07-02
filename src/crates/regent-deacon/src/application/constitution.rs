//! Constitution sync — the boot-time use case behind config.yaml
//! `constitution.enabled` (ADR-028). Enabled: the always-on CORE goes in the
//! `constitution` persona row and the FULL document is ingested into graph
//! memory as pinned, trusted section nodes (retrieved tri-modally via
//! `memory_search` — ADR-013). Disabled: both are removed — but a user-edited
//! persona row is never touched.

use regent_agent::{constitution_chunks, constitution_core, constitution_text};
use regent_graph::{GraphMemory, Provenance};
use regent_store::{Store, StoreError};

/// The agent name filled into the shipped document's placeholder.
const AGENT_NAME: &str = "Regent";

pub fn sync_constitution(
    enabled: bool,
    store: &Store,
    graph: &GraphMemory,
) -> Result<(), StoreError> {
    let core = constitution_core(AGENT_NAME);
    let full = constitution_text(AGENT_NAME);
    let row = store.get_persona("constitution")?;
    if enabled {
        // Seed the core — also upgrading a full-document row from before
        // vectorization. A user-edited row is left alone.
        if row.trim().is_empty() || row == full {
            store.set_persona("constitution", &core)?;
            tracing::info!("constitution enabled — core seeded into the persona row");
        }
        let chunks = constitution_chunks();
        // Reconcile: drop nodes that no longer match the shipped document
        // (stale after a document update), then (re-)ingest — `add_node`
        // dedups by content hash, so re-running is a no-op.
        for node in store.nodes_by_kind("constitution")? {
            let shipped = chunks
                .iter()
                .any(|(name, content)| *name == node.name && *content == node.content);
            if !shipped && let Err(error) = graph.forget(&node.id) {
                tracing::warn!(%error, node = node.name, "stale constitution node not removed");
            }
        }
        for (name, content) in &chunks {
            // Pinned (no TTL) + user-stated trust; embed-on-write / backfill
            // gives them the vector lane.
            if let Err(error) = graph.add_node(
                "constitution",
                name,
                content,
                Provenance::UserStated,
                None,
                None,
            ) {
                tracing::warn!(%error, node = %name, "constitution section not ingested");
            }
        }
    } else {
        // Clear only a row we wrote (either shipped shape) — user edits stay.
        if row == core || row == full {
            store.set_persona("constitution", "")?;
        }
        for node in store.nodes_by_kind("constitution")? {
            if let Err(error) = graph.forget(&node.id) {
                tracing::warn!(%error, node = node.name, "constitution node not removed");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn setup() -> (Arc<Store>, GraphMemory) {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let graph = GraphMemory::new(Arc::clone(&store));
        (store, graph)
    }

    #[test]
    fn enable_seeds_core_row_and_section_nodes_idempotently() {
        let (store, graph) = setup();
        sync_constitution(true, &store, &graph).unwrap();
        assert_eq!(
            store.get_persona("constitution").unwrap(),
            constitution_core(AGENT_NAME)
        );
        let count = store.nodes_by_kind("constitution").unwrap().len();
        assert!(count >= 16, "one or more nodes per section, got {count}");
        // Re-running changes nothing (hash dedup + reconcile).
        sync_constitution(true, &store, &graph).unwrap();
        assert_eq!(store.nodes_by_kind("constitution").unwrap().len(), count);
    }

    #[test]
    fn disable_clears_shipped_row_and_nodes_but_keeps_user_edits() {
        let (store, graph) = setup();
        sync_constitution(true, &store, &graph).unwrap();
        sync_constitution(false, &store, &graph).unwrap();
        assert_eq!(store.get_persona("constitution").unwrap(), "");
        assert!(store.nodes_by_kind("constitution").unwrap().is_empty());

        // A user-edited row survives a later disable; nodes still go.
        store.set_persona("constitution", "my own creed").unwrap();
        sync_constitution(false, &store, &graph).unwrap();
        assert_eq!(store.get_persona("constitution").unwrap(), "my own creed");
    }

    #[test]
    fn enable_upgrades_a_full_document_row_but_not_a_user_edit() {
        let (store, graph) = setup();
        // Pre-vectorization installs stored the full document.
        store
            .set_persona("constitution", &constitution_text(AGENT_NAME))
            .unwrap();
        sync_constitution(true, &store, &graph).unwrap();
        assert_eq!(
            store.get_persona("constitution").unwrap(),
            constitution_core(AGENT_NAME)
        );

        store.set_persona("constitution", "my own creed").unwrap();
        sync_constitution(true, &store, &graph).unwrap();
        assert_eq!(store.get_persona("constitution").unwrap(), "my own creed");
    }

    #[test]
    fn stale_nodes_from_an_older_document_are_reconciled_away() {
        let (store, graph) = setup();
        graph
            .add_node(
                "constitution",
                "constitution:99-old",
                "[Constitution §99 — Old] gone",
                Provenance::UserStated,
                None,
                None,
            )
            .unwrap();
        sync_constitution(true, &store, &graph).unwrap();
        let names: Vec<String> = store
            .nodes_by_kind("constitution")
            .unwrap()
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert!(
            !names.iter().any(|n| n == "constitution:99-old"),
            "stale node kept"
        );
    }
}
