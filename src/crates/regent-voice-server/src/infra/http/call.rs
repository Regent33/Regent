//! Token-gated call endpoints: the streamed agent turn + camera frames.

use super::{AppState, ensure_agent, err};
use crate::application::turn::{TurnDeps, run_turn};
use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;

const MAX_FRAME_BYTES: usize = 5 * 1024 * 1024;

#[derive(Deserialize)]
pub(super) struct TurnQuery {
    language: Option<String>,
}

/// The streamed call turn — token-gated (see module docs).
pub(super) async fn call_turn(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TurnQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let presented = headers
        .get("x-call-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if presented != state.token {
        return err(StatusCode::UNAUTHORIZED, "missing or wrong call token");
    }
    let deps = TurnDeps {
        engines: state.engines.read().await.clone(),
        deacon: ensure_agent(&state).await,
        agent_note: state.agent_note.read().await.clone(),
    };
    let language = q.language.filter(|l| !l.is_empty());
    let (tx, rx) = mpsc::channel::<String>(64);
    tokio::spawn(run_turn(deps, body.to_vec(), language, tx));
    let stream = futures::stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|line| {
            (
                Ok::<_, std::convert::Infallible>(Bytes::from(line + "\n")),
                rx,
            )
        })
    });
    (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(stream),
    )
        .into_response()
}

/// Camera frame from the call UI (sent every couple of seconds while the
/// caller shares camera). Token-gated like /call/turn; JPEG magic checked;
/// written to `$REGENT_HOME/voice/camera-frame.jpg` where the agent's
/// `camera_capture` tool picks it up while it is fresh.
pub(super) async fn call_frame(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let presented = headers
        .get("x-call-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if presented != state.token {
        return err(StatusCode::UNAUTHORIZED, "missing or wrong call token");
    }
    if body.len() > MAX_FRAME_BYTES {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "frame too large");
    }
    if !body.starts_with(&[0xff, 0xd8, 0xff]) {
        return err(StatusCode::BAD_REQUEST, "not a JPEG frame");
    }
    let dir = crate::infra::spawn::regent_home().join("voice");
    let write = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)?;
        // Temp + rename so the camera tool never reads a torn frame.
        let tmp = dir.join("camera-frame.jpg.tmp");
        std::fs::write(&tmp, &body)?;
        std::fs::rename(&tmp, dir.join("camera-frame.jpg"))
    })
    .await;
    match write {
        Ok(Ok(())) => Json(json!({"ok": true})).into_response(),
        Ok(Err(e)) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}
