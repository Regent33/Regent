//! Google Chat app adapter. Google authenticates each request with a bearer
//! **JWT** (`Authorization: Bearer <jwt>`) issued and RS256-signed by
//! `chat@system.gserviceaccount.com`, with `aud` = the Cloud project number.
//! We verify it against Google's rotating public keys (JWKS), cached in a
//! sync-readable map that a background task refreshes — `verify` itself stays
//! synchronous (no network on the request path). Replies are returned
//! synchronously in the HTTP response body (`{"text": …}`), like Teams.
//!
//! Reference: <https://developers.google.com/workspace/chat/verify-requests-from-chat>.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, SyncReply, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const ISSUER: &str = "chat@system.gserviceaccount.com";
const JWK_URL: &str =
    "https://www.googleapis.com/service_accounts/v1/jwk/chat@system.gserviceaccount.com";

pub struct GoogleChatAdapter {
    /// Expected `aud` — the Google Cloud project number.
    audience: String,
    /// `kid` → RS256 public key, refreshed from Google's JWKS in the background.
    keys: Arc<RwLock<HashMap<String, DecodingKey>>>,
    client: reqwest::Client,
}

impl GoogleChatAdapter {
    #[must_use]
    pub fn new(audience: impl Into<String>) -> Self {
        Self {
            audience: audience.into(),
            keys: Arc::new(RwLock::new(HashMap::new())),
            client: reqwest::Client::new(),
        }
    }

    /// Spawns a background task that refreshes Google's JWKS now and hourly, so
    /// `verify` can read the key cache synchronously. Call once at registration
    /// (inside a Tokio runtime).
    pub fn spawn_refresher(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                self.refresh().await;
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });
    }

    async fn refresh(&self) {
        match self.fetch_keys().await {
            Ok(keys) => *self.keys.write().unwrap() = keys,
            Err(error) => tracing::warn!(%error, "google chat JWKS refresh failed"),
        }
    }

    async fn fetch_keys(&self) -> Result<HashMap<String, DecodingKey>, reqwest::Error> {
        let jwks: Value = self.client.get(JWK_URL).send().await?.json().await?;
        let mut keys = HashMap::new();
        for jwk in jwks
            .get("keys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(kid), Some(n), Some(e)) = (
                jwk.get("kid").and_then(Value::as_str),
                jwk.get("n").and_then(Value::as_str),
                jwk.get("e").and_then(Value::as_str),
            ) else {
                continue;
            };
            if let Ok(key) = DecodingKey::from_rsa_components(n, e) {
                keys.insert(kid.to_owned(), key);
            }
        }
        Ok(keys)
    }

    /// Validates a bare JWT (no `Bearer ` prefix) against the cached keys.
    fn verify_token(&self, token: &str) -> bool {
        let Ok(header) = decode_header(token) else {
            return false;
        };
        let Some(kid) = header.kid else {
            return false;
        };
        let keys = self.keys.read().unwrap();
        let Some(key) = keys.get(&kid) else {
            return false; // unknown/rotated key (or cache not yet warm) → deny
        };
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[ISSUER]);
        validation.set_audience(&[&self.audience]);
        decode::<Value>(token, key, &validation).is_ok()
    }
}

impl WebhookAdapter for GoogleChatAdapter {
    fn platform(&self) -> &str {
        "google_chat"
    }

    fn verify(&self, _body: &[u8], signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        let Some(raw) = signature else {
            return false;
        };
        self.verify_token(raw.strip_prefix("Bearer ").unwrap_or(raw))
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        if value.get("type").and_then(Value::as_str) != Some("MESSAGE") {
            return Ok(Vec::new());
        }
        let Some(text) = value.pointer("/message/text").and_then(Value::as_str) else {
            return Ok(Vec::new());
        };
        let space = value
            .pointer("/message/space/name")
            .or_else(|| value.pointer("/space/name"))
            .and_then(Value::as_str)
            .unwrap_or("google_chat");
        let user = value
            .pointer("/message/sender/name")
            .and_then(Value::as_str)
            .unwrap_or(space);
        Ok(vec![MessageEvent {
            platform: "google_chat".to_owned(),
            chat_id: space.to_owned(),
            user_id: user.to_owned(),
            text: text.to_owned(),
        }])
    }

    fn send_request(&self, _message: &OutboundMessage) -> SendRequest {
        // Google Chat replies synchronously (see `sync_reply`); the route never
        // calls this for this adapter.
        SendRequest {
            url: String::new(),
            auth: SendAuth::None,
            body: SendBody::Json(Value::Null),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("authorization")
    }

    fn sync_reply(&self) -> bool {
        true
    }

    fn sync_response(&self, reply: &str) -> SyncReply {
        SyncReply::Json(json!({ "text": reply }))
    }
}

#[cfg(test)]
#[path = "google_chat_tests.rs"]
mod tests;
