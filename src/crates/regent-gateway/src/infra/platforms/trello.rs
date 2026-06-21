//! Trello webhook adapter. Trello confirms a new webhook with a `HEAD`/`GET` to
//! the callback URL (must return `200`), then POSTs action events as JSON signed
//! with `X-Trello-Webhook` = base64(HMAC-SHA1(api_secret, requestBody ‖
//! callbackURL)). The signature covers the **callback URL** as well as the body,
//! so verification runs through `verify_request` (which has the URL). We treat a
//! `commentCard` action as an inbound message and reply by posting a card
//! comment via the REST API. Parse/verify/build are pure — unit-testable.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter, WebhookRequest};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

pub struct TrelloAdapter {
    api_secret: String,
    api_key: String,
    token: String,
}

impl TrelloAdapter {
    #[must_use]
    pub fn new(
        api_secret: impl Into<String>,
        api_key: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            api_secret: api_secret.into(),
            api_key: api_key.into(),
            token: token.into(),
        }
    }
}

impl WebhookAdapter for TrelloAdapter {
    fn platform(&self) -> &str {
        "trello"
    }

    /// Trello's signature also covers the callback URL — the body-only path
    /// can't verify it, so deny (the route uses `verify_request`).
    fn verify(&self, _body: &[u8], _signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        false
    }

    fn verify_request(&self, request: &WebhookRequest<'_>) -> bool {
        let Some(signature) = request.signature else {
            return false;
        };
        let Ok(expected) = STANDARD.decode(signature) else {
            return false;
        };
        let Ok(mut mac) = HmacSha1::new_from_slice(self.api_secret.as_bytes()) else {
            return false;
        };
        mac.update(request.body);
        mac.update(request.url.as_bytes()); // base64(HMAC-SHA1(secret, body ‖ callbackURL))
        mac.verify_slice(&expected).is_ok() // constant-time
    }

    /// Trello verifies a new webhook with an unsigned `HEAD`/`GET` that just
    /// needs a `200` — echo an empty body.
    fn verify_get(&self, _query: &str) -> Option<String> {
        Some(String::new())
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        let action = &value["action"];
        // Only card comments are conversational; other actions are acked empty.
        if action.get("type").and_then(Value::as_str) != Some("commentCard") {
            return Ok(Vec::new());
        }
        let Some(text) = action.pointer("/data/text").and_then(Value::as_str) else {
            return Ok(Vec::new());
        };
        let card = action
            .pointer("/data/card/id")
            .and_then(Value::as_str)
            .unwrap_or("trello");
        let user = action
            .pointer("/memberCreator/id")
            .and_then(Value::as_str)
            .unwrap_or("trello");
        Ok(vec![MessageEvent {
            platform: "trello".to_owned(),
            chat_id: card.to_owned(),
            user_id: user.to_owned(),
            text: text.to_owned(),
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        // Post a comment on the card; key/token authenticate as form fields.
        SendRequest {
            url: format!(
                "https://api.trello.com/1/cards/{}/actions/comments",
                message.chat_id
            ),
            auth: SendAuth::None,
            body: SendBody::Form(vec![
                ("key".to_owned(), self.api_key.clone()),
                ("token".to_owned(), self.token.clone()),
                ("text".to_owned(), message.text.clone()),
            ]),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("x-trello-webhook")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, body: &[u8], url: &str) -> String {
        let mut mac = HmacSha1::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        mac.update(url.as_bytes());
        STANDARD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verify_request_accepts_valid_signature_and_rejects_tampering() {
        let adapter = TrelloAdapter::new("app-secret", "k", "t");
        let url = "https://example.com/webhook/trello";
        let body = br#"{"action":{"type":"commentCard"}}"#;
        let sig = sign("app-secret", body, url);

        let ok = WebhookRequest {
            url,
            body,
            signature: Some(&sig),
            timestamp: None,
            nonce: None,
        };
        assert!(adapter.verify_request(&ok));

        // Tampered callback URL, body, wrong key, and missing signature all fail.
        let bad_url = WebhookRequest {
            url: "https://evil.test/x",
            body,
            signature: Some(&sig),
            timestamp: None,
            nonce: None,
        };
        assert!(!adapter.verify_request(&bad_url));
        let bad_body = WebhookRequest {
            url,
            body: br#"{"x":1}"#,
            signature: Some(&sig),
            timestamp: None,
            nonce: None,
        };
        assert!(!adapter.verify_request(&bad_body));
        let wrong = sign("other-secret", body, url);
        let bad_key = WebhookRequest {
            url,
            body,
            signature: Some(&wrong),
            timestamp: None,
            nonce: None,
        };
        assert!(!adapter.verify_request(&bad_key));
        let no_sig = WebhookRequest {
            url,
            body,
            signature: None,
            timestamp: None,
            nonce: None,
        };
        assert!(!adapter.verify_request(&no_sig));
    }

    #[test]
    fn get_handshake_returns_ok_for_liveness_check() {
        let adapter = TrelloAdapter::new("s", "k", "t");
        assert_eq!(adapter.verify_get(""), Some(String::new()));
    }

    #[test]
    fn parses_comment_card_and_skips_other_actions() {
        let adapter = TrelloAdapter::new("s", "k", "t");
        let body = br#"{"action":{"type":"commentCard",
            "data":{"text":"please review","card":{"id":"card_9","name":"Ship it"}},
            "memberCreator":{"id":"mem_1","username":"sam"}}}"#;
        let events = adapter.parse_webhook(body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].chat_id, "card_9");
        assert_eq!(events[0].user_id, "mem_1");
        assert_eq!(events[0].text, "please review");

        let other = br#"{"action":{"type":"updateCard","data":{"card":{"id":"c1"}}}}"#;
        assert!(adapter.parse_webhook(other).unwrap().is_empty());
    }

    #[test]
    fn send_request_posts_a_card_comment() {
        let adapter = TrelloAdapter::new("s", "API-KEY", "TOKEN");
        let req = adapter.send_request(&OutboundMessage {
            chat_id: "card_9".into(),
            text: "on it".into(),
        });
        assert_eq!(
            req.url,
            "https://api.trello.com/1/cards/card_9/actions/comments"
        );
        assert_eq!(req.auth, SendAuth::None);
        let SendBody::Form(pairs) = &req.body else {
            panic!("expected form body")
        };
        assert!(pairs.contains(&("key".to_owned(), "API-KEY".to_owned())));
        assert!(pairs.contains(&("token".to_owned(), "TOKEN".to_owned())));
        assert!(pairs.contains(&("text".to_owned(), "on it".to_owned())));
    }
}
