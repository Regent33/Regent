//! The HTTP surface — hardened relative to the Python server it replaces:
//! - binds loopback only, and every request's Host header must be local
//!   (DNS-rebinding guard).
//! - NO wildcard CORS: only the regent-web call UI's local origin (:3000,
//!   plus `REGENT_CALL_UI_ORIGIN`) is allowed, reflected per-request.
//! - `/call/turn` reaches the FULL agent (auto-approved tools), so it demands
//!   the per-boot token — embedded in the served /call page, and fetchable at
//!   `/call/token` only by the allowed origins. A drive-by webpage can
//!   neither read the token (no CORS grant) nor forge the request.
//! - UI assets are compiled in (no filesystem serving → no path traversal).
//! - Bodies are capped (25 MB audio, 8k-char TTS input).
//!
//! Routes live in `pages` (UI/health), `audio` (OpenAI-compatible ASR/TTS),
//! and `call` (the token-gated agent turn); the Host/CORS gate in `security`.

mod audio;
mod call;
mod pages;
mod security;
#[cfg(test)]
mod tests;

use crate::infra::deacon::DeaconRpc;
use crate::infra::engines::Engines;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;

pub struct AppState {
    /// Engines load in the background at boot (model files are big) and are
    /// swapped in here once ready — requests before that get the note.
    pub engines: RwLock<Engines>,
    pub deacon: RwLock<Option<Arc<DeaconRpc>>>,
    /// Agent readiness ("ready" or the reason it's off) — shown in /health.
    pub agent_note: RwLock<String>,
    /// Next moment a failed/dead agent spawn may be retried (30s cooldown so
    /// a broken setup doesn't respawn-storm, but a fixed one recovers).
    pub agent_retry_at: RwLock<Option<std::time::Instant>>,
    /// Per-boot secret embedded into the served call page; required on
    /// /call/turn. Never logged.
    pub token: String,
}

/// The live agent, (re)spawning when absent or dead. Called per turn — the
/// Python server retried per turn too; a once-at-boot failure must not mean
/// echo-forever (the reported bug).
pub async fn ensure_agent(state: &Arc<AppState>) -> Option<Arc<DeaconRpc>> {
    if let Some(rpc) = state.deacon.read().await.as_ref()
        && !rpc.is_dead()
    {
        return Some(Arc::clone(rpc));
    }
    let mut slot = state.deacon.write().await; // serializes concurrent spawns
    if let Some(rpc) = slot.as_ref()
        && !rpc.is_dead()
    {
        return Some(Arc::clone(rpc));
    }
    let now = std::time::Instant::now();
    {
        let retry_at = state.agent_retry_at.read().await;
        if let Some(at) = *retry_at
            && now < at
        {
            return None; // cooling down after a failed attempt
        }
    }
    match crate::infra::spawn::spawn_agent().await {
        crate::infra::spawn::AgentStatus::Ready(rpc) => {
            println!("  ✓ agent brain ready — voice runs the full agent (tools/memory)");
            *state.agent_note.write().await = "ready".into();
            *slot = Some(Arc::clone(&rpc));
            Some(rpc)
        }
        crate::infra::spawn::AgentStatus::Unavailable(reason) => {
            println!("  ⚠ agent voice off ({reason}) — retrying on a later turn");
            *state.agent_note.write().await = reason;
            *state.agent_retry_at.write().await = Some(now + std::time::Duration::from_secs(30));
            *slot = None;
            None
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(pages::index))
        .route("/call", get(pages::call_page))
        .route("/call/token", get(pages::call_token))
        .route("/ui/style.css", get(pages::style))
        .route("/ui/fonts/kontes-compressed-bold.ttf", get(pages::font))
        .route("/health", get(pages::health))
        .route("/v1/models", get(pages::health))
        .route("/v1/audio/transcriptions", post(audio::transcriptions))
        .route("/v1/audio/speech", post(audio::speech))
        .route("/call/turn", post(call::call_turn))
        .route("/call/frame", post(call::call_frame))
        .layer(DefaultBodyLimit::max(MAX_AUDIO_BYTES))
        .layer(middleware::from_fn(security::security))
        .with_state(state)
}

fn err(status: StatusCode, msg: &str) -> Response {
    (status, axum::Json(json!({"error": msg}))).into_response()
}
