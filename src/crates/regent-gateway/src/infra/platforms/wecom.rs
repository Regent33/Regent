//! WeChat Work / WeCom (企业微信) callback adapter. WeCom uses the **same**
//! WXBizMsgCrypt scheme as WeChat (see [`super::wechat_crypto`]) but is *always*
//! encrypted: the `GET` verification `echostr` is itself ciphertext that must be
//! decrypted and echoed back as plaintext, and message `POST`s carry an
//! `<Encrypt>` body signed by `msg_signature` in the query string. Replies go
//! out-of-band via the corp message API with an operator-supplied access token.

use super::wechat_crypto;
use crate::domain::contracts::{
    SendAuth, SendBody, SendRequest, WebhookAdapter, WebhookFileSender, WebhookRequest,
};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;

pub struct WeComAdapter {
    token: String,
    encoding_aes_key: String,
    access_token: Option<String>,
    agent_id: String,
}

impl WeComAdapter {
    #[must_use]
    pub fn new(
        token: impl Into<String>,
        encoding_aes_key: impl Into<String>,
        access_token: Option<String>,
        agent_id: impl Into<String>,
    ) -> Self {
        Self {
            token: token.into(),
            encoding_aes_key: encoding_aes_key.into(),
            access_token: access_token.filter(|t| !t.is_empty()),
            agent_id: agent_id.into(),
        }
    }

    fn parse_query(query: &str) -> Vec<(String, String)> {
        form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    fn url_query(url: &str) -> Vec<(String, String)> {
        Self::parse_query(url.split_once('?').map_or("", |(_, q)| q))
    }

    fn param<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

impl WebhookAdapter for WeComAdapter {
    fn platform(&self) -> &str {
        "wecom"
    }

    /// Signing material is in the query string — the body-only path can't
    /// verify it (the route uses `verify_request`).
    fn verify(&self, _body: &[u8], _signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        false
    }

    fn verify_request(&self, request: &WebhookRequest<'_>) -> bool {
        let query = Self::url_query(request.url);
        let (Some(sig), Some(ts), Some(nonce)) = (
            Self::param(&query, "msg_signature"),
            Self::param(&query, "timestamp"),
            Self::param(&query, "nonce"),
        ) else {
            return false;
        };
        let body = std::str::from_utf8(request.body).unwrap_or_default();
        let Some(encrypt) = wechat_crypto::xml_field(body, "Encrypt") else {
            return false;
        };
        wechat_crypto::ct_eq(
            wechat_crypto::signature(&[&self.token, ts, nonce, encrypt]).as_bytes(),
            sig.as_bytes(),
        )
    }

    /// WeCom's `echostr` is **encrypted**: verify the query signature, then
    /// decrypt and echo the plaintext challenge (unlike WeChat's plaintext one).
    fn verify_get(&self, query: &str) -> Option<String> {
        let pairs = Self::parse_query(query);
        let sig = Self::param(&pairs, "msg_signature")?;
        let ts = Self::param(&pairs, "timestamp")?;
        let nonce = Self::param(&pairs, "nonce")?;
        let echostr = Self::param(&pairs, "echostr")?;
        if !wechat_crypto::ct_eq(
            wechat_crypto::signature(&[&self.token, ts, nonce, echostr]).as_bytes(),
            sig.as_bytes(),
        ) {
            return None;
        }
        let plain = wechat_crypto::decrypt(&self.encoding_aes_key, echostr)?;
        String::from_utf8(plain).ok()
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let raw = std::str::from_utf8(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        let Some(encrypt) = wechat_crypto::xml_field(raw, "Encrypt") else {
            return Ok(Vec::new());
        };
        let Some(bytes) = wechat_crypto::decrypt(&self.encoding_aes_key, encrypt) else {
            return Err(GatewayError::Parse(
                "wecom: undecryptable callback body".to_owned(),
            ));
        };
        let xml = String::from_utf8(bytes).map_err(|e| GatewayError::Parse(e.to_string()))?;
        if wechat_crypto::xml_field(&xml, "MsgType") != Some("text") {
            return Ok(Vec::new());
        }
        let (Some(user), Some(text)) = (
            wechat_crypto::xml_field(&xml, "FromUserName"),
            wechat_crypto::xml_field(&xml, "Content"),
        ) else {
            return Ok(Vec::new());
        };
        Ok(vec![MessageEvent {
            platform: "wecom".to_owned(),
            chat_id: user.to_owned(),
            user_id: user.to_owned(),
            text: text.to_owned(),
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        let token = self.access_token.as_deref().unwrap_or_default();
        // WeCom's `agentid` is numeric — send it as a number when it parses,
        // else fall back to the raw string (constructor stays string-typed).
        let agent_id = self
            .agent_id
            .parse::<i64>()
            .map_or_else(|_| json!(self.agent_id), |n| json!(n));
        SendRequest {
            url: format!("https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={token}"),
            auth: SendAuth::None,
            body: SendBody::Json(json!({
                "touser": message.chat_id,
                "msgtype": "text",
                "agentid": agent_id,
                "text": { "content": message.text },
            })),
        }
    }

    /// WeCom carries the signature in the query string, not a header.
    fn signature_header(&self) -> Option<&str> {
        None
    }
}

/// WeCom file send is two calls: upload temporary media (`type=file`) → media_id,
/// then send a `file` message by that id. Needs the operator access token; a
/// non-empty caption follows as a separate text message.
#[async_trait]
impl WebhookFileSender for WeComAdapter {
    async fn send_file(
        &self,
        client: &reqwest::Client,
        chat_id: &str,
        path: &Path,
        caption: &str,
    ) -> Result<(), GatewayError> {
        let token = self.access_token.as_deref().ok_or_else(|| {
            GatewayError::Transport("wecom access token not configured".to_owned())
        })?;
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| GatewayError::Transport(format!("read {}: {e}", path.display())))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_owned();

        // 1. Upload temporary media → media_id.
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
        let form = reqwest::multipart::Form::new().part("media", part);
        let upload_url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/media/upload?access_token={token}&type=file"
        );
        let resp = client
            .post(upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let parsed: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        let media_id = parsed
            .get("media_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                GatewayError::Transport(format!("wecom media upload failed: {parsed}"))
            })?;

        // 2. Send the file message by media id (agentid numeric when it parses).
        let agent_id = self
            .agent_id
            .parse::<i64>()
            .map_or_else(|_| json!(self.agent_id), |n| json!(n));
        let send_url =
            format!("https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={token}");
        client
            .post(&send_url)
            .json(&json!({
                "touser": chat_id,
                "msgtype": "file",
                "agentid": agent_id.clone(),
                "file": { "media_id": media_id },
            }))
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        if !caption.is_empty() {
            let _ = client
                .post(&send_url)
                .json(&json!({
                    "touser": chat_id,
                    "msgtype": "text",
                    "agentid": agent_id,
                    "text": { "content": caption },
                }))
                .send()
                .await;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "wecom_tests.rs"]
mod tests;
