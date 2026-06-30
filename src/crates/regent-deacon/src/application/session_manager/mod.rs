//! Per-session Agent lifecycle. One agent mutex per session — one turn at a
//! time. This file owns the registry struct + `run_turn`/`drain`; the rest is
//! split out: `build` (catalog/prompt assembly), `hooks` (per-session plumbing
//! types), `queries` (read/approval accessors), `lifecycle` (session create/
//! resume), `admin` (the in-process admin dispatcher), and `code` (the coding
//! harness `code.plan`/`code.start` flows).

mod admin;
mod build;
mod code;
mod hooks;
mod lifecycle;
mod queries;

pub use admin::AdminDeps;
pub use code::CodeStartResult;

use crate::domain::contracts::{OutboundTx, PlatformDelivery, ProviderFactory};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DaemonError;
use hooks::SessionEntry;
use regent_agent::AgentConfig;
use regent_kernel::SessionId;
use regent_skills::SkillLibrary;
use regent_store::Store;
use regent_tools::DeliverySink;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct SessionManager {
    /// Builds a provider for a model id; new sessions use the current model.
    provider_factory: ProviderFactory,
    /// Active model for new sessions — mutated by `set_model`.
    current_model: std::sync::Mutex<String>,
    store: Arc<Store>,
    graph: Arc<regent_graph::GraphMemory>,
    skills: Arc<SkillLibrary>,
    cwd: PathBuf,
    /// Template cloned per session (source overridden to "daemon"); built from
    /// config.yaml at the composition root — the single behavior source.
    agent_template: AgentConfig,
    /// Tool names filtered out of every session catalog (config `tools.disabled`).
    disabled_tools: Vec<String>,
    entries: Mutex<HashMap<SessionId, SessionEntry>>,
    out_tx: OutboundTx,
    /// Routes keyed platform sessions' outbound to the platform API. Filled by
    /// the composition root once the webhook registry exists; empty → every
    /// session uses the CLI-notification sink (the prior behavior).
    platform_delivery: OnceLock<Arc<dyn PlatformDelivery>>,
    /// Self-handle for the in-process `regent` admin tool to build a dispatcher.
    /// Set by `install_admin`; absent → the tool isn't registered (e.g. tests).
    self_ref: OnceLock<Weak<SessionManager>>,
    /// Cron/config/speech the admin dispatcher needs. Set by `install_admin`.
    admin: OnceLock<AdminDeps>,
}

impl SessionManager {
    // Composition-root wiring — all dependencies arrive explicitly.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_factory: ProviderFactory,
        initial_model: impl Into<String>,
        store: Arc<Store>,
        graph: Arc<regent_graph::GraphMemory>,
        skills: Arc<SkillLibrary>,
        cwd: PathBuf,
        agent_template: AgentConfig,
        disabled_tools: Vec<String>,
        out_tx: OutboundTx,
    ) -> Self {
        Self {
            provider_factory,
            current_model: std::sync::Mutex::new(initial_model.into()),
            store,
            graph,
            skills,
            cwd,
            agent_template,
            disabled_tools,
            entries: Mutex::new(HashMap::new()),
            out_tx,
            platform_delivery: OnceLock::new(),
            self_ref: OnceLock::new(),
            admin: OnceLock::new(),
        }
    }

    /// Installs the platform-delivery resolver (composition root, after the
    /// webhook registry is built). Idempotent: a second call is ignored.
    pub fn set_platform_delivery(&self, delivery: Arc<dyn PlatformDelivery>) {
        let _ = self.platform_delivery.set(delivery);
    }

    /// The platform sink for a keyed session, if the key names a known outbound
    /// webhook target. `None` for local CLI sessions and unkeyed creation.
    pub(super) fn platform_sink(&self, key: Option<&str>) -> Option<Arc<dyn DeliverySink>> {
        let key = key?;
        self.platform_delivery.get()?.sink_for(key)
    }

    pub async fn run_turn(
        &self,
        session_id: &SessionId,
        text: &str,
    ) -> Result<String, DaemonError> {
        let (agent_arc, interrupt_arc) = {
            let entries = self.entries.lock().await;
            match entries.get(session_id) {
                Some(e) => (Arc::clone(&e.agent), Arc::clone(&e.interrupt)),
                None => return Err(DaemonError::SessionNotFound(session_id.to_string())),
            }
        };

        let mut agent = agent_arc.lock().await;
        agent.reset_interrupt();
        let agent_cancel = agent.cancel_handle();

        let session_cancel = CancellationToken::new();
        *interrupt_arc.lock().await = Some(session_cancel.clone());

        let watcher = tokio::spawn(async move {
            session_cancel.cancelled().await;
            agent_cancel.cancel();
        });

        let result = agent.run_turn(text).await;
        // Emit post-turn context usage so the CLI status line can show the
        // context-fill bar + model (Hermes-style). Best-effort; other surfaces
        // (HTTP/gateway) don't read this notification, so it's harmless there.
        if result.is_ok() {
            let (context_tokens, max_context_tokens) = agent.context_usage();
            let model = self
                .current_model
                .lock()
                .map(|m| m.clone())
                .unwrap_or_default();
            let notification = RpcNotification::new(
                "turn.usage",
                json!({
                    "session_id": session_id.to_string(),
                    "context_tokens": context_tokens,
                    "max_context_tokens": max_context_tokens,
                    "model": model,
                }),
            );
            if let Ok(line) = serde_json::to_string(&notification) {
                self.out_tx.send(line).ok();
            }
        }
        watcher.abort();
        *interrupt_arc.lock().await = None;
        result.map_err(DaemonError::Core)
    }

    /// Cancels every in-flight turn, then waits briefly so cancelled turns
    /// finish recording their ledger rows before the process exits.
    pub async fn drain(&self) {
        let arcs: Vec<_> = {
            let entries = self.entries.lock().await;
            entries.values().map(|e| Arc::clone(&e.interrupt)).collect()
        };
        let mut cancelled_any = false;
        for arc in arcs {
            if let Some(token) = arc.lock().await.as_ref() {
                token.cancel();
                cancelled_any = true;
            }
        }
        if cancelled_any {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}
