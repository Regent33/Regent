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
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
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
    rx: Mutex<mpsc::UnboundedReceiver<MessageEvent>>,
}

impl DiscordGateway {
    /// Spawns the gateway connection loop. Must be called within a Tokio
    /// runtime (the gateway binary is `#[tokio::main]`).
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run_gateway(token.clone(), tx));
        Self { token, client: reqwest::Client::new(), rx: Mutex::new(rx) }
    }
}

#[async_trait]
impl PlatformAdapter for DiscordGateway {
    fn platform(&self) -> &str {
        "discord"
    }

    async fn next_event(&self) -> Result<MessageEvent, GatewayError> {
        self.rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| GatewayError::Transport("discord gateway closed".to_owned()))
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), GatewayError> {
        let url = format!("https://discord.com/api/v10/channels/{}/messages", message.chat_id);
        self.client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, format!("Bot {}", self.token))
            .json(&json!({"content": message.text}))
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        Ok(())
    }
}

/// Reconnect loop — re-identifies on every drop (no resume in v1). Stops when
/// the receiver is gone.
async fn run_gateway(token: String, tx: mpsc::UnboundedSender<MessageEvent>) {
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
    tx: &mpsc::UnboundedSender<MessageEvent>,
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

/// A `MESSAGE_CREATE` dispatch → a normalized event, or `None` for non-message
/// dispatches, bot authors, and empty content (so the agent never echoes
/// itself).
fn parse_message_create(event: &Value) -> Option<MessageEvent> {
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
    let user = data.pointer("/author/id").and_then(Value::as_str).unwrap_or(channel);
    Some(MessageEvent {
        platform: "discord".to_owned(),
        chat_id: channel.to_owned(),
        user_id: user.to_owned(),
        text: content.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify_carries_token_and_message_content_intent() {
        let p = identify_payload("bot-tok");
        assert_eq!(p["op"], 2);
        assert_eq!(p["d"]["token"], "bot-tok");
        // MESSAGE_CONTENT (1<<15) must be set or content is empty.
        assert_eq!(p["d"]["intents"].as_u64().unwrap() & (1 << 15), 1 << 15);
    }

    #[test]
    fn heartbeat_uses_null_then_the_last_sequence() {
        assert!(heartbeat_payload(None)["d"].is_null());
        assert_eq!(heartbeat_payload(Some(7))["d"], 7);
    }

    #[test]
    fn parses_a_user_message_and_skips_bots_and_non_messages() {
        let msg = json!({"op":0,"t":"MESSAGE_CREATE","d":{
            "channel_id":"C1","content":"hi there","author":{"id":"U1","bot":false}}});
        let event = parse_message_create(&msg).unwrap();
        assert_eq!(event.chat_id, "C1");
        assert_eq!(event.user_id, "U1");
        assert_eq!(event.text, "hi there");

        let bot = json!({"op":0,"t":"MESSAGE_CREATE","d":{
            "channel_id":"C1","content":"x","author":{"id":"B1","bot":true}}});
        assert!(parse_message_create(&bot).is_none(), "bot messages are skipped");

        let typing = json!({"op":0,"t":"TYPING_START","d":{}});
        assert!(parse_message_create(&typing).is_none());

        let empty = json!({"op":0,"t":"MESSAGE_CREATE","d":{
            "channel_id":"C1","content":"","author":{"id":"U1"}}});
        assert!(parse_message_create(&empty).is_none(), "empty content skipped");
    }
}
