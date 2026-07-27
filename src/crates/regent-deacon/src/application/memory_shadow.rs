//! Memory measurement (W3 steps 1–2). **Injects nothing and changes nothing.**
//!
//! Today the whole memory corpus is injected on every turn, whatever the
//! question — the per-turn path is byte-identical to the system Regent was
//! measured against, and the live store sits at 75% of its ceiling with six
//! entries. The plan's rule is that nothing may be narrowed until automatic
//! retrieval is shown to cover what narrowing would remove.
//!
//! This is the evidence-gathering half, and it is deliberately inert:
//!
//! - **Step 1** records what the always-injected block costs per session.
//! - **Step 2** records what `retrieve` WOULD have returned for each real turn,
//!   including the candidate set — scoring only the selection cannot tell you
//!   what selection missed.
//!
//! Two properties this must not violate, or the data it collects is worthless:
//!
//! 1. **No side effects.** `shadow_recall` never touches nodes. `retrieve`
//!    does, feeding `access_count`; a shadow pass that touched would manufacture
//!    the exposure-feedback loop the plan warns about — injected entries accrue
//!    hits, and hits then read as relevance.
//! 2. **No latency.** It runs off the turn path entirely. Measurement that slows
//!    the thing it measures corrupts the baseline it exists to establish.
//!
//! Off by default; `REGENT_MEMORY_SHADOW=1` turns it on.

use regent_graph::GraphMemory;
use std::sync::Arc;

/// How many entries a shadow retrieval asks for. Matches `memory_search`'s
/// default so the log describes the retrieval Regent actually has.
const SHADOW_K: usize = 10;

/// Whether shadow measurement is enabled. Read per call so it can be turned on
/// without a restart.
#[must_use]
pub fn enabled() -> bool {
    std::env::var("REGENT_MEMORY_SHADOW")
        .map(|raw| matches!(raw.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Step 1: log what the always-injected block costs. Called once per session
/// build, where the block is frozen.
pub fn record_block_cost(graph: &GraphMemory) {
    if !enabled() {
        return;
    }
    match graph.block_metrics() {
        Ok(metrics) => {
            for m in metrics {
                tracing::info!(
                    target: "memory_shadow",
                    phase = "block",
                    store = %m.target.kind(),
                    entries = m.entries,
                    chars = m.chars,
                    limit = m.limit,
                    percent_full = m.percent_full(),
                    "always-injected block cost"
                );
            }
        }
        Err(error) => tracing::warn!(%error, "block metrics unavailable"),
    }
}

/// Step 2: log what retrieval WOULD have injected for this turn. Spawned, so
/// the turn never waits on it, and blocking (the graph is SQLite underneath).
pub fn record_would_inject(graph: &Arc<GraphMemory>, query: &str) {
    if !enabled() || query.trim().is_empty() {
        return;
    }
    let graph = Arc::clone(graph);
    let query = query.to_owned();
    tokio::task::spawn_blocking(move || match graph.shadow_recall(&query, SHADOW_K) {
        Ok(shadow) => {
            // The selection, as ids and scores rather than content: this lands
            // in a log file, and memory content is the user's.
            let picked: Vec<String> = shadow
                .selected
                .iter()
                .map(|r| format!("{}:{}:{:.4}", r.node.kind, r.node.id, r.score))
                .collect();
            tracing::info!(
                target: "memory_shadow",
                phase = "recall",
                query_chars = query.chars().count(),
                considered = shadow.considered,
                selected = shadow.selected.len(),
                would_inject_chars = shadow.would_inject_chars,
                picked = %picked.join(","),
                "shadow retrieval (nothing injected)"
            );
        }
        Err(error) => tracing::warn!(%error, "shadow retrieval failed"),
    });
}

#[cfg(test)]
#[path = "tests/memory_shadow.rs"]
mod tests;
