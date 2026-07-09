//! Feishu / Lark event-subscription adapter. Events arrive as JSON POSTs. Two
//! modes (see [`super::feishu_crypto`]):
//!
//! - **Plaintext:** authenticity is the Verification Token carried in the body.
//! - **Encrypted** (Encrypt Key set): the body is `{"encrypt": "<base64>"}`,
//!   authenticated by `X-Lark-Signature` (SHA256 over `ts ‖ nonce ‖ key ‖ body`)
//!   and decrypted with AES-256-CBC.
//!
//! A one-time `url_verification` request is answered via `handshake` (echo the
//! challenge). Replies use the `im/v1/messages` API with a tenant access token
//! (operator-supplied; auto-refresh is future work).

use super::feishu_crypto;
use crate::domain::contracts::{
    SendAuth, SendBody, SendRequest, SyncReply, WebhookAdapter, WebhookFileSender, WebhookRequest,
};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;

const SEND_URL: &str = "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type=chat_id";

pub struct FeishuAdapter {
    verification_token: String,
    encrypt_key: Option<String>,
    tenant_token: Option<String>,
}

impl FeishuAdapter {
    #[must_use]
    pub fn new(
        verification_token: impl Into<String>,
        encrypt_key: Option<String>,
        tenant_token: Option<String>,
    ) -> Self {
        Self {
            verification_token: verification_token.into(),
            encrypt_key: encrypt_key.filter(|k| !k.is_empty()),
            tenant_token: tenant_token.filter(|t| !t.is_empty()),
        }
    }

    /// The event JSON, decrypting the `encrypt` envelope when a key is set.
    fn decoded(&self, body: &[u8]) -> Option<Value> {
        let raw: Value = serde_json::from_slice(body).ok()?;
        match (
            &self.encrypt_key,
            raw.get("encrypt").and_then(Value::as_str),
        ) {
            (Some(key), Some(enc)) => {
                serde_json::from_slice(&feishu_crypto::decrypt(key, enc)?).ok()
            }
            _ => Some(raw),
        }
    }

    /// The Verification Token carried by the event (top-level v1.0, or under
    /// `header` in schema 2.0).
    fn body_token(value: &Value) -> Option<&str> {
        value
            .get("token")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/header/token").and_then(Value::as_str))
    }
}

impl WebhookAdapter for FeishuAdapter {
    fn platform(&self) -> &str {
        "feishu"
    }

    /// Feishu binds the signature to the nonce header, so the body-only path
    /// can't verify it — deny (the route uses `verify_request`).
    fn verify(&self, _body: &[u8], _signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        false
    }

    fn verify_request(&self, request: &WebhookRequest<'_>) -> bool {
        match &self.encrypt_key {
            // Encrypted mode: SHA256(ts ‖ nonce ‖ key ‖ body) == X-Lark-Signature.
            Some(key) => {
                let (Some(sig), Some(ts), Some(nonce)) =
                    (request.signature, request.timestamp, request.nonce)
                else {
                    return false;
                };
                let expected = feishu_crypto::sign(ts, nonce, key, request.body);
                feishu_crypto::ct_eq(expected.as_bytes(), sig.as_bytes())
            }
            // Plaintext mode: authenticity is the Verification Token in the body.
            None => {
                let Some(value) = self.decoded(request.body) else {
                    return false;
                };
                Self::body_token(&value).is_some_and(|t| {
                    feishu_crypto::ct_eq(t.as_bytes(), self.verification_token.as_bytes())
                })
            }
        }
    }

    fn handshake(&self, body: &[u8]) -> Option<SyncReply> {
        let value = self.decoded(body)?;
        if value.get("type").and_then(Value::as_str) == Some("url_verification") {
            let challenge = value
                .get("challenge")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            return Some(SyncReply::Json(json!({ "challenge": challenge })));
        }
        None
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let Some(value) = self.decoded(body) else {
            return Err(GatewayError::Parse(
                "feishu: undecryptable callback body".to_owned(),
            ));
        };
        // Only inbound message events; other callbacks are acked empty.
        let event_type = value
            .pointer("/header/event_type")
            .or_else(|| value.get("type"))
            .and_then(Value::as_str);
        if event_type != Some("im.message.receive_v1") {
            return Ok(Vec::new());
        }
        let event = &value["event"];
        let chat = event.pointer("/message/chat_id").and_then(Value::as_str);
        // `content` is itself a JSON string, e.g. {"text":"hi"}.
        let text = event
            .pointer("/message/content")
            .and_then(Value::as_str)
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .and_then(|c| c.get("text").and_then(Value::as_str).map(ToOwned::to_owned));
        let user = event
            .pointer("/sender/sender_id/open_id")
            .or_else(|| event.pointer("/sender/sender_id/user_id"))
            .and_then(Value::as_str);
        let (Some(chat), Some(text)) = (chat, text) else {
            return Ok(Vec::new());
        };
        Ok(vec![MessageEvent {
            platform: "feishu".to_owned(),
            chat_id: chat.to_owned(),
            user_id: user.unwrap_or(chat).to_owned(),
            text,
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: SEND_URL.to_owned(),
            auth: self
                .tenant_token
                .clone()
                .map_or(SendAuth::None, SendAuth::Bearer),
            body: SendBody::Json(json!({
                "receive_id": message.chat_id,
                "msg_type": "text",
                "content": json!({ "text": message.text }).to_string(),
            })),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("x-lark-signature")
    }

    fn timestamp_header(&self) -> Option<&str> {
        Some("x-lark-request-timestamp")
    }

    fn nonce_header(&self) -> Option<&str> {
        Some("x-lark-request-nonce")
    }
}

/// Feishu file send is two calls: upload the bytes to `im/v1/files` (returns a
/// `file_key`), then send a `file` message referencing it. Needs the tenant
/// access token. A non-empty caption follows as a separate text message.
#[async_trait]
impl WebhookFileSender for FeishuAdapter {
    async fn send_file(
        &self,
        client: &reqwest::Client,
        chat_id: &str,
        path: &Path,
        caption: &str,
    ) -> Result<(), GatewayError> {
        let token = self.tenant_token.as_deref().ok_or_else(|| {
            GatewayError::Transport("feishu tenant token not configured".to_owned())
        })?;
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| GatewayError::Transport(format!("read {}: {e}", path.display())))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_owned();

        // 1. Upload as a generic file (file_type "stream") → file_key.
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename.clone());
        let form = reqwest::multipart::Form::new()
            .text("file_type", "stream")
            .text("file_name", filename)
            .part("file", part);
        let resp = client
            .post("https://open.feishu.cn/open-apis/im/v1/files")
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let parsed: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        let file_key = parsed
            .get("data")
            .and_then(|d| d.get("file_key"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                GatewayError::Transport(format!("feishu file upload failed: {parsed}"))
            })?;

        // 2. Send a file message (content is a JSON string per the API).
        let body = json!({
            "receive_id": chat_id,
            "msg_type": "file",
            "content": json!({ "file_key": file_key }).to_string(),
        });
        client
            .post(SEND_URL)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        if !caption.is_empty() {
            let _ = client
                .post(SEND_URL)
                .bearer_auth(token)
                .json(&json!({
                    "receive_id": chat_id,
                    "msg_type": "text",
                    "content": json!({ "text": caption }).to_string(),
                }))
                .send()
                .await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req<'a>(body: &'a [u8], sig: Option<&'a str>, nonce: Option<&'a str>) -> WebhookRequest<'a> {
        WebhookRequest {
            url: "",
            body,
            signature: sig,
            timestamp: Some("1700000000"),
            nonce,
        }
    }

    #[test]
    fn plaintext_mode_verifies_token_and_handshakes() {
        let adapter = FeishuAdapter::new("vtok", None, None);
        let body = br#"{"type":"url_verification","challenge":"C9","token":"vtok"}"#;
        assert!(adapter.verify_request(&req(body, None, None)));
        let SyncReply::Json(reply) = adapter.handshake(body).unwrap() else {
            panic!("json")
        };
        assert_eq!(reply["challenge"], "C9");

        // Wrong token → rejected.
        let bad = br#"{"type":"url_verification","challenge":"C9","token":"nope"}"#;
        assert!(!adapter.verify_request(&req(bad, None, None)));
    }

    #[test]
    fn encrypted_mode_verifies_signature_and_decrypts() {
        let adapter = FeishuAdapter::new("vtok", Some("ekey".to_owned()), None);
        let plain = br#"{"type":"url_verification","challenge":"XYZ"}"#;
        let blob = feishu_crypto::encrypt("ekey", plain, &[3u8; 16]);
        let body = format!(r#"{{"encrypt":"{blob}"}}"#);
        let sig = feishu_crypto::sign("1700000000", "n1", "ekey", body.as_bytes());

        assert!(adapter.verify_request(&req(body.as_bytes(), Some(&sig), Some("n1"))));
        // Tampered signature and missing nonce are rejected.
        assert!(!adapter.verify_request(&req(body.as_bytes(), Some("deadbeef"), Some("n1"))));
        assert!(!adapter.verify_request(&req(body.as_bytes(), Some(&sig), None)));

        let SyncReply::Json(reply) = adapter.handshake(body.as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!(reply["challenge"], "XYZ");
    }

    #[test]
    fn parses_message_event_and_skips_others() {
        let adapter = FeishuAdapter::new("vtok", None, None);
        let body = br#"{"schema":"2.0",
            "header":{"event_type":"im.message.receive_v1","token":"vtok"},
            "event":{"message":{"chat_id":"oc_1","content":"{\"text\":\"hi there\"}"},
                     "sender":{"sender_id":{"open_id":"ou_1"}}}}"#;
        let events = adapter.parse_webhook(body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].chat_id, "oc_1");
        assert_eq!(events[0].user_id, "ou_1");
        assert_eq!(events[0].text, "hi there");

        let other = br#"{"schema":"2.0","header":{"event_type":"im.chat.updated_v1"},"event":{}}"#;
        assert!(adapter.parse_webhook(other).unwrap().is_empty());
    }

    #[test]
    fn send_request_targets_the_messages_api() {
        let adapter = FeishuAdapter::new("vtok", None, Some("tk".to_owned()));
        let req = adapter.send_request(&OutboundMessage {
            chat_id: "oc_1".into(),
            text: "yo".into(),
        });
        assert!(req.url.contains("/im/v1/messages"));
        assert_eq!(req.auth, SendAuth::Bearer("tk".into()));
        let SendBody::Json(body) = &req.body else {
            panic!("json body")
        };
        assert_eq!(body["receive_id"], "oc_1");
        assert_eq!(body["msg_type"], "text");
        assert!(body["content"].as_str().unwrap().contains("yo"));
    }
}
