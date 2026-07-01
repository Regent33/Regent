//! Optional HTTP listener (P5): a REST ingress so platform webhooks/clients can
//! drive a turn without the stdio JSON-RPC transport. `/health` is open (for
//! load balancers); `/v1/chat` requires a bearer token — deny-by-default, so the
//! composition root refuses to start the listener without one. The reply is
//! returned synchronously (a turn yields its reply directly).

use crate::domain::errors::DeaconError;
use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Runs one turn for the HTTP layer. Injected so the router is testable without
/// the full session manager.
#[async_trait]
pub trait ChatService: Send + Sync {
    /// Runs `message` in `session` (or a fresh session when `None`); returns the
    /// session id used and the assistant reply.
    async fn chat(
        &self,
        session: Option<String>,
        message: String,
    ) -> Result<ChatReply, DeaconError>;

    /// Runs `message` in the session bound to `conversation_key` (platform
    /// continuity — one session per chat across messages). Default ignores the
    /// key and starts fresh; the session-manager-backed impl overrides it.
    async fn chat_keyed(
        &self,
        conversation_key: &str,
        message: String,
    ) -> Result<ChatReply, DeaconError> {
        let _ = conversation_key;
        self.chat(None, message).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatReply {
    pub session: String,
    pub reply: String,
}

#[derive(Clone)]
struct AppState {
    service: Arc<dyn ChatService>,
    token: Arc<String>,
}

#[derive(Deserialize)]
struct ChatBody {
    #[serde(default)]
    session: Option<String>,
    message: String,
}

/// Builds the router. `token` is required on `/v1/chat` as
/// `Authorization: Bearer <token>` and must be non-empty (enforced by the caller).
pub fn router(service: Arc<dyn ChatService>, token: String) -> Router {
    let state = AppState {
        service,
        token: Arc::new(token),
    };
    Router::new()
        .route("/health", get(health))
        .route("/v1/chat", post(chat))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ChatBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<ChatReply>, (StatusCode, String)> {
    if !authorized(&state, &headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "missing or invalid bearer token".into(),
        ));
    }
    let Json(body) = body.map_err(|e| (StatusCode::BAD_REQUEST, e.body_text()))?;
    if body.message.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "message is required".into()));
    }
    state
        .service
        .chat(body.session, body.message)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|presented| constant_time_eq(presented.as_bytes(), state.token.as_bytes()))
}

/// Length-checked, branch-free byte compare — avoids leaking the token via
/// early-exit timing on the matching prefix.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt; // oneshot

    struct EchoService;
    #[async_trait]
    impl ChatService for EchoService {
        async fn chat(
            &self,
            session: Option<String>,
            message: String,
        ) -> Result<ChatReply, DeaconError> {
            Ok(ChatReply {
                session: session.unwrap_or_else(|| "new".into()),
                reply: message,
            })
        }
    }

    fn app() -> Router {
        router(Arc::new(EchoService), "secret".to_owned())
    }

    async fn status_of(req: Request<Body>) -> StatusCode {
        app().oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn health_is_open() {
        let req = Request::get("/health").body(Body::empty()).unwrap();
        assert_eq!(status_of(req).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn chat_requires_a_valid_bearer_token() {
        let body = || Body::from(r#"{"message":"hi"}"#);
        let mk = |auth: Option<&str>| {
            let mut b = Request::post("/v1/chat").header("content-type", "application/json");
            if let Some(a) = auth {
                b = b.header(AUTHORIZATION, a);
            }
            b.body(body()).unwrap()
        };
        assert_eq!(status_of(mk(None)).await, StatusCode::UNAUTHORIZED);
        assert_eq!(
            status_of(mk(Some("Bearer wrong"))).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(status_of(mk(Some("Bearer secret"))).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn chat_runs_the_turn_and_returns_the_reply() {
        let req = Request::post("/v1/chat")
            .header("content-type", "application/json")
            .header(AUTHORIZATION, "Bearer secret")
            .body(Body::from(r#"{"session":"s1","message":"ping"}"#))
            .unwrap();
        let resp = app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let reply: ChatReply = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reply.session, "s1");
        assert_eq!(reply.reply, "ping");
    }

    #[tokio::test]
    async fn empty_message_is_rejected() {
        let req = Request::post("/v1/chat")
            .header("content-type", "application/json")
            .header(AUTHORIZATION, "Bearer secret")
            .body(Body::from(r#"{"message":"   "}"#))
            .unwrap();
        assert_eq!(status_of(req).await, StatusCode::BAD_REQUEST);
    }
}
