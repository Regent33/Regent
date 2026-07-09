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

use regent_agent::{AgentConfig, AgentJobRunner, CompressionConfig};
use regent_deacon::{
    Dispatcher, ProviderFactory, ProviderKind, ProviderRegistry, SessionManager, load_config,
    make_provider_factory, spawn_write_loop,
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
    let _log_guard = regent_deacon::init_logging(&home.join("logs"));

    // ── Config ────────────────────────────────────────────────────────────────
    let cfg = load_config(&home)?;

    // ── Persistence ───────────────────────────────────────────────────────────
    let store = Arc::new(regent_store::Store::open(&home.join("state.db"))?);

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
    let initial_model = std::env::var("REGENT_MODEL").unwrap_or_else(|_| cfg.model.default.clone());
    let kind = ProviderKind::from_env_or(cfg.model.provider);
    let routing = Arc::new(std::sync::RwLock::new(routing_from(&cfg)));
    let provider_factory: ProviderFactory = {
        let routing = Arc::clone(&routing);
        Arc::new(move |model: &str| {
            let r = routing.read().unwrap();
            let single =
                || make_provider_factory(r.kind, r.kind.resolve_key(), r.base_url.clone())(model);
            match &r.primary {
                Some(primary) if !r.registry.is_empty() => {
                    let picked = r
                        .registry
                        .resolve_model_str(model, Some(primary))
                        .unwrap_or_else(|| primary.clone());
                    let mut chain_fallbacks = Vec::new();
                    if picked != *primary {
                        chain_fallbacks.push(primary.clone());
                    }
                    chain_fallbacks.extend(r.fallbacks.iter().filter(|f| **f != picked).cloned());
                    r.registry
                        .chain_for(&picked, &chain_fallbacks)
                        .unwrap_or_else(|e| {
                            tracing::warn!(%e, "fallback chain unresolvable; using single provider");
                            single()
                        })
                }
                _ => single(),
            }
        })
    };
    let reload: regent_deacon::ConfigReload = {
        let routing = Arc::clone(&routing);
        Arc::new(move |cfg: &regent_deacon::DeaconConfig| {
            *routing.write().unwrap() = routing_from(cfg);
            tracing::info!("provider routing reloaded from config/env change");
        })
    };
    let provider = provider_factory(&initial_model); // for the cron runner
    tracing::info!(provider = ?kind, model = %initial_model, "model provider selected");

    // ── Write loop ────────────────────────────────────────────────────────────
    let out_tx = spawn_write_loop();

    // ── Session manager ───────────────────────────────────────────────────────
    // config.yaml is the single behavior source: context settings flow into
    // every session's AgentConfig through this template.
    let agent_template = AgentConfig {
        max_context_tokens: cfg.context.max_tokens,
        max_turn_tokens: cfg.limits.max_turn_tokens,
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
        let registry = Arc::new(regent_deacon::ProviderRegistry::from_config(&cfg.providers));
        regent_deacon::spawn_board_dispatcher(
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
            regent_deacon::spawn_http_listener(Arc::clone(&sessions), &cfg.http).await
    {
        tracing::warn!(%error, "http listener not started");
    }

    // ── Maintenance loops (hourly) ────────────────────────────────────────────
    regent_deacon::spawn_ttl_purge(Arc::clone(&graph));
    regent_deacon::spawn_pending_expiry(Arc::clone(&sessions));
    regent_deacon::spawn_curator(Arc::clone(&skills));

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
    let keepalive = std::env::args().any(|a| a == "--keepalive")
        || std::env::var("REGENT_KEEPALIVE").is_ok_and(|v| matches!(v.trim(), "1" | "true"));
    if keepalive {
        tracing::info!("stdin closed — keepalive mode, cron/board loops stay up");
        std::future::pending::<()>().await
    }

    // ── Graceful shutdown ─────────────────────────────────────────────────────
    tracing::info!("stdin closed — draining sessions");
    sessions.drain().await;
    Ok(())
}

/// Live provider-routing state — one snapshot per config/env change. The
/// factory reads it per session build; the reload hook replaces it whole
/// (also dropping the old registry's memoized providers, so rotated keys and
/// edited provider entries take effect).
struct Routing {
    registry: ProviderRegistry,
    primary: Option<regent_kernel::ModelRef>,
    fallbacks: Vec<regent_kernel::ModelRef>,
    kind: ProviderKind,
    base_url: Option<String>,
}

fn routing_from(cfg: &regent_deacon::DeaconConfig) -> Routing {
    Routing {
        registry: ProviderRegistry::from_config(&cfg.providers),
        primary: cfg.agents_defaults.primary.clone(),
        fallbacks: cfg.agents_defaults.fallbacks.clone(),
        // Env still wins at boot AND on reload (same precedence as before).
        kind: ProviderKind::from_env_or(cfg.model.provider),
        base_url: std::env::var("REGENT_BASE_URL")
            .ok()
            .or_else(|| cfg.model.base_url.clone()),
    }
}

fn regent_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return Ok(custom.into());
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))?;
    Ok(PathBuf::from(home).join(".regent"))
}

/// Seeds skills shipped inside the binary into `$REGENT_HOME/skills` — once:
/// an existing skill of the same name is the user's (edits win, never overwrite).
fn seed_bundled_skills(skills: &regent_skills::SkillLibrary) {
    const BUNDLED: &[&str] = &[include_str!("../../../../../skills/doc-forge/SKILL.md")];
    for raw in BUNDLED {
        let mut parts = raw.splitn(3, "---");
        let (Some(_), Some(front), Some(body)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let field = |key: &str| {
            front
                .lines()
                .find_map(|l| {
                    l.strip_prefix(&format!("{key}:"))
                        .map(|v| v.trim().to_owned())
                })
                .unwrap_or_default()
        };
        let (name, description) = (field("name"), field("description"));
        if name.is_empty() {
            continue;
        }
        match skills.create(&name, &description, body.trim(), "bundled") {
            Ok(()) => tracing::info!(skill = %name, "seeded bundled skill"),
            Err(regent_skills::SkillError::AlreadyExists(_)) => {}
            Err(e) => tracing::warn!(skill = %name, %e, "bundled skill seed failed"),
        }
    }
}
