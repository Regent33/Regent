//! Pure Telegram wire codec: parse a `getUpdates` body into text events and
//! build a `sendMessage` payload. No I/O — unit-tested without a token.

use crate::domain::entities::{MessageEvent, OutboundMessage};
use serde_json::{Value, json};

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

/// The `sendMessage` JSON payload for an outbound text reply.
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
