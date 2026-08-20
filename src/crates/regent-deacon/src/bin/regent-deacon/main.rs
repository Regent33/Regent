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
mod config_cli;
mod mcp;
mod routing;

use boot::{regent_home, retire_legacy_skills, spawn_cron};
use regent_agent::AgentJobRunner;
use regent_deacon::{Dispatcher, ProviderFactory, SessionManager, load_config, spawn_write_loop};
use regent_skills::FsSkillRepository;
use regent_tools::{DenyAll, ToolContext, core_catalog_from_env};
use routing::{provider_factory_from, routing_from};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Offline subcommands run and exit before any of the daemon wiring below —
    // no store, no provider, no logging file, no stdio protocol. A config bad
    // enough to stop the daemon is exactly when these have to still work.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "config") {
        let home = regent_home().unwrap_or_else(|e| {
            eprintln!("fatal: {e}");
            std::process::exit(1);
        });
        std::process::exit(config_cli::run(&home, &args[1..]));
    }
    // `regent mcp serve` — an MCP client spawns us with this argument and talks
    // JSON-RPC over stdio. Never returns; the daemon wiring below is not reached.
    if args.first().is_some_and(|a| a == "mcp") {
        mcp::serve().await;
    }
    if let Err(e) = run().await {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let home = regent_home()?;
    std::fs::create_dir_all(&home)?;
    // Publish the resolved home so every crate's `$REGENT_HOME`-relative lookup
    // agrees with the one this process actually uses. `regent_home()` FALLS BACK
    // to `~/.regent` without setting the variable, so anything reading the env
    // directly simply failed whenever the parent hadn't set it — silently. That
    // took out the whole `.env` credential path (`key_tool::env_path` returns
    // "REGENT_HOME is not set"): the boot merge below, `run_turn`'s live
    // re-merge, and `manage_keys`/`env_var_status` all no-opped for any deacon
    // spawned without it — a CLI one-shot, the voice server's, this test
    // harness. Idempotent when the parent already set it.
    // SAFETY: single-threaded boot, before any task is spawned.
    unsafe { std::env::set_var("REGENT_HOME", &home) };
    // Base area for agent-generated artifacts (one subfolder per object); the
    // system prompt points the agent here (see session_manager::artifacts_line).
    std::fs::create_dir_all(home.join("artifacts"))?;

    // Logs to stderr (stdout carries the JSON-RPC stream) + a redacted rolling
    // file under $REGENT_HOME/logs/. Guard flushes on drop — hold it for the
    // whole run.
    let _log_guard = regent_deacon::init_logging(&home.join("logs"));

    // ── Credentials ───────────────────────────────────────────────────────────
    // Merge `$REGENT_HOME/.env` into the process env BEFORE anything resolves a
    // provider. Only `run_turn` used to do this, so every provider built before
    // the first turn — the cron runner's, and the routing snapshot logged below
    // — saw no keys at all. A fallback chain drops members whose key is missing,
    // so it collapsed to a single provider and the process ran with NO failover
    // while the keys sat in `.env` the whole time ("fallback chain
    // unresolvable; using single provider … has no API key", 20 times in one day
    // on a machine whose .env had both keys). The per-turn re-merge stays: it is
    // what picks up a key saved while the deacon is already running.
    let merged = regent_tools::reload_credentials_from_dotenv();
    if merged > 0 {
        tracing::info!(merged, "merged credentials from .env at boot");
    }

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
    retire_legacy_skills(&skills);

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
    // `model.review`: the learning loop grades sessions on this model instead
    // of each session's chat model (weak chat models grade themselves as
    // "Nothing to save"). Applies to sessions built from here on.
    sessions.set_review_model(cfg.model.review.clone());

    // ── Cron loop ─────────────────────────────────────────────────────────────
    let cron_repo = Arc::new(regent_cron::FsJobRepository::new(home.join("cron"))?);
    // From ENV so `REGENT_SANDBOX` applies to scheduled jobs as well. With
    // `core_catalog()` a cron job ran host shell commands with the flag set,
    // silently — no sandbox and no error.
    let cron_runner = Arc::new(AgentJobRunner::new(
        Arc::clone(&provider),
        Arc::new(core_catalog_from_env()?),
        Arc::clone(&store),
        ToolContext::new(std::env::current_dir()?, Arc::new(DenyAll)),
        "You are Regent running a scheduled job. Do the task, then summarize.",
    ));
    spawn_cron(
        &cron_repo,
        cron_runner,
        sessions.jobs(),
        cfg.cron.tick_interval_secs,
    );

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

    // ── Background update check (ADR-041, Phase 0: notify only) ───────────────
    // Detached so boot and chat turns NEVER wait on it: one cached, conditional
    // GET per home per day (ETag + deterministic jitter). A failure keeps the
    // last good cache and stays silent; `REGENT_NO_UPDATE_CHECK=1` opts out.
    let update_checker = Arc::new(regent_deacon::UpdateChecker::new(
        home.clone(),
        env!("CARGO_PKG_VERSION").to_string(),
    ));
    Arc::clone(&update_checker).spawn();

    // ── Idle-session review sweep ─────────────────────────────────────────────
    // Learn from conversations the user walked away from. Without this the
    // reviewer's batch gate (8 new messages) meant a short chat was never
    // learned from at all unless the whole process happened to shut down.
    {
        let sessions = Arc::clone(&sessions);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(120));
            // The first tick fires immediately; skip it so boot isn't a sweep.
            tick.tick().await;
            loop {
                tick.tick().await;
                sessions.sweep_idle_reviews().await;
            }
        });
    }

    // ── JSON-RPC main loop ────────────────────────────────────────────────────
    let dispatcher = Dispatcher::new(Arc::clone(&sessions), out_tx)
        .with_cron(cron_repo)
        .with_config(cfg)
        .with_reload(reload)
        .with_speech_executor(speech_exec)
        .with_update_checker(update_checker);
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
