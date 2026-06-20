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

/// Sink for streamed assistant-text deltas — the daemon forwards each
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
    /// Optional graph memory — episode capture on compression. The memory
    /// tools themselves are wired through the catalog, not through here.
    pub(crate) graph: Option<Arc<regent_graph::GraphMemory>>,
    /// Optional learning loop (post-turn background review fork).
    pub(crate) review: Option<Arc<crate::application::review::ReviewSetup>>,
    pub(crate) review_handle: Option<tokio::task::JoinHandle<()>>,
    /// Optional sink for streamed assistant-text deltas (live UI). When set,
    /// the turn uses the provider's streaming path.
    pub(crate) delta_sink: Option<DeltaSink>,
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
            graph: None,
            review: None,
            review_handle: None,
            delta_sink: None,
        })
    }

    /// Resumes an existing session. The **stored** system prompt wins over
    /// `fallback_system_prompt` (byte-stability across resumes); history is
    /// replayed through the alternation-validating transcript so corruption
    /// fails loudly here, not as a provider 400 mid-turn.
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
            transcript.push(stored.message)?;
        }
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
            graph: None,
            review: None,
            review_handle: None,
            delta_sink: None,
        })
    }

    /// Attaches graph memory (episode capture on compression splits).
    #[must_use]
    pub fn with_graph_memory(mut self, graph: Arc<regent_graph::GraphMemory>) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Attaches a delta sink; turns then stream assistant text to it as the
    /// model produces it (the daemon forwards these as `message.delta`).
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
}
