//! regent-daemon — composition root (canonical `app/di`).
//!
//! Env (required for a live provider):
//!   REGENT_API_KEY   — provider API key
//!   REGENT_MODEL     — model name (e.g. claude-sonnet-4-6)
//!   REGENT_BASE_URL  — provider base URL (default: https://openrouter.ai/api)
//!   REGENT_HOME      — override for ~/.regent
//!
//! Wire-up: config.yaml → store → graph → skills → provider →
//!          session_manager → dispatcher → stdio JSON-RPC loop.

use regent_agent::{AgentConfig, AgentJobRunner, CompressionConfig};
use regent_daemon::{
    Dispatcher, ProviderKind, SessionManager, load_config, make_provider_factory, spawn_write_loop,
};
use regent_skills::FsSkillRepository;
use regent_tools::{DenyAll, ToolContext, core_catalog};
use std::path::PathBuf;
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
    let _log_guard = regent_daemon::init_logging(&home.join("logs"));

    // ── Config ────────────────────────────────────────────────────────────────
    let cfg = load_config(&home)?;

    // ── Persistence ───────────────────────────────────────────────────────────
    let store = Arc::new(regent_store::Store::open(&home.join("state.db"))?);

    // ── Memory embedder (local ONNX semantic lane) ─────────────────────────────
    // Attached in the background so a slow first-run model download never blocks
    // boot; memory runs on FTS + graph until it binds (see background::attach_embedder).
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    if cfg.memory.embeddings {
        regent_daemon::attach_embedder(Arc::clone(&graph));
    }

    let skills = Arc::new(regent_skills::SkillLibrary::new(Arc::new(
        FsSkillRepository::new(home.join("skills"))?,
    )));

    // ── Provider factory (env wins over config; daemon still boots without a
    //    key for tests — runtime errors surface on the first prompt.submit). A
    //    factory (not a fixed provider) lets `model.set` rebuild per session. ──
    let api_key = std::env::var("REGENT_API_KEY").unwrap_or_default();
    let initial_model = std::env::var("REGENT_MODEL").unwrap_or_else(|_| cfg.model.default.clone());
    let kind = ProviderKind::from_env_or(cfg.model.provider);
    let base_url_override = std::env::var("REGENT_BASE_URL")
        .ok()
        .or_else(|| cfg.model.base_url.clone());
    let provider_factory = make_provider_factory(kind, api_key.clone(), base_url_override.clone());
    let provider = provider_factory(&initial_model); // for the cron runner
    tracing::info!(provider = ?kind, model = %initial_model, "model provider selected");

    // ── Write loop ────────────────────────────────────────────────────────────
    let out_tx = spawn_write_loop();

    // ── Session manager ───────────────────────────────────────────────────────
    // config.yaml is the single behavior source: context settings flow into
    // every session's AgentConfig through this template.
    let agent_template = AgentConfig {
        max_context_tokens: cfg.context.max_tokens,
        compression: CompressionConfig {
            trigger_fraction: cfg.context.trigger_fraction,
            protect_last_n: cfg.context.protect_last_n,
            ..CompressionConfig::default()
        },
        ..AgentConfig::default()
    };
    let sessions = Arc::new(SessionManager::new(
        provider_factory,
        initial_model,
        Arc::clone(&store),
        Arc::clone(&graph),
        Arc::clone(&skills),
        std::env::current_dir()?,
        agent_template,
        cfg.tools.disabled.clone(),
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
    let tick_secs = cfg.cron.tick_interval_secs;
    let cron_repo_for_scheduler = Arc::clone(&cron_repo);
    tokio::spawn(async move {
        let scheduler = regent_cron::Scheduler::new(
            cron_repo_for_scheduler,
            cron_runner,
            regent_cron::SchedulerConfig::default(),
        );
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(tick_secs)).await;
            match scheduler.tick(regent_store::now_epoch()).await {
                Ok(outcomes) => {
                    for o in outcomes {
                        tracing::info!(job = o.job_name, status = ?o.status, summary = o.summary, "cron tick");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "cron tick failed"),
            }
        }
    });

    // ── Board dispatcher loop (opt-in; off by default) ──────────────────────
    // Autonomous task execution + its token spend is never enabled silently.
    if cfg.board.enabled {
        // Provider registry for per-agent models (ADR-026); empty providers map
        // ⇒ the resolver no-ops and workers run on the shared provider.
        let registry = Arc::new(regent_daemon::ProviderRegistry::from_config(&cfg.providers));
        regent_daemon::spawn_board_dispatcher(
            Arc::clone(&store),
            Arc::clone(&provider),
            std::env::current_dir()?,
            &cfg.board,
            registry,
            cfg.agents_defaults.clone(),
        );
        tracing::info!(
            tick_secs = cfg.board.tick_interval_secs,
            "board dispatcher loop enabled"
        );
    }

    // ── HTTP listener (opt-in REST ingress; off by default) ──────────────────
    if cfg.http.enabled
        && let Err(error) =
            regent_daemon::spawn_http_listener(Arc::clone(&sessions), &cfg.http).await
    {
        tracing::warn!(%error, "http listener not started");
    }

    // ── Maintenance loops (hourly) ────────────────────────────────────────────
    regent_daemon::spawn_ttl_purge(Arc::clone(&graph));
    regent_daemon::spawn_pending_expiry(Arc::clone(&sessions));
    regent_daemon::spawn_curator(Arc::clone(&skills));

    // Admin context for the in-process `regent` tool: the agent runs its own
    // commands through this same dispatcher surface (no second daemon, no store
    // deadlock). Install before the dispatcher consumes cfg/cron_repo below.
    let speech_exec: Arc<dyn regent_speech::HttpExecutor> =
        Arc::new(regent_daemon::infra::speech_http::ReqwestExecutor::new());
    sessions.install_admin(regent_daemon::AdminDeps {
        cron: Some(Arc::clone(&cron_repo) as Arc<dyn regent_cron::JobRepository>),
        config: Some(cfg.clone()),
        speech: Some(Arc::clone(&speech_exec)),
    });

    // ── JSON-RPC main loop ────────────────────────────────────────────────────
    let dispatcher = Dispatcher::new(Arc::clone(&sessions), out_tx)
        .with_cron(cron_repo)
        .with_config(cfg)
        .with_speech_executor(speech_exec);
    let mut transport = regent_daemon::StdioTransport::new();

    tracing::info!("regent-daemon ready (stdio JSON-RPC 2.0)");

    while let Some(line) = transport.next_line().await {
        let req: regent_daemon::RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, raw = line, "malformed JSON-RPC request");
                continue;
            }
        };
        dispatcher.handle(req).await;
    }

    // ── Graceful shutdown ─────────────────────────────────────────────────────
    tracing::info!("stdin closed — draining sessions");
    sessions.drain().await;
    Ok(())
}

fn regent_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return Ok(custom.into());
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))?;
    Ok(PathBuf::from(home).join(".regent"))
}
