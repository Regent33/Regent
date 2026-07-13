//! regent-deacon — composition root (canonical `app/di`).
//!
//! Env (required for a live provider):
//!   REGENT_API_KEY   — provider API key
//!   REGENT_MODEL     — model name (e.g. claude-sonnet-4-6)
//!   REGENT_BASE_URL  — provider base URL (default: https://openrouter.ai/api)
//!   REGENT_HOME      — override for ~/.regent
//!
//! Wire-up: config.yaml → store → graph → skills → provider →
//!          session_manager → dispatcher → stdio JSON-RPC loop.

mod boot;
mod routing;

use boot::{regent_home, seed_bundled_skills, spawn_cron};
use regent_agent::AgentJobRunner;
use regent_deacon::{Dispatcher, ProviderFactory, SessionManager, load_config, spawn_write_loop};
use regent_skills::FsSkillRepository;
use regent_tools::{DenyAll, ToolContext, core_catalog};
use routing::{provider_factory_from, routing_from};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let home = regent_home()?;
    std::fs::create_dir_all(&home)?;
    // Base area for agent-generated artifacts (one subfolder per object); the
    // system prompt points the agent here (see session_manager::artifacts_line).
    std::fs::create_dir_all(home.join("artifacts"))?;

    // Logs to stderr (stdout carries the JSON-RPC stream) + a redacted rolling
    // file under $REGENT_HOME/logs/. Guard flushes on drop — hold it for the
    // whole run.
    let _log_guard = regent_deacon::init_logging(&home.join("logs"));

    // ── Config ────────────────────────────────────────────────────────────────
    let cfg = load_config(&home)?;

    // ── Persistence ───────────────────────────────────────────────────────────
    let store = Arc::new(regent_store::Store::open(&home.join("state.db"))?);
    // Sweep abandoned empty sessions (no messages/turns/children) so the rail
    // never accumulates "0 messages" rows. 1h grace: another live process may
    // have just created one it's about to use.
    match store.delete_empty_sessions(3_600.0) {
        Ok(0) => {}
        Ok(n) => tracing::info!(deleted = n, "swept empty sessions"),
        Err(error) => tracing::warn!(%error, "empty-session sweep failed"),
    }

    // ── Memory embedder (local ONNX semantic lane) ─────────────────────────────
    // Attached in the background so a slow first-run model download never blocks
    // boot; memory runs on FTS + graph until it binds (see background::attach_embedder).
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    if cfg.memory.embeddings {
        regent_deacon::attach_embedder(Arc::clone(&graph));
    }

    // ── Constitution (opt-in values layer, config.yaml `constitution.enabled`) ─
    // Core into the persona row; the full document into graph memory as
    // retrievable section nodes — or both removed when disabled (ADR-028).
    regent_deacon::sync_constitution(cfg.constitution.enabled, &store, &graph)?;

    let skills = Arc::new(regent_skills::SkillLibrary::new(Arc::new(
        FsSkillRepository::new(home.join("skills"))?,
    )));
    seed_bundled_skills(&skills);

    // ── Provider routing (LIVE — config.set / env.set rebuild it at runtime,
    //    so key/provider/model changes reach the NEXT session with no restart).
    //    Keys resolve at session-build time (never captured at boot). With
    //    `agents_defaults.primary` set, chat runs the primary → fallbacks chain
    //    (FallbackChat reroutes on transport/5xx/auth/rate-limit, never 4xx);
    //    `model.set` re-routes it: a "<provider>/<model>" pick (model.list's id
    //    format) becomes the chain's NEW primary with the configured chain as
    //    fallbacks. ──
    // Boot-time active model: REGENT_MODEL override, else the applied
    // `agents_defaults.primary` ("provider/model" — the id the registry and
    // model.set use), else the legacy `model.default`. Without the primary
    // here, a restart re-pointed chat at the old default and the UI honestly
    // showed it — undoing the user's applied pick.
    let initial_model = routing::initial_model_from(&cfg);
    let routing = Arc::new(std::sync::RwLock::new(routing_from(&cfg)));
    // Write loop first, so the provider factory can hand FallbackChat a callback
    // that surfaces runtime failovers to the UI over the same notification path.
    let out_tx = spawn_write_loop();
    let provider_factory: ProviderFactory = provider_factory_from(&routing, &out_tx);
    let reload: regent_deacon::ConfigReload = {
        let routing = Arc::clone(&routing);
        Arc::new(move |cfg: &regent_deacon::DeaconConfig| {
            *routing.write().unwrap() = routing_from(cfg);
            tracing::info!("provider routing reloaded from config/env change");
        })
    };
    let provider = provider_factory(&initial_model); // for the cron runner
    // Log the ACTUAL provider the active model resolves to through the registry
    // — NOT the legacy `kind` (the single-provider/cron default). A chain id like
    // "nvidia/z-ai/glm-5.2" routes to its own provider; logging `kind` made it
    // look like it ran on ollama when it did not.
    routing::log_selected_provider(&routing, &initial_model);

    // ── Session manager ───────────────────────────────────────────────────────
    // config.yaml is the single behavior source: context settings flow into
    // every session's AgentConfig through this template.
    let agent_template = boot::agent_template_from(&cfg);
    let sessions = Arc::new(SessionManager::new(
        provider_factory,
        initial_model,
        Arc::clone(&store),
        Arc::clone(&graph),
        Arc::clone(&skills),
        std::env::current_dir()?,
        agent_template,
        cfg.tools.clone(),
        out_tx.clone(),
    ));

    // ── Cron loop ─────────────────────────────────────────────────────────────
    let cron_repo = Arc::new(regent_cron::FsJobRepository::new(home.join("cron"))?);
    let cron_runner = Arc::new(AgentJobRunner::new(
        Arc::clone(&provider),
        Arc::new(core_catalog()),
        Arc::clone(&store),
        ToolContext::new(std::env::current_dir()?, Arc::new(DenyAll)),
        "You are Regent running a scheduled job. Do the task, then summarize.",
    ));
    spawn_cron(&cron_repo, cron_runner, cfg.cron.tick_interval_secs);

    boot::spawn_services(&cfg, &store, &provider, &graph, &skills, &sessions).await?;

    // Admin context for the in-process `regent` tool: the agent runs its own
    // commands through this same dispatcher surface (no second deacon, no store
    // deadlock). Install before the dispatcher consumes cfg/cron_repo below.
    let speech_exec: Arc<dyn regent_speech::HttpExecutor> =
        Arc::new(regent_deacon::infra::speech_http::ReqwestExecutor::new());
    sessions.install_admin(regent_deacon::AdminDeps {
        cron: Some(Arc::clone(&cron_repo) as Arc<dyn regent_cron::JobRepository>),
        config: Some(cfg.clone()),
        speech: Some(Arc::clone(&speech_exec)),
    });

    // ── JSON-RPC main loop ────────────────────────────────────────────────────
    let dispatcher = Dispatcher::new(Arc::clone(&sessions), out_tx)
        .with_cron(cron_repo)
        .with_config(cfg)
        .with_reload(reload)
        .with_speech_executor(speech_exec);
    let mut transport = regent_deacon::StdioTransport::new();

    tracing::info!("regent-deacon ready (stdio JSON-RPC 2.0)");

    while let Some(line) = transport.next_line().await {
        let req: regent_deacon::RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, raw = line, "malformed JSON-RPC request");
                continue;
            }
        };
        dispatcher.handle(req).await;
    }

    // ── Keepalive (background service mode) ───────────────────────────────────
    // `--keepalive` / REGENT_KEEPALIVE=1: run without a connected client so
    // the cron/board loops keep firing — the mode `regent cron autostart`
    // registers at logon. Cron's tick lock keeps a session-spawned deacon
    // from double-firing jobs alongside this one.
    let keepalive = boot::keepalive_requested();
    if keepalive {
        tracing::info!("stdin closed — keepalive mode, cron/board loops stay up");
        std::future::pending::<()>().await
    }

    // ── Graceful shutdown ─────────────────────────────────────────────────────
    tracing::info!("stdin closed — draining sessions");
    sessions.drain().await;
    Ok(())
}
