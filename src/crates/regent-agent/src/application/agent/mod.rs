//! The agent: a frozen-prompt session over a provider + tool catalog. This
//! module owns construction (fresh/resume), the streaming/cancel handles, and
//! the field layout; `turn` owns the run loop. Compression and post-turn
//! review live in `lifecycle`/`review` (also `impl Agent`).

mod resume;
mod telemetry;
mod turn;

use crate::domain::config::AgentConfig;
use regent_kernel::{RegentError, SessionId, Transcript};
use regent_providers::ChatProvider;
use regent_store::Store;
use regent_tools::{ToolCatalog, ToolContext};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
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
    /// Gap L2: the current/last turn hit its budget and ended with a wrap-up
    /// summary — the turns ledger records `budget_exhausted` even though the
    /// turn returns `Ok`.
    pub(crate) last_turn_budget_exhausted: bool,
    /// Gap C4 circuit breaker: a compression pass failed to bring the estimate
    /// back under threshold, so further passes would loop session-splits for
    /// nothing. Once open it stays open for the session.
    pub(crate) compression_broken: bool,
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
    /// Transcript length already covered by a spawned review — reviews batch
    /// and see only messages past this mark (see `review.rs`).
    pub(crate) reviewed_len: Arc<AtomicUsize>,
    /// Highest transcript length already queued for review.
    pub(crate) review_scheduled_len: Arc<AtomicUsize>,
    /// Serializes queued review jobs so a later snapshot can trim the range a
    /// preceding successful job already committed.
    pub(crate) review_gate: Arc<tokio::sync::Mutex<()>>,
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
        // Adopt the caller's interrupt if it handed one over, else mint one and
        // hand it down. This is the whole of `interrupt_subagent`: a
        // `delegate_task` child is built from a clone of the parent's context,
        // so adopting here links it to the parent's stop without the delegation
        // code knowing anything about tokens. Previously every child minted a
        // fresh one and ran its full 50-iteration budget after the user had
        // already pressed stop.
        let cancel = tool_context.cancel_token().unwrap_or_default();
        let tool_context = tool_context.with_cancel(cancel.clone());
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
            cancel,
            turn_api_calls: 0,
            last_turn_budget_exhausted: false,
            compression_broken: false,
            last_turn_input_tokens: 0,
            last_turn_output_tokens: 0,
            last_turn_cache_read: None,
            last_turn_cache_write: None,
            graph: None,
            review: None,
            review_handle: None,
            reviewed_len: Arc::new(AtomicUsize::new(0)),
            review_scheduled_len: Arc::new(AtomicUsize::new(0)),
            review_gate: Arc::new(tokio::sync::Mutex::new(())),
            delta_sink: None,
            last_cache_reset: None,
            pending_cache_reset: None,
        })
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

    /// Repoints this session's tools at a different tree — the *only* legal
    /// way to move a live session's workspace.
    ///
    /// The caller must build `ctx` through the same constructor used at session
    /// birth, so the jail is **recomputed**, never edited: opening a real repo
    /// is what turns the jail on (`should_sandbox`'s `workspace_set` trigger),
    /// and a setter that mutated a root in place would keep an unjailed context
    /// pointed at the user's actual project. Recomputing means a rebind lands
    /// exactly the context a new session with that folder would have got.
    pub fn set_tool_context(&mut self, ctx: ToolContext) {
        self.tool_context = ctx;
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
    ///
    /// The context is re-stamped with the new token, or children spawned after
    /// a reset would adopt the *cancelled* one and stop immediately.
    pub fn reset_interrupt(&mut self) {
        self.cancel = CancellationToken::new();
        self.tool_context = self.tool_context.clone().with_cancel(self.cancel.clone());
    }

    /// Swaps the provider mid-session so a model/key/config change reaches
    /// open sessions on their next turn, not just new ones. Costs the cached
    /// prompt prefix — acceptable for an explicit user switch.
    pub fn set_provider(&mut self, provider: Arc<dyn ChatProvider>) {
        self.provider = provider;
    }

    /// ADR-038 P2: one-way light→full profile escalation. Swaps in the FULL
    /// catalog + prompt the deacon rebuilt and stamps the coming turn's
    /// `cache_reset: "profile"`. Alongside the compression session-split,
    /// this is the only sanctioned mid-session prompt change — it happens at
    /// most once per session and only ever upward (light→full), so it can't
    /// oscillate the cache.
    pub fn escalate_profile(&mut self, catalog: Arc<ToolCatalog>, system_prompt: String) {
        self.catalog = catalog;
        self.system_prompt = system_prompt;
        self.pending_cache_reset = Some("profile");
    }
}
