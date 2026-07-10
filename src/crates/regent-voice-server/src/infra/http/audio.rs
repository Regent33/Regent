//! OpenAI-compatible ASR/TTS endpoints.

use super::{AppState, err};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Multipart, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::sync::Arc;

const MAX_TTS_CHARS: usize = 8_000;

/// OpenAI-compatible: multipart `file` → `{"text": …}`.
pub(super) async fn transcriptions(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Response {
    let engines = state.engines.read().await.clone();
    let Some(asr) = engines.asr else {
        return err(StatusCode::SERVICE_UNAVAILABLE, &engines.note);
    };
    let mut data: Option<Bytes> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            data = field.bytes().await.ok();
            break;
        }
    }
    let Some(data) = data else {
        return err(StatusCode::BAD_REQUEST, "missing `file` field");
    };
    let out = tokio::task::spawn_blocking(move || asr.transcribe(&data, None)).await;
    match out {
        Ok(Ok(text)) => Json(json!({"text": text.trim()})).into_response(),
        Ok(Err(e)) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("ASR failed: {e}"),
        ),
        Err(e) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("ASR panicked: {e}"),
        ),
    }
}

/// OpenAI-compatible: `{input, …}` → audio bytes (WAV — opus arrives with the
/// local engines slice).
pub(super) async fn speech(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let engines = state.engines.read().await.clone();
    let Some(tts) = engines.tts else {
        return err(StatusCode::SERVICE_UNAVAILABLE, &engines.note);
    };
    let text = body
        .get("input")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    if text.is_empty() {
        return err(StatusCode::BAD_REQUEST, "empty input");
    }
    if text.chars().count() > MAX_TTS_CHARS {
        return err(StatusCode::BAD_REQUEST, "input too long");
    }
    let clean = crate::domain::speakable::strip_markdown(&text);
    let out = tokio::task::spawn_blocking(move || tts.synthesize(&clean)).await;
    match out {
        Ok(Ok(audio)) => (
            [(header::CONTENT_TYPE, "audio/wav")],
            regent_speech::wav::encode(&audio),
        )
            .into_response(),
        Ok(Err(e)) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("TTS failed: {e}"),
        ),
        Err(e) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("TTS panicked: {e}"),
        ),
    }
}
