//! Boot helpers for the deacon binary: home-dir resolution, legacy-skill
//! retirement, and the cron scheduler loop. Split from main.rs (file-size rule).

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

/// `doc-forge` predated compiled-in skills and was seeded into the user's
/// repository from a root-level asset. `documents` now owns that capability in
/// `regent-skills`; archive only our legacy copy so it cannot keep steering the
/// model toward Python. A user-authored skill of the same name always wins.
pub(crate) fn retire_legacy_skills(skills: &regent_skills::SkillLibrary) {
    match skills.repository().load("doc-forge") {
        Ok(record) if record.meta.created_by == "bundled" => {
            // `archive` moves an on-disk directory. A skill resolved from the
            // BUNDLED set compiled into the binary has no directory to move, so
            // for those this call can only ever fail — and it warned on EVERY
            // boot (16 times in one day) about work that was never possible.
            // There is also nothing to retire in that case: no user copy exists.
            match skills.repository().archive("doc-forge") {
                Ok(()) => tracing::info!(skill = "doc-forge", "retired legacy bundled skill"),
                Err(regent_skills::SkillError::NotFound(_)) => tracing::debug!(
                    skill = "doc-forge",
                    "bundled-only legacy skill — nothing on disk to archive"
                ),
                Err(error) => {
                    tracing::warn!(skill = "doc-forge", %error, "legacy bundled skill archive failed");
                }
            }
        }
        Ok(_) | Err(regent_skills::SkillError::NotFound(_)) => {}
        Err(error) => tracing::warn!(skill = "doc-forge", %error, "legacy skill lookup failed"),
    }
}

/// Spawns the cron scheduler tick loop.
///
/// The runner is wrapped in [`LedgerCronRunner`] so every execution lands in
/// the shared job ledger (W1/W7) — one ledger for all work, not a second one
/// for cron. That also gives cron its overlap guard: a tick that fires while
/// the previous execution is still running is refused rather than stacking.
pub(crate) fn spawn_cron(
    cron_repo: &Arc<regent_cron::FsJobRepository>,
    cron_runner: Arc<AgentJobRunner>,
    jobs: Arc<regent_jobs::JobLedger>,
    tick_secs: u64,
) {
    let cron_repo_for_scheduler = Arc::clone(cron_repo);
    let runner = Arc::new(regent_jobs::LedgerCronRunner::new(cron_runner, jobs));
    tokio::spawn(async move {
        let scheduler = regent_cron::Scheduler::new(
            cron_repo_for_scheduler,
            runner,
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
        context_windows: cfg.context.windows.clone(),
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

#[cfg(test)]
mod tests {
    use super::retire_legacy_skills;
    use regent_skills::{FsSkillRepository, SkillLibrary, SkillMeta};
    use std::sync::Arc;

    fn library() -> (tempfile::TempDir, SkillLibrary) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(FsSkillRepository::new(dir.path()).unwrap());
        (dir, SkillLibrary::new(repo))
    }

    #[test]
    fn bundled_doc_forge_is_archived() {
        let (_dir, skills) = library();
        let mut meta = SkillMeta::new(
            "doc-forge",
            "Build designed pptx, docx, xlsx, PDF, and CSV files.",
            "bundled",
        );
        meta.version = "0.1.0".into();
        skills
            .repository()
            .save(&meta, "legacy python instructions")
            .unwrap();

        retire_legacy_skills(&skills);

        assert!(
            !skills
                .list()
                .unwrap()
                .iter()
                .any(|summary| summary.name == "doc-forge")
        );
        let archived = skills.repository().list_archived().unwrap();
        assert!(
            archived
                .iter()
                .any(|record| record.body == "legacy python instructions")
        );
    }

    #[test]
    fn user_owned_doc_forge_is_never_replaced() {
        let (_dir, skills) = library();
        let meta = SkillMeta::new(
            "doc-forge",
            "Build designed pptx, docx, xlsx, PDF, and CSV files.",
            "user",
        );
        skills
            .repository()
            .save(&meta, "my custom workflow")
            .unwrap();

        retire_legacy_skills(&skills);

        let current = skills.repository().load("doc-forge").unwrap();
        assert_eq!(current.meta.created_by, "user");
        assert_eq!(current.body, "my custom workflow");
        assert!(skills.repository().list_archived().unwrap().is_empty());
    }
}
