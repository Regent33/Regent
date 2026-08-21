//! Token-gated call endpoints: the streamed agent turn + camera frames.

use super::{AppState, ensure_agent, err};
use crate::application::turn::{TurnDeps, run_text_turn, run_turn};
use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;

/// A questionnaire answer is a handful of short ids — anything larger is not
/// one, and this endpoint resumes a turn, so it is bounded like every other.
const MAX_ANSWER_BYTES: usize = 64 * 1024;
const MAX_FRAME_BYTES: usize = 5 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 8_000;

fn valid_token(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get("x-call-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|presented| presented == state.token)
}

async fn synced_agent(
    state: &Arc<AppState>,
) -> Result<Option<Arc<crate::infra::deacon::DeaconRpc>>, Response> {
    let Some(agent) = ensure_agent(state).await else {
        return Ok(None);
    };
    if let Some(model) = crate::infra::spawn::call_model_from(&crate::infra::spawn::regent_home())
        && let Err(error) = agent.sync_model(&model).await
    {
        return Err(err(StatusCode::BAD_GATEWAY, &error));
    }
    Ok(Some(agent))
}

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
    if !valid_token(&state, &headers) {
        return err(StatusCode::UNAUTHORIZED, "missing or wrong call token");
    }
    let deacon = match synced_agent(&state).await {
        Ok(agent) => agent,
        Err(response) => return response,
    };
    let deps = TurnDeps {
        engines: state.engines.read().await.clone(),
        deacon,
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

/// Typed Butler turn. It skips ASR only; agent session, tools, diagrams, TTS,
/// streaming, cancellation, and token security are identical to microphone
/// turns.
pub(super) async fn call_text(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !valid_token(&state, &headers) {
        return err(StatusCode::UNAUTHORIZED, "missing or wrong call token");
    }
    let Ok(text) = std::str::from_utf8(&body) else {
        return err(StatusCode::BAD_REQUEST, "typed message is not UTF-8");
    };
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT_CHARS {
        return err(
            StatusCode::BAD_REQUEST,
            "typed message must contain 1 to 8000 characters",
        );
    }
    let deacon = match synced_agent(&state).await {
        Ok(agent) => agent,
        Err(response) => return response,
    };
    let deps = TurnDeps {
        engines: state.engines.read().await.clone(),
        deacon,
        agent_note: state.agent_note.read().await.clone(),
    };
    let (tx, rx) = mpsc::channel::<String>(64);
    tokio::spawn(run_text_turn(deps, text.to_owned(), tx));
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

/// Begin one Butler-modal conversation. The voice server is process-scoped,
/// but session history is call-scoped so each opening gets its own rail row.
pub(super) async fn call_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if !valid_token(&state, &headers) {
        return err(StatusCode::UNAUTHORIZED, "missing or wrong call token");
    }
    let agent = match synced_agent(&state).await {
        Ok(Some(agent)) => agent,
        Ok(None) => {
            return err(
                StatusCode::SERVICE_UNAVAILABLE,
                "Butler agent is unavailable",
            );
        }
        Err(response) => return response,
    };
    match agent.begin_session().await {
        Some(session_id) => Json(json!({"session_id": session_id})).into_response(),
        None => err(
            StatusCode::BAD_GATEWAY,
            "Butler could not create a conversation session",
        ),
    }
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
    if !valid_token(&state, &headers) {
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

/// `POST /call/answer` — the caller tapped an option on a question card, so
/// hand the typed answer to the paused turn. Token-gated exactly like
/// `/call/turn`: this resumes an agent turn, so a drive-by page must not reach
/// it. `{"resolved": false}` means nothing was waiting (the question already
/// timed out, or a second tap raced the first), which the client shows rather
/// than silently swallowing.
pub(super) async fn call_answer(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !valid_token(&state, &headers) {
        return err(StatusCode::UNAUTHORIZED, "missing or wrong call token");
    }
    if body.len() > MAX_ANSWER_BYTES {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "answer too large");
    }
    let Ok(answer) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return err(StatusCode::BAD_REQUEST, "answer is not JSON");
    };
    let Some(rpc) = state.deacon.read().await.clone() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "no agent connected");
    };
    Json(json!({"resolved": rpc.answer_question(answer).await})).into_response()
}
