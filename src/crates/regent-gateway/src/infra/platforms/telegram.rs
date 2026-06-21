//! Telegram Bot API adapter (long polling). Wire parsing/building are pure
//! functions so the format is unit-testable without a token; the live
//! round-trip needs `getUpdates`/`sendMessage` against api.telegram.org.

use crate::domain::contracts::PlatformAdapter;
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

const LONG_POLL_SECS: u64 = 30;

pub struct TelegramAdapter {
    token: String,
    client: reqwest::Client,
    /// Highest update_id seen; getUpdates offset = this + 1.
    offset: AtomicI64,
}

impl TelegramAdapter {
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            client: reqwest::Client::new(),
            offset: AtomicI64::new(0),
        }
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{method}", self.token)
    }

    async fn call(&self, method: &str, payload: Value) -> Result<Value, GatewayError> {
        let response = self
            .client
            .post(self.url(method))
            .timeout(Duration::from_secs(LONG_POLL_SECS + 15))
            .json(&payload)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let body: Value = response
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Transport(format!(
                "telegram API error: {body}"
            )));
        }
        Ok(body)
    }
}

#[async_trait]
impl PlatformAdapter for TelegramAdapter {
    fn platform(&self) -> &str {
        "telegram"
    }

    async fn next_event(&self) -> Result<MessageEvent, GatewayError> {
        loop {
            let offset = self.offset.load(Ordering::Relaxed) + 1;
            let body = self
                .call(
                    "getUpdates",
                    json!({"offset": offset, "timeout": LONG_POLL_SECS}),
                )
                .await?;
            let events = parse_updates(&body);
            // Advance past everything we saw, message or not (e.g. edits).
            if let Some(max_id) = body
                .get("result")
                .and_then(Value::as_array)
                .and_then(|updates| {
                    updates
                        .iter()
                        .filter_map(|u| u.get("update_id")?.as_i64())
                        .max()
                })
            {
                self.offset.fetch_max(max_id, Ordering::Relaxed);
            }
            if let Some(event) = events.into_iter().next() {
                return Ok(event);
            }
            // empty long-poll → poll again
        }
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), GatewayError> {
        self.call("sendMessage", send_payload(&message)).await?;
        Ok(())
    }
}

/// Extracts text-message events from a getUpdates response body.
#[must_use]
pub fn parse_updates(body: &Value) -> Vec<MessageEvent> {
    let Some(updates) = body.get("result").and_then(Value::as_array) else {
        return Vec::new();
    };
    updates
        .iter()
        .filter_map(|update| {
            let message = update.get("message")?;
            let text = message.get("text")?.as_str()?;
            Some(MessageEvent {
                platform: "telegram".to_owned(),
                chat_id: message.pointer("/chat/id")?.as_i64()?.to_string(),
                user_id: message.pointer("/from/id")?.as_i64()?.to_string(),
                text: text.to_owned(),
            })
        })
        .collect()
}

#[must_use]
pub fn send_payload(message: &OutboundMessage) -> Value {
    json!({"chat_id": message.chat_id, "text": message.text})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_updates_and_skips_non_text() {
        let body = json!({"ok": true, "result": [
            {"update_id": 7, "message": {
                "text": "hello", "chat": {"id": -100123}, "from": {"id": 42}}},
            {"update_id": 8, "message": {"photo": [], "chat": {"id": 1}, "from": {"id": 2}}},
            {"update_id": 9, "edited_message": {"text": "x"}}
        ]});
        let events = parse_updates(&body);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].chat_id, "-100123");
        assert_eq!(events[0].user_id, "42");
        assert_eq!(events[0].text, "hello");
        assert_eq!(events[0].user_key(), "telegram:42");
    }

    #[test]
    fn send_payload_shape() {
        let payload = send_payload(&OutboundMessage {
            chat_id: "5".into(),
            text: "hi".into(),
        });
        assert_eq!(payload["chat_id"], "5");
        assert_eq!(payload["text"], "hi");
    }
}
