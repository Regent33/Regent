//! Boot helpers for the deacon binary: home-dir resolution, bundled-skill
//! seeding, and the cron scheduler loop. Split from main.rs (file-size rule).

use regent_agent::{AgentConfig, AgentJobRunner, CompressionConfig};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn regent_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return Ok(custom.into());
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))?;
    Ok(PathBuf::from(home).join(".regent"))
}

/// Seeds skills shipped inside the binary into `$REGENT_HOME/skills` — once:
/// an existing skill of the same name is the user's (edits win, never overwrite).
pub(crate) fn seed_bundled_skills(skills: &regent_skills::SkillLibrary) {
    const BUNDLED: &[&str] = &[include_str!("../../../../../../skills/doc-forge/SKILL.md")];
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

/// Spawns the cron scheduler tick loop.
pub(crate) fn spawn_cron(
    cron_repo: &Arc<regent_cron::FsJobRepository>,
    cron_runner: Arc<AgentJobRunner>,
    tick_secs: u64,
) {
    let cron_repo_for_scheduler = Arc::clone(cron_repo);
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
}

/// config.yaml is the single behavior source: context settings flow into
/// every session's AgentConfig through this template.
pub(crate) fn agent_template_from(cfg: &regent_deacon::DeaconConfig) -> AgentConfig {
    AgentConfig {
        max_context_tokens: cfg.context.max_tokens,
        max_turn_tokens: cfg.limits.max_turn_tokens,
        compression: CompressionConfig {
            trigger_fraction: cfg.context.trigger_fraction,
            protect_last_n: cfg.context.protect_last_n,
            prune_after_turns: cfg.context.prune_after_turns,
            ..CompressionConfig::default()
        },
        ..AgentConfig::default()
    }
}

/// `--keepalive` / REGENT_KEEPALIVE=1: run without a connected client so the
/// cron/board loops keep firing (the mode `regent cron autostart` registers).
pub(crate) fn keepalive_requested() -> bool {
    std::env::args().any(|a| a == "--keepalive")
        || std::env::var("REGENT_KEEPALIVE").is_ok_and(|v| matches!(v.trim(), "1" | "true"))
}

/// Opt-in board dispatcher + HTTP ingress, and the hourly maintenance loops.
pub(crate) async fn spawn_services(
    cfg: &regent_deacon::DeaconConfig,
    store: &Arc<regent_store::Store>,
    provider: &Arc<dyn regent_providers::ChatProvider>,
    graph: &Arc<regent_graph::GraphMemory>,
    skills: &Arc<regent_skills::SkillLibrary>,
    sessions: &Arc<regent_deacon::SessionManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    // ── Board dispatcher loop (opt-in; off by default) ──────────────────────
    // Autonomous task execution + its token spend is never enabled silently.
    if cfg.board.enabled {
        // Provider registry for per-agent models (ADR-026); empty providers map
        // ⇒ the resolver no-ops and workers run on the shared provider.
        let registry = Arc::new(regent_deacon::ProviderRegistry::from_config(&cfg.providers));
        regent_deacon::spawn_board_dispatcher(
            Arc::clone(store),
            Arc::clone(provider),
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
            regent_deacon::spawn_http_listener(Arc::clone(sessions), &cfg.http).await
    {
        tracing::warn!(%error, "http listener not started");
    }

    // ── Maintenance loops (hourly) ────────────────────────────────────────────
    regent_deacon::spawn_ttl_purge(Arc::clone(graph));
    regent_deacon::spawn_pending_expiry(Arc::clone(sessions));
    regent_deacon::spawn_curator(Arc::clone(skills));
    // SPL P5: the Distiller watches persona-store fill and stages human-gated
    // consolidation proposals (memory.pending) before budgets fail-closed.
    regent_deacon::spawn_distiller(Arc::clone(store), Arc::clone(provider));
    Ok(())
}
