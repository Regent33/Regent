//! Wires the HTTP listener to the session manager and serves it. The
//! `SessionChatService` is the `ChatService` impl the router calls: a request
//! either resumes the given session or starts a fresh one, then runs the turn.

use crate::application::session_manager::SessionManager;
use crate::domain::config::HttpConfig;
use crate::domain::errors::DaemonError;
use crate::infra::http_listener::{ChatReply, ChatService, router};
use crate::infra::{discord_interactions, webhook};
use async_trait::async_trait;
use regent_kernel::SessionId;
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
    ) -> Result<ChatReply, DaemonError> {
        let sid = match session {
            Some(s) => {
                self.sessions
                    .resume_session(SessionId::from_string(&s))
                    .await?
            }
            None => self.sessions.create_session().await?,
        };
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
    ) -> Result<ChatReply, DaemonError> {
        let sid = self.sessions.ensure_keyed_session(conversation_key).await?;
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
) -> Result<(), DaemonError> {
    if cfg.token.trim().is_empty() {
        return Err(DaemonError::Config(
            "http.enabled requires a non-empty http.token (refusing to expose /v1/chat unauthenticated)".into(),
        ));
    }
    let service: Arc<dyn ChatService> = Arc::new(SessionChatService { sessions });
    let mut app = router(Arc::clone(&service), cfg.token.clone());

    // Mount platform webhooks for whatever secrets are present in the env.
    let registry = webhook::registry_from_env();
    if !registry.is_empty() {
        let platforms: Vec<_> = registry.keys().cloned().collect();
        app = app.merge(webhook::router(registry, Arc::clone(&service)));
        tracing::info!(
            ?platforms,
            "platform webhooks enabled at /webhook/{{platform}}"
        );
    }

    // Discord interactions (slash commands) — separate sync-response route.
    if let Ok(public_key) = std::env::var("DISCORD_PUBLIC_KEY")
        && !public_key.is_empty()
    {
        app = app.merge(discord_interactions::router(
            public_key,
            Arc::clone(&service),
        ));
        tracing::info!("discord interactions enabled at /discord/interactions");
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
