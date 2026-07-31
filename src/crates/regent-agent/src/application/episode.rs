//! The episodic anchor for an ORDINARY session — the half of `record_episode`
//! that was never wired.
//!
//! `regent_graph::record_episode` documents itself as "one summary node per
//! compressed/ended session", but its only caller was the compaction branch in
//! `lifecycle.rs`. Measured on the owner's store: `end_reason='compressed'` was
//! true for 1 session out of 1,186, and the graph held ZERO nodes of kind
//! `episode`. So the anchor fired essentially never, and "what did we do last
//! week" had nothing to retrieve — the memory tool was searching an empty
//! shelf and reporting, accurately, that it found nothing.
//!
//! A session that never compacted has no summarizer output to reuse, and
//! shutdown is the wrong moment to spend a model call that can fail. So the
//! summary here is EXTRACTIVE: the user's own asks, in order, bounded. That is
//! also the text most likely to match a later "what did we do about X" query,
//! because it is phrased the way the user phrases things.

use crate::application::agent::Agent;
use regent_kernel::Role;
use std::sync::Arc;

/// Upper bound on the episode summary. Long enough to hold a real session's
/// worth of asks, short enough that the node stays a usable retrieval target
/// rather than a second copy of the transcript.
const MAX_SUMMARY_CHARS: usize = 1_200;

/// Below this there is no session worth anchoring — a stray "hi" and nothing
/// else is noise in the graph, not history.
const MIN_SUMMARY_CHARS: usize = 24;

/// The user's asks from `messages`, oldest first, truncated on a message
/// boundary so the summary never ends mid-sentence. Pure: the whole selection
/// rule is testable without a store, a graph, or a model.
pub(crate) fn extractive_summary(messages: &[regent_kernel::ChatMessage]) -> String {
    let mut out = String::new();
    for message in messages.iter().filter(|m| m.role == Role::User) {
        let Some(text) = message.content.as_deref().map(str::trim) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        // Stop BEFORE overflowing rather than pushing and truncating: a
        // half-sentence embeds badly and reads worse.
        if out.len() + text.len() + 2 > MAX_SUMMARY_CHARS {
            break;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(text);
    }
    out
}

impl Agent {
    /// Record this session's episodic anchor, best-effort. Called from the
    /// session-end flush so both wind-down paths (the idle sweep and shutdown
    /// drain) are covered by one hook.
    ///
    /// Fire-and-forget by design: a graph failure must never hold up shutdown
    /// or mask the review it travels with. No-op without a graph.
    ///
    /// ponytail: dedupe is the store's `content_hash` over (kind, name,
    /// content), so flushing the same session twice with no new user messages
    /// writes nothing. A session that GREW between two flushes does leave a
    /// second, longer episode — superseding rather than duplicating. If that
    /// noise ever matters, the upgrade is an update-by-name in the graph
    /// store, not a flag on the agent that a restart would forget.
    pub(crate) fn record_session_episode(&self) {
        let Some(graph) = self.graph.clone() else {
            return;
        };
        let summary = extractive_summary(self.transcript.messages());
        if summary.len() < MIN_SUMMARY_CHARS {
            return;
        }
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let recorded = tokio::task::spawn_blocking(move || {
                graph.record_episode(session_id.as_str(), &summary)
            })
            .await;
            match recorded {
                Ok(Ok(node_id)) => tracing::info!(node_id, "session episode recorded"),
                Ok(Err(error)) => tracing::warn!(%error, "episode capture failed"),
                Err(join_error) => tracing::warn!(%join_error, "episode task failed"),
            }
        });
    }
}

// Silences an unused-import warning when the crate is built without tokio's
// full feature set; `Arc` is used by the graph handle above.
const _: Option<Arc<()>> = None;

#[cfg(test)]
#[path = "tests/episode.rs"]
mod tests;
