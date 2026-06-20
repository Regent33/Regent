//! Board dispatcher wiring + tick loop (opt-in). Mirrors the cron loop: builds
//! an agent-backed worker + reviewer over the shared `default` board and spawns
//! a background tick that drains claimable tasks. The caller gates this on
//! `BoardConfig::enabled` — autonomous task execution is never on by default.

use crate::domain::config::BoardConfig;
use regent_agent::{AgentReviewer, AgentTaskRunner, BoardDispatcher};
use regent_providers::ChatProvider;
use regent_store::Store;
use regent_tools::{DenyAll, ToolContext, core_catalog};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// The board every daemon session shares (matches `session_manager/build.rs`).
const DEFAULT_BOARD: &str = "default";

const WORKER_PROMPT: &str =
    "You are Regent running a board task. Do the task, then briefly summarize what you did.";

/// Builds the worker + reviewer dispatcher over the default board and spawns
/// its tick loop. Worker and reviewer run with `DenyAll` (no autonomous
/// terminal/destructive actions) — the safe default for unattended execution.
pub fn spawn_board_dispatcher(
    store: Arc<Store>,
    provider: Arc<dyn ChatProvider>,
    cwd: PathBuf,
    cfg: &BoardConfig,
) {
    let runner = Arc::new(AgentTaskRunner::new(
        Arc::clone(&provider),
        Arc::new(core_catalog()),
        Arc::clone(&store),
        ToolContext::new(cwd.clone(), Arc::new(DenyAll)),
        WORKER_PROMPT,
    ));
    let reviewer = Arc::new(AgentReviewer::new(
        provider,
        Arc::new(core_catalog()),
        Arc::clone(&store),
        ToolContext::new(cwd, Arc::new(DenyAll)),
    ));
    let dispatcher =
        Arc::new(BoardDispatcher::new(store, runner, "regent").with_reviewer(reviewer));

    let (interval, max) = (cfg.tick_interval_secs, cfg.max_per_tick);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            match dispatcher.dispatch_pending(DEFAULT_BOARD, max).await {
                Ok(outcomes) => {
                    for o in outcomes {
                        tracing::info!(task = o.id, status = o.status, "board tick");
                    }
                }
                Err(error) => tracing::warn!(%error, "board tick failed"),
            }
        }
    });
}
