//! Discord Gateway (WebSocket) adapter — real chat, not slash-command
//! interactions. A background task holds the gateway connection (HELLO →
//! IDENTIFY → heartbeat loop) and pushes each `MESSAGE_CREATE` onto a channel
//! that `next_event` drains; replies post to the REST API with `Bot` auth.
//! Reconnects on any disconnect. The protocol payloads/parse are pure (unit-
//! tested); the live connection needs a real bot token to validate end to end.
//!
//! `MESSAGE_CONTENT` is a privileged intent — enable it for the bot in the
//! Discord developer portal or `content` arrives empty.

use crate::domain::contracts::PlatformAdapter;
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use crate::infra::platforms::reaction_names::percent_encode_path;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex as SyncMutex;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;

const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
/// GUILD_MESSAGES | DIRECT_MESSAGES | MESSAGE_CONTENT.
const INTENTS: u64 = (1 << 9) | (1 << 12) | (1 << 15);
const DEFAULT_HEARTBEAT_MS: u64 = 41_250;

pub struct DiscordGateway {
    token: String,
    client: reqwest::Client,
    /// The message id rides beside the event rather than inside it: adding it
    /// to `MessageEvent` would touch every construction site across the whole
    /// gateway to serve the adapters with a reaction API.
    rx: Mutex<mpsc::UnboundedReceiver<(MessageEvent, String)>>,
    /// Newest inbound message id per channel, so `react` with no explicit id
    /// can target "the message that started this turn".
    // ponytail: last-seen only, same ceiling as the Telegram adapter's — an
    // ARBITRARY older message needs the id plumbed through the event.
    last_message: SyncMutex<HashMap<String, String>>,
}

impl DiscordGateway {
    /// Spawns the gateway connection loop. Must be called within a Tokio
    /// runtime (the gateway binary is `#[tokio::main]`).
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run_gateway(token.clone(), tx));
        Self {
            token,
            client: reqwest::Client::new(),
            rx: Mutex::new(rx),
            last_message: SyncMutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PlatformAdapter for DiscordGateway {
    fn platform(&self) -> &str {
        "discord"
    }

    async fn next_event(&self) -> Result<MessageEvent, GatewayError> {
        let (event, message_id) = self
            .rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| GatewayError::Transport("discord gateway closed".to_owned()))?;
        // Remembered here rather than in the gateway task so the map is only
        // ever touched from the adapter that reads it.
        self.last_message
            .lock()
            .expect("last_message poisoned")
            .insert(event.chat_id.clone(), message_id);
        Ok(event)
    }

    /// `PUT /channels/{id}/messages/{id}/reactions/{emoji}/@me` — the REST
    /// call, so unlike Discord's message components this needs no interactions
    /// webhook. The emoji goes into a URL PATH, so it is percent-encoded; the
    /// tool has already refused anything that is not a single emoji.
    async fn react(
        &self,
        chat_id: &str,
        message_id: Option<&str>,
        emoji: &str,
    ) -> Result<(), GatewayError> {
        let target = match message_id {
            Some(id) => id.to_owned(),
            None => self
                .last_message
                .lock()
                .expect("last_message poisoned")
                .get(chat_id)
                .cloned()
                .ok_or_else(|| {
                    GatewayError::Transport(
                        "no message to react to yet in this channel — send one first".to_owned(),
                    )
                })?,
        };
        let url = format!(
            "https://discord.com/api/v10/channels/{chat_id}/messages/{target}/reactions/{}/@me",
            percent_encode_path(emoji)
        );
        let response = self
            .client
            .put(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bot {}", self.token),
            )
            .header(reqwest::header::CONTENT_LENGTH, 0)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        if response.status().is_success() {
            return Ok(());
        }
        Err(GatewayError::Transport(format!(
            "discord reaction failed ({})",
            response.status()
        )))
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), GatewayError> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages",
            message.chat_id
        );
        self.client
            .post(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bot {}", self.token),
            )
            .json(&json!({"content": message.text}))
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        Ok(())
    }

    async fn send_typing(&self, chat_id: &str) -> Result<(), GatewayError> {
        // POST /channels/{id}/typing shows "typing…" for ~10s. Best-effort.
        let url = format!("https://discord.com/api/v10/channels/{chat_id}/typing");
        let _ = self
            .client
            .post(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bot {}", self.token),
            )
            .send()
            .await;
        Ok(())
    }

    async fn send_file(
        &self,
        chat_id: &str,
        path: &Path,
        caption: &str,
    ) -> Result<(), GatewayError> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| GatewayError::Transport(format!("read {}: {e}", path.display())))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_owned();
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
        let mut form = reqwest::multipart::Form::new().part("files[0]", part);
        if !caption.is_empty() {
            form = form.text("payload_json", json!({ "content": caption }).to_string());
        }
        let url = format!("https://discord.com/api/v10/channels/{chat_id}/messages");
        self.client
            .post(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bot {}", self.token),
            )
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        Ok(())
    }
}

/// Reconnect loop — re-identifies on every drop (no resume in v1). Stops when
/// the receiver is gone.
async fn run_gateway(token: String, tx: mpsc::UnboundedSender<(MessageEvent, String)>) {
    while !tx.is_closed() {
        if let Err(error) = connect_once(&token, &tx).await {
            tracing::warn!(%error, "discord gateway error; reconnecting in 5s");
        }
        if tx.is_closed() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn connect_once(
    token: &str,
    tx: &mpsc::UnboundedSender<(MessageEvent, String)>,
) -> Result<(), GatewayError> {
    let (stream, _) = tokio_tungstenite::connect_async(GATEWAY_URL)
        .await
        .map_err(|e| GatewayError::Transport(e.to_string()))?;
    let (mut write, mut read) = stream.split();

    // HELLO (op 10) carries the heartbeat interval.
    let interval_ms = match read.next().await {
        Some(Ok(Message::Text(t))) => serde_json::from_str::<Value>(t.as_str())
            .ok()
            .and_then(|v| v.pointer("/d/heartbeat_interval").and_then(Value::as_u64))
            .unwrap_or(DEFAULT_HEARTBEAT_MS),
        _ => DEFAULT_HEARTBEAT_MS,
    };
    write
        .send(Message::Text(identify_payload(token).to_string().into()))
        .await
        .map_err(|e| GatewayError::Transport(e.to_string()))?;

    let mut heartbeat = tokio::time::interval(Duration::from_millis(interval_ms));
    let mut seq: Option<u64> = None;
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                write
                    .send(Message::Text(heartbeat_payload(seq).to_string().into()))
                    .await
                    .map_err(|e| GatewayError::Transport(e.to_string()))?;
            }
            incoming = read.next() => {
                let Some(message) = incoming else { return Ok(()) };
                match message.map_err(|e| GatewayError::Transport(e.to_string()))? {
                    Message::Text(t) => {
                        let Ok(value) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                        if let Some(s) = value.get("s").and_then(Value::as_u64) {
                            seq = Some(s);
                        }
                        match value.get("op").and_then(Value::as_u64) {
                            Some(0) => {
                                if let Some(event) = parse_message_create(&value) {
                                    let _ = tx.send(event);
                                }
                            }
                            // reconnect / invalid session → drop and re-identify.
                            Some(7 | 9) => return Ok(()),
                            _ => {}
                        }
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

fn identify_payload(token: &str) -> Value {
    json!({
        "op": 2,
        "d": {
            "token": token,
            "intents": INTENTS,
            "properties": {"os": "linux", "browser": "regent", "device": "regent"},
        }
    })
}

fn heartbeat_payload(seq: Option<u64>) -> Value {
    json!({"op": 1, "d": seq})
}

/// A `MESSAGE_CREATE` dispatch → a normalized event plus Discord's own message
/// id, or `None` for non-message dispatches, bot authors, and empty content (so
/// the agent never echoes itself). The id is what a later "react to that" names.
fn parse_message_create(event: &Value) -> Option<(MessageEvent, String)> {
    if event.get("t").and_then(Value::as_str) != Some("MESSAGE_CREATE") {
        return None;
    }
    let data = event.get("d")?;
    if data.pointer("/author/bot").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let content = data.get("content").and_then(Value::as_str)?;
    if content.is_empty() {
        return None;
    }
    let channel = data.get("channel_id").and_then(Value::as_str)?;
    let message_id = data.get("id").and_then(Value::as_str)?;
    let user = data
        .pointer("/author/id")
        .and_then(Value::as_str)
        .unwrap_or(channel);
    Some((
        MessageEvent {
            platform: "discord".to_owned(),
            chat_id: channel.to_owned(),
            user_id: user.to_owned(),
            text: content.to_owned(),
        },
        message_id.to_owned(),
    ))
}

#[cfg(test)]
#[path = "discord_tests.rs"]
mod tests;
