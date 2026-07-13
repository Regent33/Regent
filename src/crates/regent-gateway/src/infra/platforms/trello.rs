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
#[path = "trello_tests.rs"]
mod tests;
