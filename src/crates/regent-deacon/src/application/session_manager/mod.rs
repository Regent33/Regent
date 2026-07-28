//! Per-session Agent lifecycle. One agent mutex per session — one turn at a
//! time. This file owns the registry struct + constructor/setters; the rest is
//! split out: `run` (`run_turn`/`drain`), `build` (catalog/prompt assembly),
//! `hooks` (per-session plumbing types), `queries` (read/approval accessors),
//! `lifecycle` (session create/resume), `admin` (the in-process admin
//! dispatcher), `code` (the coding harness `code.plan`/`code.start` flows), and
//! `telemetry` (the SPL stable-prefix per-turn check).

mod admin;
mod backfill;
mod board_queries;
mod build;
mod catalogs;
mod code;
mod escalation;
mod explore;
mod hooks;
mod lifecycle;
mod memory_queries;
mod profile_estimate;
mod prompt_lines;
mod queries;
mod run;
mod session_ctx;
mod telemetry;
mod titling;
mod turn_meta;

pub use admin::AdminDeps;
pub use backfill::BackfillReport;
pub use code::CodeStartResult;
pub(crate) use hooks::code_detail;
pub(crate) use titling::exchange_snippet;

use crate::domain::contracts::{OutboundTx, PlatformDelivery, ProviderFactory};
use hooks::SessionEntry;
use regent_agent::AgentConfig;
use regent_kernel::SessionId;
use regent_skills::SkillLibrary;
use regent_store::Store;
use regent_tools::DeliverySink;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, Weak};
use tokio::sync::Mutex;

/// Wall-clock seconds since the epoch — the stamp the idle-review sweep
/// compares against. Seconds are plenty: the threshold is minutes.
pub(super) fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// How long a session must sit untouched before its tail is learned from.
/// Long enough that a pause mid-conversation isn't mistaken for the end;
/// short enough that walking away still teaches Regent something.
pub(super) const IDLE_REVIEW_AFTER_SECS: u64 = 10 * 60;

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
    /// ADR-038 P1: route plain chat sessions to the `light` profile
    /// (config `tools.light_profile`; the kill-switch).
    light_profile: bool,
    /// Gap S7 lifecycle hooks (config `tools.hook_tool_start` /
    /// `tools.hook_tool_complete`); `None` when neither is set.
    shell_hook: Option<Arc<regent_tools::ShellHook>>,
    /// Auto mode (config `tools.auto_approve`): approve every tool gate
    /// without prompting. Shared into each session's approval handler and
    /// checked per request, so a `config.set` toggle reaches OPEN sessions
    /// immediately — not just new ones.
    auto_approve: Arc<std::sync::atomic::AtomicBool>,
    entries: Mutex<HashMap<SessionId, SessionEntry>>,
    out_tx: OutboundTx,
    /// Routes keyed platform sessions' outbound to the platform API. Filled by
    /// the composition root once the webhook registry exists; empty → every
    /// session uses the CLI-notification sink (the prior behavior).
    platform_delivery: OnceLock<Arc<dyn PlatformDelivery>>,
    /// Reviewer model override (config `model.review`) — the learning loop
    /// runs on this model when set, instead of each session's chat provider.
    /// Set at boot via `set_review_model`; `None` = inherit (the default).
    review_model: std::sync::Mutex<Option<String>>,
    /// Self-handle for the in-process `regent` admin tool to build a dispatcher.
    /// Set by `install_admin`; absent → the tool isn't registered (e.g. tests).
    self_ref: OnceLock<Weak<SessionManager>>,
    /// Cron/config/speech the admin dispatcher needs. Set by `install_admin`.
    admin: OnceLock<AdminDeps>,
    /// Durable record of every job that outlives the turn that started it
    /// (W1). Shared, not per-session: a job survives the session that filed it,
    /// and — unlike the `Vec` this replaced — the process that ran it.
    jobs: Arc<crate::application::jobs::JobLedger>,
    /// How many detached jobs may RUN at once. Shared for the same reason the
    /// ledger is: the catalog builds a fresh `BackgroundTaskTool` per session,
    /// so a cap owned by the tool would bound each session separately and the
    /// process not at all.
    job_slots: Arc<tokio::sync::Semaphore>,
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
        // Boot recovery runs here, once, before any session exists: a job the
        // previous process left running is marked `interrupted` so it is
        // delivered as news instead of disappearing (W1).
        let jobs = Arc::new(crate::application::jobs::JobLedger::new(Arc::clone(&store)));
        jobs.recover();
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
            light_profile: tools_cfg.light_profile,
            shell_hook: {
                let hook = regent_tools::ShellHook::new(
                    &tools_cfg.hook_tool_start,
                    &tools_cfg.hook_tool_complete,
                );
                hook.is_active().then(|| Arc::new(hook))
            },
            auto_approve: Arc::new(std::sync::atomic::AtomicBool::new(tools_cfg.auto_approve)),
            entries: Mutex::new(HashMap::new()),
            out_tx,
            platform_delivery: OnceLock::new(),
            review_model: std::sync::Mutex::new(None),
            self_ref: OnceLock::new(),
            admin: OnceLock::new(),
            jobs,
            job_slots: Arc::new(tokio::sync::Semaphore::new(
                crate::application::background_task_tool::MAX_CONCURRENT_JOBS,
            )),
        }
    }

    /// The durable job ledger (W1) — shared with the `background_task` tool and
    /// the turn paths that relay finished work.
    #[must_use]
    pub fn jobs(&self) -> Arc<crate::application::jobs::JobLedger> {
        Arc::clone(&self.jobs)
    }

    /// The process-wide pool of background-job slots.
    #[must_use]
    pub fn job_slots(&self) -> Arc<tokio::sync::Semaphore> {
        Arc::clone(&self.job_slots)
    }

    /// The memory graph, for the turn path's shadow measurement (W3).
    #[must_use]
    pub fn graph(&self) -> &Arc<regent_graph::GraphMemory> {
        &self.graph
    }

    /// Installs the platform-delivery resolver (composition root, after the
    /// webhook registry is built). Idempotent: a second call is ignored.
    pub fn set_platform_delivery(&self, delivery: Arc<dyn PlatformDelivery>) {
        let _ = self.platform_delivery.set(delivery);
    }

    /// Live-reload hook for `tools.auto_approve` (`config.set` path). Every
    /// session's handler shares this flag, so the flip applies mid-session.
    pub fn set_auto_approve(&self, on: bool) {
        self.auto_approve
            .store(on, std::sync::atomic::Ordering::Release);
    }

    /// Reviewer model override (config `model.review`) — applies to sessions
    /// built after the call; `None` restores inherit-the-chat-model.
    pub fn set_review_model(&self, model: Option<String>) {
        *self.review_model.lock().unwrap() = model;
    }

    /// The platform sink for a keyed session, if the key names a known outbound
    /// webhook target. `None` for local CLI sessions and unkeyed creation.
    pub(super) fn platform_sink(&self, key: Option<&str>) -> Option<Arc<dyn DeliverySink>> {
        let key = key?;
        self.platform_delivery.get()?.sink_for(key)
    }
}
