//! Mattermost outgoing-webhook adapter. Mattermost POSTs a JSON payload
//! carrying a shared `token`; we constant-time compare it to the configured
//! verify token (Mattermost doesn't HMAC-sign). Replies post back to the
//! channel via the REST API with a bot token. Parse/verify/build are pure.
//!
//! Configure the outgoing webhook with content type `application/json`.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use serde_json::{Value, json};

pub struct MattermostAdapter {
    /// Instance base URL, e.g. `https://mm.example.com`.
    base_url: String,
    verify_token: String,
    bot_token: String,
}

impl MattermostAdapter {
    #[must_use]
    pub fn new(
        base_url: impl Into<String>,
        verify_token: impl Into<String>,
        bot_token: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            verify_token: verify_token.into(),
            bot_token: bot_token.into(),
        }
    }
}

/// Length-checked, branch-free byte compare — no early-exit timing leak.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl WebhookAdapter for MattermostAdapter {
    fn platform(&self) -> &str {
        "mattermost"
    }

    fn verify(&self, body: &[u8], _signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        // The shared token rides in the JSON body, not a header.
        let Ok(value) = serde_json::from_slice::<Value>(body) else {
            return false;
        };
        let Some(token) = value.get("token").and_then(Value::as_str) else {
            return false;
        };
        ct_eq(token.as_bytes(), self.verify_token.as_bytes())
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        let (Some(channel), Some(text)) = (
            value.get("channel_id").and_then(Value::as_str),
            value.get("text").and_then(Value::as_str),
        ) else {
            return Ok(Vec::new());
        };
        let user = value.get("user_id").and_then(Value::as_str).unwrap_or(channel);
        Ok(vec![MessageEvent {
            platform: "mattermost".to_owned(),
            chat_id: channel.to_owned(),
            user_id: user.to_owned(),
            text: text.to_owned(),
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: format!("{}/api/v4/posts", self.base_url.trim_end_matches('/')),
            auth: SendAuth::Bearer(self.bot_token.clone()),
            body: SendBody::Json(json!({"channel_id": message.chat_id, "message": message.text})),
        }
    }

    /// Mattermost carries the shared token in the body, not a header.
    fn signature_header(&self) -> Option<&str> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_the_body_token_and_rejects_mismatches() {
        let adapter = MattermostAdapter::new("https://mm.example.com", "tok-123", "bot");
        assert!(adapter.verify(br#"{"token":"tok-123"}"#, None, None));
        assert!(!adapter.verify(br#"{"token":"wrong"}"#, None, None));
        assert!(!adapter.verify(br#"{"no":"token"}"#, None, None));
        assert!(!adapter.verify(b"not json", None, None));
    }

    #[test]
    fn parses_channel_and_text() {
        let adapter = MattermostAdapter::new("https://mm.example.com", "t", "b");
        let body = br#"{"token":"t","channel_id":"chan9","user_id":"usr1","text":"hello"}"#;
        let events = adapter.parse_webhook(body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].chat_id, "chan9");
        assert_eq!(events[0].user_id, "usr1");
        assert_eq!(events[0].text, "hello");
    }

    #[test]
    fn send_request_posts_to_the_rest_api() {
        let adapter = MattermostAdapter::new("https://mm.example.com/", "t", "BOT_TOKEN");
        let req = adapter.send_request(&OutboundMessage { chat_id: "chan9".into(), text: "hi".into() });
        assert_eq!(req.url, "https://mm.example.com/api/v4/posts");
        assert_eq!(req.auth, SendAuth::Bearer("BOT_TOKEN".into()));
        let SendBody::Json(body) = &req.body else { panic!("expected json body") };
        assert_eq!(body["channel_id"], "chan9");
        assert_eq!(body["message"], "hi");
    }
}
