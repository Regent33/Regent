//! Per-session Agent lifecycle. One agent mutex per session — one turn at a
//! time. This file owns the registry struct + `run_turn`/`drain`; the rest is
//! split out: `build` (catalog/prompt assembly), `hooks` (per-session plumbing
//! types), `queries` (read/approval accessors), `lifecycle` (session create/
//! resume), `admin` (the in-process admin dispatcher), `code` (the coding
//! harness `code.plan`/`code.start` flows), and `telemetry` (the SPL
//! stable-prefix per-turn check).

mod admin;
mod backfill;
mod board_queries;
mod build;
mod catalogs;
mod code;
mod explore;
mod hooks;
mod lifecycle;
mod memory_queries;
mod prompt_lines;
mod queries;
mod telemetry;
mod titling;

pub use admin::AdminDeps;
pub use backfill::BackfillReport;
pub use code::CodeStartResult;
pub(crate) use titling::exchange_snippet;

use crate::domain::contracts::{OutboundTx, PlatformDelivery, ProviderFactory};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DeaconError;
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
    /// Bumped on every model/key/config change (`set_model`, the dispatcher's
    /// reload path). `run_turn` compares each session's stamp against this and
    /// rebuilds a stale session's provider first, so a switch reaches open
    /// sessions on their very next turn — not just new sessions.
    routing_epoch: std::sync::atomic::AtomicU64,
    store: Arc<Store>,
    graph: Arc<regent_graph::GraphMemory>,
    skills: Arc<SkillLibrary>,
    cwd: PathBuf,
    /// Template cloned per session (source overridden to "deacon"); built from
    /// config.yaml at the composition root — the single behavior source.
    agent_template: AgentConfig,
    /// Tool names filtered out of every session catalog (config `tools.disabled`).
    disabled_tools: Vec<String>,
    /// Tool names whose schemas are withheld per request until `load_tools`
    /// activates them (config `tools.deferred` — token efficiency).
    deferred_tools: Vec<String>,
    /// Adaptive tiering (SPL §3.5): auto-defer tools unused in the last 30
    /// days at session build (config `tools.auto_tier`).
    auto_tier: bool,
    /// Never auto-deferred (config `tools.pinned` — the §3.5 safety valve).
    pinned_tools: Vec<String>,
    /// Gap S7 lifecycle hooks (config `tools.hook_tool_start` /
    /// `tools.hook_tool_complete`); `None` when neither is set.
    shell_hook: Option<Arc<regent_tools::ShellHook>>,
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
        tools_cfg: crate::domain::config::ToolsConfig,
        out_tx: OutboundTx,
    ) -> Self {
        Self {
            provider_factory,
            current_model: std::sync::Mutex::new(initial_model.into()),
            routing_epoch: std::sync::atomic::AtomicU64::new(0),
            store,
            graph,
            skills,
            cwd,
            agent_template,
            disabled_tools: tools_cfg.disabled,
            deferred_tools: tools_cfg.deferred,
            auto_tier: tools_cfg.auto_tier,
            pinned_tools: tools_cfg.pinned,
            shell_hook: {
                let hook = regent_tools::ShellHook::new(
                    &tools_cfg.hook_tool_start,
                    &tools_cfg.hook_tool_complete,
                );
                hook.is_active().then(|| Arc::new(hook))
            },
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
    ) -> Result<String, DeaconError> {
        let (agent_arc, interrupt_arc, epoch_arc) = {
            let entries = self.entries.lock().await;
            match entries.get(session_id) {
                Some(e) => (
                    Arc::clone(&e.agent),
                    Arc::clone(&e.interrupt),
                    Arc::clone(&e.provider_epoch),
                ),
                None => return Err(DeaconError::SessionNotFound(session_id.to_string())),
            }
        };

        let mut agent = agent_arc.lock().await;
        // A model/key/config change since this session's provider was built?
        // Swap in a fresh one so the change applies to THIS turn, not just new
        // sessions. Costs the cached prompt prefix — the user asked to switch.
        let epoch = self.routing_epoch();
        if epoch_arc.load(std::sync::atomic::Ordering::Acquire) != epoch {
            agent.set_provider(self.provider());
            epoch_arc.store(epoch, std::sync::atomic::Ordering::Release);
            // SPL P2 (§3.1): a routing swap warms the new provider's cache cold —
            // stamp this turn so `turn.complete` attributes the full-price turn.
            agent.mark_provider_routed();
        }
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
            let (input_tokens, output_tokens) = agent.last_turn_usage();
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
                    // Additive (M8 status-bar context meter): the just-finished
                    // turn's token spend + the context budget under the name the
                    // desktop expects. `context_max` == `max_context_tokens`.
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "context_max": max_context_tokens,
                    "model": model,
                }),
            );
            if let Ok(line) = serde_json::to_string(&notification) {
                self.out_tx.send(line).ok();
            }
        }
        watcher.abort();
        *interrupt_arc.lock().await = None;
        result.map_err(DeaconError::Core)
    }

    /// The just-finished turn's usage for the status-bar context meter:
    /// `(input_tokens, output_tokens, context_max, cache_read, cache_write)`
    /// where `context_max` is the session's context budget and the two cache
    /// fields (SPL P2) are `Some` only when the provider reported prompt-cache
    /// usage. `None` for an unknown session. Smallest additive accessor so
    /// `prompt.submit` can attach the fields the desktop reads off
    /// `turn.complete` without re-plumbing `run_turn`.
    pub async fn last_turn_usage(
        &self,
        session_id: &SessionId,
    ) -> Option<(u32, u32, u32, Option<u32>, Option<u32>)> {
        let agent_arc = {
            let entries = self.entries.lock().await;
            Arc::clone(&entries.get(session_id)?.agent)
        };
        let agent = agent_arc.lock().await;
        let (input_tokens, output_tokens) = agent.last_turn_usage();
        let (cache_read, cache_write) = agent.last_turn_cache_usage();
        let (_used, context_max) = agent.context_usage();
        Some((
            input_tokens,
            output_tokens,
            context_max,
            cache_read,
            cache_write,
        ))
    }

    /// SPL P2 (§3.1): why the just-finished turn was full-price, when known
    /// (`"compaction"` | `"failover"` | `"routing"` | `"pruning"`). `None` when
    /// no reset happened or the session is unknown — omitted from `turn.complete`
    /// in that case.
    pub async fn last_turn_cache_reset(&self, session_id: &SessionId) -> Option<&'static str> {
        let agent_arc = {
            let entries = self.entries.lock().await;
            Arc::clone(&entries.get(session_id)?.agent)
        };
        let guard = agent_arc.lock().await;
        guard.last_cache_reset()
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
