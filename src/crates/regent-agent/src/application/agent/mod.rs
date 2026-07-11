//! The agent: a frozen-prompt session over a provider + tool catalog. This
//! module owns construction (fresh/resume), the streaming/cancel handles, and
//! the field layout; `turn` owns the run loop. Compression and post-turn
//! review live in `lifecycle`/`review` (also `impl Agent`).

mod turn;

use crate::domain::config::AgentConfig;
use regent_kernel::{RegentError, SessionId, Transcript};
use regent_providers::ChatProvider;
use regent_store::Store;
use regent_tools::{ToolCatalog, ToolContext};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Sink for streamed assistant-text deltas — the deacon forwards each
/// fragment to the UI as a `message.delta` notification.
pub type DeltaSink = Arc<dyn Fn(&str) + Send + Sync>;

pub struct Agent {
    pub(crate) provider: Arc<dyn ChatProvider>,
    pub(crate) catalog: Arc<ToolCatalog>,
    pub(crate) store: Arc<Store>,
    pub(crate) tool_context: ToolContext,
    pub(crate) config: AgentConfig,
    pub(crate) session_id: SessionId,
    pub(crate) transcript: Transcript,
    /// Frozen at construction — never rebuilt mid-conversation. The sole
    /// exception is the compression session-split (`maybe_compress`), which
    /// starts a child session rather than mutating this one.
    pub(crate) system_prompt: String,
    pub(crate) cancel: CancellationToken,
    /// Model calls made by the current/last turn (reproducibility ledger).
    pub(crate) turn_api_calls: u32,
    /// Prompt/completion tokens summed across the current/last turn's model
    /// calls — surfaced post-turn so the status bar can show a context meter.
    pub(crate) last_turn_input_tokens: u32,
    pub(crate) last_turn_output_tokens: u32,
    /// SPL P2 (§3.3): provider-reported prompt-cache usage summed across the
    /// current/last turn's model calls. `None` when no call reported cache
    /// activity (non-caching provider) — additive passthrough to `turn.complete`.
    pub(crate) last_turn_cache_read: Option<u32>,
    pub(crate) last_turn_cache_write: Option<u32>,
    /// Optional graph memory — episode capture on compression. The memory
    /// tools themselves are wired through the catalog, not through here.
    pub(crate) graph: Option<Arc<regent_graph::GraphMemory>>,
    /// Optional learning loop (post-turn background review fork).
    pub(crate) review: Option<Arc<crate::application::review::ReviewSetup>>,
    pub(crate) review_handle: Option<tokio::task::JoinHandle<()>>,
    /// Optional sink for streamed assistant-text deltas (live UI). When set,
    /// the turn uses the provider's streaming path.
    pub(crate) delta_sink: Option<DeltaSink>,
    /// SPL P2/P3 cache-reset seam. Records why the current/last turn busted the
    /// provider cache — set to `"pruning"` when tool-result pruning (§3.8)
    /// rewrites Tier-2 history. P2's `cache_reset` attribution plumbing (built
    /// concurrently in regent-deacon/regent-agent) reads this at turn end. This
    /// is the single recording point, deliberately NOT a parallel mechanism —
    /// P2 wires the enum/notification; P3 only fills in the pruning reason.
    /// P2 adds `routing`/`compaction`/`failover` via `note_cache_reset`, which
    /// keeps the highest-priority cause when several fire in one turn.
    pub(crate) last_cache_reset: Option<&'static str>,
    /// SPL P2: a reset reason the deacon stamps BEFORE the turn (a routing-epoch
    /// provider swap), consumed into `last_cache_reset` at turn start — the one
    /// cause that originates outside the turn loop. `None` normally.
    pub(crate) pending_cache_reset: Option<&'static str>,
}

impl Agent {
    /// Starts a fresh session; the system prompt is persisted with it so
    /// resume can restore the exact bytes.
    pub fn new(
        provider: Arc<dyn ChatProvider>,
        catalog: Arc<ToolCatalog>,
        store: Arc<Store>,
        tool_context: ToolContext,
        system_prompt: impl Into<String>,
        config: AgentConfig,
    ) -> Result<Self, RegentError> {
        let session_id = SessionId::generate();
        let system_prompt = system_prompt.into();
        store.create_session(
            &session_id,
            &config.source,
            Some(provider.model()),
            Some(&system_prompt),
            None,
        )?;
        Ok(Self {
            provider,
            catalog,
            store,
            tool_context,
            config,
            session_id,
            transcript: Transcript::new(),
            system_prompt,
            cancel: CancellationToken::new(),
            turn_api_calls: 0,
            last_turn_input_tokens: 0,
            last_turn_output_tokens: 0,
            last_turn_cache_read: None,
            last_turn_cache_write: None,
            graph: None,
            review: None,
            review_handle: None,
            delta_sink: None,
            last_cache_reset: None,
            pending_cache_reset: None,
        })
    }

    /// Estimated current context size (tokens) and the configured budget — drives
    /// the CLI status line's context-fill bar. Cheap: a length-based estimate, the
    /// same one compression uses, so the two never disagree.
    #[must_use]
    pub fn context_usage(&self) -> (u32, u32) {
        let used = crate::domain::compression::estimate_tokens(
            &self.system_prompt,
            self.transcript.messages(),
        );
        (used, self.config.max_context_tokens)
    }

    /// Prompt/completion tokens the last completed turn spent (summed across its
    /// model calls). `(0, 0)` before the first turn.
    #[must_use]
    pub fn last_turn_usage(&self) -> (u32, u32) {
        (self.last_turn_input_tokens, self.last_turn_output_tokens)
    }

    /// SPL P2 (§3.3): provider-reported prompt-cache usage for the last turn as
    /// `(cache_read, cache_write)`. Each is `None` when no model call this turn
    /// reported that field — passed through additively to `turn.complete`.
    #[must_use]
    pub fn last_turn_cache_usage(&self) -> (Option<u32>, Option<u32>) {
        (self.last_turn_cache_read, self.last_turn_cache_write)
    }

    /// SPL cache-reset reason for the current/last turn (`"pruning"`,
    /// `"compaction"`, `"failover"`, or `"routing"`; `None` when the prefix
    /// carried over). The seam P2's `cache_reset` attribution reads at turn end.
    #[must_use]
    pub fn last_cache_reset(&self) -> Option<&'static str> {
        self.last_cache_reset
    }

    /// Records a cache-reset reason for the current turn, keeping the
    /// highest-priority cause when several fire: routing (whole prefix cold) >
    /// compaction (history rewritten wholesale) > failover (provider swapped
    /// mid-turn) > pruning (Tier-2 stub). The single write path for
    /// `last_cache_reset` — every trigger point calls this.
    pub(crate) fn note_cache_reset(&mut self, reason: &'static str) {
        fn rank(reason: &str) -> u8 {
            match reason {
                "routing" => 4,
                "compaction" => 3,
                "failover" => 2,
                "pruning" => 1,
                _ => 0,
            }
        }
        if self
            .last_cache_reset
            .is_none_or(|cur| rank(reason) > rank(cur))
        {
            self.last_cache_reset = Some(reason);
        }
    }

    /// SPL P2: stamps the next turn as a `routing` reset — called by the deacon
    /// when a routing-epoch bump swaps this session's provider before the turn
    /// runs (the whole prefix warms cold on the new model). Consumed at turn
    /// start; harmless if the turn never runs.
    pub fn mark_provider_routed(&mut self) {
        self.pending_cache_reset = Some("routing");
    }

    /// The frozen system prompt exactly as every turn sends it. Stable-prefix
    /// telemetry hashes THIS — never a live store re-read, because mid-session
    /// persona edits don't reach the wire until the next session build.
    #[must_use]
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    /// The tool definitions exactly as `run_turn` derives them each turn —
    /// re-derived per call so telemetry can catch serialization instability.
    #[must_use]
    pub fn tool_definitions(&self) -> Vec<regent_kernel::ToolDefinition> {
        self.catalog.definitions()
    }

    /// Resumes an existing session. The **stored** system prompt wins over
    /// `fallback_system_prompt` (byte-stability across resumes); history is
    /// replayed through the alternation-validating transcript. A crashed turn
    /// keeps its rows in the store (dangling user message, unanswered tool
    /// calls), so replay REPAIRS instead of failing: illegal rows get the same
    /// recovery `run_turn` applies live, and a repaired-but-still-illegal row
    /// is skipped — resume must never brick a session on old history.
    pub fn resume(
        provider: Arc<dyn ChatProvider>,
        catalog: Arc<ToolCatalog>,
        store: Arc<Store>,
        tool_context: ToolContext,
        fallback_system_prompt: impl Into<String>,
        config: AgentConfig,
        session_id: SessionId,
    ) -> Result<Self, RegentError> {
        let fallback = fallback_system_prompt.into();
        let system_prompt = match store.session_system_prompt(&session_id)? {
            Some(stored) => {
                if stored != fallback {
                    tracing::info!(session = %session_id, "using stored system prompt (differs from caller's)");
                }
                stored
            }
            None => fallback,
        };
        let mut transcript = Transcript::new();
        for stored in store.get_conversation(&session_id)? {
            let message = stored.message;
            if transcript.push(message.clone()).is_err() {
                transcript.settle_pending_tools("interrupted before completion");
                transcript.drop_trailing_user();
                if transcript.push(message).is_err() {
                    tracing::warn!(session = %session_id, "resume: skipped a stored message that violates transcript order");
                }
            }
        }
        // A stored tail from a crashed turn would make the next user push
        // illegal — trim it exactly like run_turn's live recovery does.
        transcript.settle_pending_tools("interrupted before completion");
        transcript.drop_trailing_user();
        Ok(Self {
            provider,
            catalog,
            store,
            tool_context,
            config,
            session_id,
            transcript,
            system_prompt,
            cancel: CancellationToken::new(),
            turn_api_calls: 0,
            last_turn_input_tokens: 0,
            last_turn_output_tokens: 0,
            last_turn_cache_read: None,
            last_turn_cache_write: None,
            graph: None,
            review: None,
            review_handle: None,
            delta_sink: None,
            last_cache_reset: None,
            pending_cache_reset: None,
        })
    }

    /// Attaches graph memory (episode capture on compression splits).
    #[must_use]
    pub fn with_graph_memory(mut self, graph: Arc<regent_graph::GraphMemory>) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Attaches a delta sink; turns then stream assistant text to it as the
    /// model produces it (the deacon forwards these as `message.delta`).
    #[must_use]
    pub fn with_delta_sink(mut self, sink: DeltaSink) -> Self {
        self.delta_sink = Some(sink);
        self
    }

    #[must_use]
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Handle for interrupting a running turn from another task (Ctrl-C,
    /// `/stop`, shutdown). An abandoned model call never enters history.
    #[must_use]
    pub fn cancel_handle(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Re-arms the interrupt after a cancelled turn — a cancelled token
    /// stays cancelled, so long-lived sessions (gateway) reset per turn.
    pub fn reset_interrupt(&mut self) {
        self.cancel = CancellationToken::new();
    }

    /// Swaps the provider mid-session so a model/key/config change reaches
    /// open sessions on their next turn, not just new ones. Costs the cached
    /// prompt prefix — acceptable for an explicit user switch.
    pub fn set_provider(&mut self, provider: Arc<dyn ChatProvider>) {
        self.provider = provider;
    }
}
