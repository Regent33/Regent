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

use crate::application::turn::{TurnDeps, run_turn};
use crate::infra::deacon::DeaconRpc;
use crate::infra::engines::Engines;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Multipart, Query, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

const INDEX_HTML: &str = include_str!("../../../../../python-voice-server/ui/index.html");
const CALL_HTML: &str = include_str!("../../../../../python-voice-server/ui/call.html");
const STYLE_CSS: &str = include_str!("../../../../../python-voice-server/ui/style.css");
const FONT: &[u8] =
    include_bytes!("../../../../../python-voice-server/ui/fonts/kontes-compressed-bold.ttf");

const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;
const MAX_TTS_CHARS: usize = 8_000;
const MAX_FRAME_BYTES: usize = 5 * 1024 * 1024;

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
        .route("/", get(index))
        .route("/call", get(call_page))
        .route("/call/token", get(call_token))
        .route("/ui/style.css", get(style))
        .route("/ui/fonts/kontes-compressed-bold.ttf", get(font))
        .route("/health", get(health))
        .route("/v1/models", get(health))
        .route("/v1/audio/transcriptions", post(transcriptions))
        .route("/v1/audio/speech", post(speech))
        .route("/call/turn", post(call_turn))
        .route("/call/frame", post(call_frame))
        .layer(DefaultBodyLimit::max(MAX_AUDIO_BYTES))
        .layer(middleware::from_fn(security))
        .with_state(state)
}

/// The origins allowed to call cross-origin: the regent-web call UI (Next,
/// :3000) plus an optional `REGENT_CALL_UI_ORIGIN`. Never a wildcard.
fn origin_allowed(origin: &str) -> bool {
    if matches!(origin, "http://localhost:3000" | "http://127.0.0.1:3000") {
        return true;
    }
    std::env::var("REGENT_CALL_UI_ORIGIN").is_ok_and(|o| o.trim_end_matches('/') == origin)
}

/// Host + CORS gate. Rejects non-local Hosts — a page on `evil.tld` that
/// resolves to 127.0.0.1 (DNS rebinding) still sends `Host: evil.tld`.
/// Reflects CORS headers only for [`origin_allowed`] origins and answers
/// their preflights; every other origin gets no grant, so its scripts can't
/// read anything (the browser blocks it).
async fn security(req: Request<Body>, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let name = host.rsplit_once(':').map_or(host, |(n, _)| n);
    if !matches!(name, "localhost" | "127.0.0.1" | "[::1]") {
        return (StatusCode::FORBIDDEN, "local requests only").into_response();
    }
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok())
        .filter(|o| origin_allowed(o))
        .map(ToOwned::to_owned);
    if req.method() == axum::http::Method::OPTIONS {
        let mut res = StatusCode::NO_CONTENT.into_response();
        if origin.is_some() {
            cors_headers(res.headers_mut(), origin.as_deref());
            res.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST".parse().unwrap(),
            );
            res.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                "content-type, x-call-token".parse().unwrap(),
            );
        }
        return res;
    }
    let mut res = next.run(req).await;
    cors_headers(res.headers_mut(), origin.as_deref());
    res
}

fn cors_headers(headers: &mut HeaderMap, origin: Option<&str>) {
    headers.insert(header::VARY, "Origin".parse().unwrap());
    if let Some(o) = origin
        && let Ok(value) = o.parse()
    {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
}

/// The call token, for the cross-origin call UI. Only [`origin_allowed`]
/// origins can READ this response (no CORS grant for anyone else).
async fn call_token(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({"token": state.token}))
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn call_page(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(CALL_HTML.replace("__CALL_TOKEN__", &state.token))
}

async fn style() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], STYLE_CSS)
}

async fn font() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "font/ttf")], FONT)
}

async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
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

/// OpenAI-compatible: multipart `file` → `{"text": …}`.
async fn transcriptions(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
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
async fn speech(
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

#[derive(Deserialize)]
struct TurnQuery {
    language: Option<String>,
}

/// The streamed call turn — token-gated (see module docs).
async fn call_turn(
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
async fn call_frame(
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

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({"error": msg}))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    fn app() -> (Router, Arc<AppState>) {
        let state = Arc::new(AppState {
            engines: RwLock::new(Engines::default()),
            deacon: RwLock::new(None),
            agent_note: RwLock::new("test".into()),
            // Tests never spawn a real deacon: keep the retry gate far away.
            agent_retry_at: RwLock::new(Some(
                std::time::Instant::now() + std::time::Duration::from_secs(3600),
            )),
            token: "sekrit".into(),
        });
        (router(Arc::clone(&state)), state)
    }

    fn req(method: &str, uri: &str) -> axum::http::request::Builder {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "localhost:8000")
    }

    #[tokio::test]
    async fn non_local_host_is_forbidden() {
        let (app, _) = app();
        let r = Request::builder()
            .method("GET")
            .uri("/health")
            .header(header::HOST, "evil.tld")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.oneshot(r).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn call_turn_requires_the_token() {
        let (app, _) = app();
        let bad = req("POST", "/call/turn").body(Body::empty()).unwrap();
        assert_eq!(
            app.clone().oneshot(bad).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let good = req("POST", "/call/turn")
            .header("x-call-token", "sekrit")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.oneshot(good).await.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn cors_grant_only_for_the_call_ui_origin() {
        let (app, _) = app();
        let allowed = req("GET", "/call/token")
            .header(header::ORIGIN, "http://localhost:3000")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(allowed).await.unwrap();
        assert_eq!(
            res.headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "http://localhost:3000"
        );
        let other = req("GET", "/call/token")
            .header(header::ORIGIN, "http://attacker.example")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(other).await.unwrap();
        assert!(
            res.headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none(),
            "no CORS grant for unknown origins — the browser blocks the read"
        );
    }

    #[tokio::test]
    async fn preflight_answers_for_the_allowed_origin() {
        let (app, _) = app();
        let r = req("OPTIONS", "/call/turn")
            .header(header::ORIGIN, "http://localhost:3000")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(r).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let allow = res
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .unwrap();
        assert!(allow.to_str().unwrap().contains("x-call-token"));
    }

    #[tokio::test]
    async fn served_call_page_carries_the_real_token() {
        let (app, _) = app();
        let r = req("GET", "/call").body(Body::empty()).unwrap();
        let res = app.oneshot(r).await.unwrap();
        let body = axum::body::to_bytes(res.into_body(), 1_000_000)
            .await
            .unwrap();
        let page = String::from_utf8_lossy(&body);
        assert!(page.contains("content=\"sekrit\""));
        assert!(!page.contains("__CALL_TOKEN__"));
    }
}
