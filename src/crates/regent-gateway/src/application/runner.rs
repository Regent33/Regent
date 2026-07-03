//! The gateway runner: auth first, then command dispatch (bypass commands
//! reach here even while an agent is busy), then conversation routing.
//! One running turn per session; `/stop` cancels it; busy sessions answer
//! immediately instead of silently queueing.

use crate::application::approval::ApprovalRouter;
use crate::domain::auth::AuthPolicy;
use crate::domain::contracts::{ConversationHandler, PlatformAdapter};
use crate::domain::rate::RateLimiter;
use crate::domain::entities::{
    MessageEvent, OutboundMessage, build_session_key, render_help, resolve_command,
};
use crate::domain::errors::GatewayError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct GatewayRunner {
    adapter: Arc<dyn PlatformAdapter>,
    handler: Arc<dyn ConversationHandler>,
    auth: Arc<AuthPolicy>,
    rate: Arc<RateLimiter>,
    approvals: Arc<ApprovalRouter>,
    running: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl GatewayRunner {
    #[must_use]
    pub fn new(
        adapter: Arc<dyn PlatformAdapter>,
        handler: Arc<dyn ConversationHandler>,
        auth: Arc<AuthPolicy>,
        rate: Arc<RateLimiter>,
        approvals: Arc<ApprovalRouter>,
    ) -> Arc<Self> {
        Arc::new(Self {
            adapter,
            handler,
            auth,
            rate,
            approvals,
            running: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Main loop: pull events, dispatch each. Adapter errors back off
    /// briefly instead of busy-spinning.
    pub async fn run(self: &Arc<Self>) -> Result<(), GatewayError> {
        loop {
            match self.adapter.next_event().await {
                Ok(event) => self.dispatch(event).await,
                Err(error) => {
                    tracing::warn!(%error, "adapter event failure; backing off");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    pub async fn dispatch(self: &Arc<Self>, event: MessageEvent) {
        // 1. Authorization before anything else (default deny). Unknown
        //    users get exactly one capability: redeeming a pairing code.
        if !self.auth.is_authorized(&event.user_key()) {
            let reply = if self.auth.try_redeem_code(&event.text, &event.user_key()) {
                "✅ Paired! You can talk to the agent now."
            } else {
                "Not authorized. Ask an operator for a pairing code and send it here."
            };
            self.reply(&event, reply).await;
            return;
        }

        // 1b. Rate limit per user (W2.4) — a flood brake after authz, before any
        //     work. Over-limit → told to slow down; no turn runs.
        if !self.rate.check(&event.user_key()) {
            self.reply(
                &event,
                "⏳ You're sending messages too fast — give me a moment.",
            )
            .await;
            return;
        }

        // 2. Slash commands resolve through the shared registry.
        if let Some((command, _args)) = resolve_command(&event.text) {
            self.dispatch_command(command.name, &event).await;
            return;
        }

        // 3. Conversation routing — one running turn per session.
        let session_key = build_session_key(&event.platform, &event.chat_id);
        let cancel = CancellationToken::new();
        let already_running = {
            let mut running = self.running.lock().expect("running mutex poisoned");
            if running.contains_key(&session_key) {
                true
            } else {
                running.insert(session_key.clone(), cancel.clone());
                false
            }
        };
        if already_running {
            self.reply(
                &event,
                "⏳ Still working on the previous message — /stop to interrupt.",
            )
            .await;
            return;
        }

        let this = Arc::clone(self);
        tokio::spawn(async move {
            // "Thinking" cue: refresh the platform typing indicator while the
            // turn runs, so the user sees the agent working the whole time.
            let typing = CancellationToken::new();
            spawn_typing_indicator(
                Arc::clone(&this.adapter),
                event.chat_id.clone(),
                typing.clone(),
            );

            let outcome = this.handler.handle(&session_key, &event.text, cancel).await;
            typing.cancel();
            this.running
                .lock()
                .expect("running mutex poisoned")
                .remove(&session_key);
            let text = match outcome {
                Ok(reply) => reply,
                Err(error) => format!("⚠ turn failed: {error}"),
            };
            this.reply(&event, &text).await;
        });
    }

    async fn dispatch_command(self: &Arc<Self>, name: &str, event: &MessageEvent) {
        let session_key = build_session_key(&event.platform, &event.chat_id);
        match name {
            "help" => self.reply(event, &render_help()).await,
            "pair" => {
                let code = self.auth.create_pairing_code();
                self.reply(event, &format!("Pairing code (one-time): {code}"))
                    .await;
            }
            "stop" => {
                let cancelled = self
                    .running
                    .lock()
                    .expect("running mutex poisoned")
                    .get(&session_key)
                    .map(|token| token.cancel())
                    .is_some();
                let reply = if cancelled {
                    "🛑 Stopping."
                } else {
                    "Nothing is running."
                };
                self.reply(event, reply).await;
            }
            "approve" | "deny" => {
                let approved = name == "approve";
                let resolved = self.approvals.resolve(&event.chat_key(), approved);
                let reply = match (resolved, approved) {
                    (true, true) => "Approved — continuing.",
                    (true, false) => "Denied.",
                    (false, _) => "No approval is pending.",
                };
                self.reply(event, reply).await;
            }
            "new" => {
                if let Some(token) = self
                    .running
                    .lock()
                    .expect("running mutex poisoned")
                    .get(&session_key)
                {
                    token.cancel();
                }
                self.handler.reset(&session_key).await;
                self.reply(event, "🆕 Fresh session started.").await;
            }
            other => {
                tracing::warn!(command = other, "registry command without a runner arm");
                self.reply(event, "Command not available here.").await;
            }
        }
    }

    async fn reply(&self, event: &MessageEvent, text: &str) {
        // Chat platforms show raw markdown literally; flatten it to plain text
        // here (the CLI renders markdown itself and never takes this path).
        let message = OutboundMessage {
            chat_id: event.chat_id.clone(),
            text: crate::application::format::flatten_markdown(text),
        };
        if let Err(error) = self.adapter.send(message).await {
            tracing::warn!(%error, chat = event.chat_id, "outbound send failed");
        }
    }
}

/// Fire the platform "typing"/working indicator immediately, then refresh it
/// every 4s until `cancel` fires (Telegram's typing expires after ~5s). Purely
/// best-effort — failures are ignored so the indicator never affects the turn.
fn spawn_typing_indicator(
    adapter: Arc<dyn PlatformAdapter>,
    chat_id: String,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            let _ = adapter.send_typing(&chat_id).await;
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(4)) => {}
                _ = cancel.cancelled() => break,
            }
        }
    });
}
