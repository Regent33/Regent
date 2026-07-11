//! SPL P5 (`docs/proposal/token-efficiency-architecture-v1.md` §3.6): the
//! Distiller. Persona budgets fail-closed at the write; the Distiller keeps
//! writers from ever hitting that wall. Any persona row past 80% of its budget
//! gets ONE consolidation model call (merge duplicates, compress phrasing,
//! lose nothing semantic) whose result is staged as a PENDING WRITE — **always
//! human-gated, for every store including soul and constitution**: a
//! background model call must never rewrite the agent's identity unreviewed.
//! Approval (see `SessionManager::approve_memory_write`) first backs the old
//! content up to a non-rendering `backup.<key>` persona row — a bulk rewrite
//! is a relocation, never a loss — then applies through the budgeted
//! `set_persona`. Fail-open everywhere: a failed call or an over-budget
//! rewrite just skips that row until next pass.

use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest};
use regent_store::{ABOUT_SECTIONS, PendingWriteRow, Store, now_epoch, persona_budget};
use std::sync::Arc;
use std::time::Duration;

/// Pending-write kind for a Distiller persona rewrite — the approve path
/// dispatches on it (budgeted persona apply, not a graph node).
pub const PERSONA_REWRITE_KIND: &str = "persona_rewrite";

/// Fill ratio (percent of the row's char budget) that triggers consolidation.
const FILL_TRIGGER_PERCENT: usize = 80;

/// An undecided proposal expires after a week (the pending-expiry loop
/// auto-rejects it) — a stale rewrite of a store that kept changing shouldn't
/// land months later.
const PROPOSAL_TTL_SECS: f64 = 7.0 * 86_400.0;

/// Between watcher passes. Same order as the skill curator: persona stores
/// fill over days, not minutes.
const DISTILL_INTERVAL_SECS: u64 = 6 * 3_600;

const DISTILL_SYSTEM: &str = "You are a careful editor consolidating a personal-context document. \
    Merge duplicates, compress phrasing, and drop nothing semantic: every fact, preference, \
    boundary, and constraint in the input must survive in the output (reworded is fine, lost is \
    not). Keep the document's language, person, and formatting conventions. Return ONLY the \
    consolidated document text — no preamble, no commentary, no code fences.";

/// Every budgeted persona row the watcher checks.
fn watched_keys() -> Vec<String> {
    let mut keys = vec![
        "constitution".to_owned(),
        "soul".to_owned(),
        "about".to_owned(),
    ];
    keys.extend(
        ABOUT_SECTIONS
            .iter()
            .map(|(slug, _)| format!("about.{slug}")),
    );
    keys
}

/// One watcher pass: stages a consolidation proposal for every persona row
/// past the fill trigger that doesn't already have one pending. Returns how
/// many proposals were staged.
pub async fn distill_once(store: &Store, provider: &dyn ChatProvider) -> usize {
    let mut proposed = 0;
    for key in watched_keys() {
        let Ok(content) = store.get_persona(&key) else {
            continue;
        };
        let budget = persona_budget(&key);
        let fill = content.chars().count();
        if fill * 100 < budget * FILL_TRIGGER_PERCENT {
            continue;
        }
        let request = ChatRequest::new(
            DISTILL_SYSTEM,
            vec![ChatMessage::user(format!(
                "Consolidate this `{key}` document so it comfortably fits {budget} characters \
                 (aim for about two thirds of that):\n\n{content}"
            ))],
        );
        let rewritten = match provider.complete(&request).await {
            Ok(r) => r.message.content.unwrap_or_default(),
            Err(error) => {
                tracing::warn!(%error, key, "distill consolidation call failed");
                continue;
            }
        };
        let rewritten = rewritten.trim();
        // Sanity gates: a rewrite must be real text, actually smaller, and
        // inside the budget — otherwise skip and retry next pass.
        if rewritten.is_empty() || rewritten.chars().count() >= fill.min(budget) {
            tracing::warn!(key, "distill rewrite rejected (empty or not smaller)");
            continue;
        }
        let row = PendingWriteRow {
            // Stable id = at most one live proposal per store: a second pass
            // while one is pending hits the primary key and is skipped.
            id: format!("distill:{key}"),
            kind: PERSONA_REWRITE_KIND.to_owned(),
            name: key.clone(),
            content: rewritten.to_owned(),
            provenance: "distiller".to_owned(),
            trust: 1.0,
            session_id: None,
            ttl_secs: Some(PROPOSAL_TTL_SECS),
            created_at: now_epoch(),
        };
        if store.enqueue_pending_write(&row).is_ok() {
            tracing::info!(key, fill, budget, "distill proposal staged for approval");
            proposed += 1;
        }
    }
    proposed
}

/// The boot-spawned watcher loop. First pass waits one interval, so short
/// sessions never spend a consolidation model call.
pub fn spawn_distiller(store: Arc<Store>, provider: Arc<dyn ChatProvider>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(DISTILL_INTERVAL_SECS)).await;
            let n = distill_once(&store, provider.as_ref()).await;
            if n > 0 {
                tracing::info!(proposals = n, "distiller pass staged persona rewrites");
            }
        }
    });
}
