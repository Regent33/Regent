//! Background loops spawned at boot by the composition root. Each is a
//! self-contained `tokio::spawn` extracted from the bin so the composition
//! root stays linear wiring. (The board dispatcher loop lives in its own
//! `board_dispatch` module; the cron loop stays in the bin, coupled to the
//! cron repository the JSON-RPC dispatcher also holds.)

use crate::application::session_manager::SessionManager;
use regent_graph::GraphMemory;
use regent_skills::{CuratorConfig, SkillLibrary, curate};
use std::sync::Arc;
use std::time::Duration;

const HOURLY: u64 = 3_600;
const CURATOR_INTERVAL: u64 = 6 * HOURLY;

/// Periodic skill curation (the Hermes inactivity-curator analog): every
/// `CURATOR_INTERVAL`, transition stale agent-created skills toward archived
/// over usage telemetry. Deterministic + idempotent; pinned/user skills are
/// exempt. The first pass waits one interval so short CLI sessions skip it.
pub fn spawn_curator(skills: Arc<SkillLibrary>) {
    tokio::spawn(async move {
        let config = CuratorConfig::default();
        loop {
            tokio::time::sleep(Duration::from_secs(CURATOR_INTERVAL)).await;
            let skills = Arc::clone(&skills);
            let config = config.clone();
            match tokio::task::spawn_blocking(move || {
                curate(&skills, regent_store::now_epoch(), &config)
            })
            .await
            {
                Ok(Ok(report)) if !report.archived.is_empty() || !report.marked_stale.is_empty() => {
                    tracing::info!(
                        archived = report.archived.len(),
                        stale = report.marked_stale.len(),
                        "skill curation pass"
                    );
                }
                Ok(Err(error)) => tracing::warn!(%error, "skill curation failed"),
                _ => {}
            }
        }
    });
}

/// Loads the local ONNX embedder off the runtime and *attaches it when ready*,
/// then backfills missing embeddings. The daemon serves immediately (memory
/// runs on FTS + graph until the model binds); a load failure degrades to FTS
/// + graph rather than failing. Caller gates this on `memory.embeddings`.
pub fn attach_embedder(graph: Arc<GraphMemory>) {
    tokio::spawn(async move {
        match tokio::task::spawn_blocking(regent_embed::FastEmbedProvider::new).await {
            Ok(Ok(provider)) => {
                graph.attach_embedder(Arc::new(provider));
                tracing::info!("embedding model attached; semantic lane active");
                let graph_bf = Arc::clone(&graph);
                let backfilled =
                    tokio::task::spawn_blocking(move || graph_bf.backfill_embeddings(1000)).await;
                match backfilled {
                    Ok(Ok(n)) if n > 0 => tracing::info!(embedded = n, "memory embeddings backfilled"),
                    Ok(Err(error)) => tracing::warn!(%error, "embedding backfill failed"),
                    _ => {}
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "embedding model unavailable; memory uses FTS + graph only");
            }
            Err(error) => {
                tracing::warn!(%error, "embedder init task failed; memory uses FTS + graph only");
            }
        }
    });
}

/// Hourly graph TTL purge (the sync store call runs off the runtime).
pub fn spawn_ttl_purge(graph: Arc<GraphMemory>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(HOURLY)).await;
            let graph = Arc::clone(&graph);
            match tokio::task::spawn_blocking(move || graph.purge_expired()).await {
                Ok(Ok(count)) if count > 0 => tracing::info!(count, "TTL purge"),
                Ok(Err(error)) => tracing::warn!(%error, "TTL purge failed"),
                _ => {}
            }
        }
    });
}

/// Hourly pending-write expiry — auto-rejects staged memory writes whose
/// approval TTL elapsed, so a missed decision never commits.
pub fn spawn_pending_expiry(sessions: Arc<SessionManager>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(HOURLY)).await;
            match sessions.expire_memory_writes() {
                Ok(n) if n > 0 => tracing::info!(rejected = n, "stale pending memory writes expired"),
                Err(error) => tracing::warn!(%error, "pending-write expiry failed"),
                _ => {}
            }
        }
    });
}
