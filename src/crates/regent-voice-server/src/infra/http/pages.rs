//! Compiled-in UI assets, the call token, and /health.

use super::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{Html, IntoResponse};
use serde_json::json;
use std::sync::Arc;

const INDEX_HTML: &str = include_str!("../../../../../../python-voice-server/ui/index.html");
const CALL_HTML: &str = include_str!("../../../../../../python-voice-server/ui/call.html");
const STYLE_CSS: &str = include_str!("../../../../../../python-voice-server/ui/style.css");
const FONT: &[u8] =
    include_bytes!("../../../../../../python-voice-server/ui/fonts/CHORUS-BLACK.otf");

/// The call token, for the cross-origin call UI. Only allowed origins can READ
/// this response (no CORS grant for anyone else).
pub(super) async fn call_token(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({"token": state.token}))
}

pub(super) async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

pub(super) async fn call_page(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(CALL_HTML.replace("__CALL_TOKEN__", &state.token))
}

pub(super) async fn style() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], STYLE_CSS)
}

pub(super) async fn font() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "font/otf")], FONT)
}

pub(super) async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let engines = state.engines.read().await.clone();
    Json(json!({
        "engine": "regent-voice-server (rust)",
        "asr": engines.asr.is_some(),
        "tts": engines.tts.is_some(),
        "note": engines.note,
        "agent": *state.agent_note.read().await,
        "device": "cpu",
        "warm": engines.ready(),
        "models_dir": std::env::var("REGENT_MODELS_DIR")
            .unwrap_or_else(|_| "tts-asr-local-models".into()),
    }))
}
