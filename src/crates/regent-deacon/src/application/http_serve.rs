//! Wires the HTTP listener to the session manager and serves it. The
//! `SessionChatService` is the `ChatService` impl the router calls: a request
//! either resumes the given session or starts a fresh one, then runs the turn.

use crate::application::session_manager::SessionManager;
use crate::domain::config::HttpConfig;
use crate::domain::errors::DeaconError;
use crate::infra::http_listener::{ChatReply, ChatService, router};
use crate::infra::{discord_interactions, webhook};
use async_trait::async_trait;
use regent_gateway::{AuthPolicy, RateLimiter};
use regent_kernel::SessionId;
use std::path::PathBuf;
use std::sync::Arc;

struct SessionChatService {
    sessions: Arc<SessionManager>,
}

#[async_trait]
impl ChatService for SessionChatService {
    async fn chat(
        &self,
        session: Option<String>,
        message: String,
    ) -> Result<ChatReply, DeaconError> {
        let sid = match session {
            Some(s) => {
                self.sessions
                    .resume_session(SessionId::from_string(&s))
                    .await?
            }
            None => self.sessions.create_session().await?,
        };
        // Background-task results ride the next real turn on ANY surface —
        // same injection the JSON-RPC prompt.submit path does.
        let message = crate::application::background_task_tool::wrap_prompt(&message);
        let reply = self.sessions.run_turn(&sid, &message).await?;
        Ok(ChatReply {
            session: sid.to_string(),
            reply,
        })
    }

    async fn chat_keyed(
        &self,
        conversation_key: &str,
        message: String,
    ) -> Result<ChatReply, DeaconError> {
        let sid = self.sessions.ensure_keyed_session(conversation_key).await?;
        let message = crate::application::background_task_tool::wrap_prompt(&message);
        let reply = self.sessions.run_turn(&sid, &message).await?;
        Ok(ChatReply {
            session: sid.to_string(),
            reply,
        })
    }
}

/// Binds the HTTP listener and serves it in the background. Deny-by-default:
/// returns a config error (and the caller skips serving) if no bearer token is
/// set, so the REST surface is never exposed unauthenticated.
pub async fn spawn_http_listener(
    sessions: Arc<SessionManager>,
    cfg: &HttpConfig,
) -> Result<(), DeaconError> {
    if cfg.token.trim().is_empty() {
        return Err(DeaconError::Config(
            "http.enabled requires a non-empty http.token (refusing to expose /v1/chat unauthenticated)".into(),
        ));
    }
    let manager = Arc::clone(&sessions);
    let service: Arc<dyn ChatService> = Arc::new(SessionChatService { sessions });
    let mut app = router(Arc::clone(&service), cfg.token.clone());

    // Per-user authorization for external ingress (webhook + Discord planes),
    // shared with the gateway via $REGENT_HOME/gateway-auth.json. Default-deny:
    // an unknown sender's only capability is redeeming a pairing code — a
    // signature-valid request no longer runs a turn on its own (W1.1/P0-001).
    let home = Arc::new(regent_home());
    let auth = Arc::new(AuthPolicy::new(regent_gateway::load_auth_snapshot(&home)));
    // Per-user inbound rate limit (W2.4) from REGENT_MESSAGES_PER_MIN; unset = off.
    let rate = Arc::new(RateLimiter::from_env());

    // Mount platform webhooks for whatever secrets are present in the env.
    let registry = webhook::registry_from_env();
    if !registry.is_empty() {
        let platforms: Vec<_> = registry.keys().cloned().collect();
        // Let keyed platform sessions deliver the agent's send_message/send_file
        // back to the platform (replies still go via the webhook handler).
        manager.set_platform_delivery(Arc::new(webhook::WebhookPlatformDelivery::from_env()));
        app = app.merge(webhook::router(
            registry,
            Arc::clone(&service),
            Arc::clone(&auth),
            Arc::clone(&home),
            Arc::clone(&rate),
        ));
        tracing::info!(
            ?platforms,
            "platform webhooks enabled at /webhook/{{platform}} (authorized)"
        );
    }

    // Discord interactions (slash commands) — separate sync-response route.
    if let Ok(public_key) = std::env::var("DISCORD_PUBLIC_KEY")
        && !public_key.is_empty()
    {
        app = app.merge(discord_interactions::router(
            public_key,
            Arc::clone(&service),
            Arc::clone(&auth),
            Arc::clone(&home),
            Arc::clone(&rate),
        ));
        tracing::info!("discord interactions enabled at /discord/interactions (authorized)");
    }

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!(bind = %cfg.bind, "http listener enabled (REST ingress)");
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!(%error, "http listener stopped");
        }
    });
    Ok(())
}

/// Resolve `$REGENT_HOME` (the dir the store/graph and `.env` live in), falling
/// back to `~/.regent`. The shared auth snapshot is persisted here.
fn regent_home() -> PathBuf {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return PathBuf::from(custom);
    }
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(base).join(".regent")
}
