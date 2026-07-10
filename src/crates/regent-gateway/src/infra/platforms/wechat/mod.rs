//! WeChat Official Account (公众号) adapter. WeChat verifies the endpoint with a
//! `GET` (`signature`/`timestamp`/`nonce`/`echostr` query) and delivers messages
//! as XML `POST`s — plaintext, or encrypted (WXBizMsgCrypt) with `msg_signature`
//! in the query and an `<Encrypt>` body (see [`super::wechat_crypto`]). The
//! signing material rides the **query string**, not headers. We ack the POST and
//! reply asynchronously via the Customer Service message API (operator-supplied
//! access token; auto-refresh is future work). Media sends live in `media`.

mod media;
#[cfg(test)]
mod tests;

use super::wechat_crypto;
use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter, WebhookRequest};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use serde_json::json;

pub struct WeChatAdapter {
    token: String,
    encoding_aes_key: Option<String>,
    access_token: Option<String>,
}

impl WeChatAdapter {
    #[must_use]
    pub fn new(
        token: impl Into<String>,
        encoding_aes_key: Option<String>,
        access_token: Option<String>,
    ) -> Self {
        Self {
            token: token.into(),
            encoding_aes_key: encoding_aes_key.filter(|k| !k.is_empty()),
            access_token: access_token.filter(|t| !t.is_empty()),
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

    /// The access token for the Customer Service / media APIs, if configured.
    fn access_token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    /// The inner message XML — decrypted `<Encrypt>` when an AES key is set,
    /// else the body as-is.
    fn message_xml(&self, body: &str) -> Option<String> {
        match (
            &self.encoding_aes_key,
            wechat_crypto::xml_field(body, "Encrypt"),
        ) {
            (Some(key), Some(enc)) => {
                wechat_crypto::decrypt(key, enc).and_then(|bytes| String::from_utf8(bytes).ok())
            }
            _ => Some(body.to_owned()),
        }
    }
}

impl WebhookAdapter for WeChatAdapter {
    fn platform(&self) -> &str {
        "wechat"
    }

    /// Signing material is in the query string — the body-only path can't
    /// verify it (the route uses `verify_request`).
    fn verify(&self, _body: &[u8], _signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        false
    }

    fn verify_request(&self, request: &WebhookRequest<'_>) -> bool {
        let query = Self::url_query(request.url);
        let (Some(ts), Some(nonce)) = (
            Self::param(&query, "timestamp"),
            Self::param(&query, "nonce"),
        ) else {
            return false;
        };
        match &self.encoding_aes_key {
            Some(_) => {
                let Some(sig) = Self::param(&query, "msg_signature") else {
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
            None => {
                let Some(sig) = Self::param(&query, "signature") else {
                    return false;
                };
                wechat_crypto::ct_eq(
                    wechat_crypto::signature(&[&self.token, ts, nonce]).as_bytes(),
                    sig.as_bytes(),
                )
            }
        }
    }

    fn verify_get(&self, query: &str) -> Option<String> {
        let pairs = Self::parse_query(query);
        let ts = Self::param(&pairs, "timestamp")?;
        let nonce = Self::param(&pairs, "nonce")?;
        let echostr = Self::param(&pairs, "echostr")?;
        let signature = Self::param(&pairs, "signature")?;
        wechat_crypto::ct_eq(
            wechat_crypto::signature(&[&self.token, ts, nonce]).as_bytes(),
            signature.as_bytes(),
        )
        .then(|| echostr.to_owned())
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let raw = std::str::from_utf8(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        let Some(xml) = self.message_xml(raw) else {
            return Err(GatewayError::Parse(
                "wechat: undecryptable callback body".to_owned(),
            ));
        };
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
            platform: "wechat".to_owned(),
            chat_id: user.to_owned(),
            user_id: user.to_owned(),
            text: text.to_owned(),
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        let token = self.access_token.as_deref().unwrap_or_default();
        SendRequest {
            url: format!(
                "https://api.weixin.qq.com/cgi-bin/message/custom/send?access_token={token}"
            ),
            auth: SendAuth::None,
            body: SendBody::Json(json!({
                "touser": message.chat_id,
                "msgtype": "text",
                "text": { "content": message.text },
            })),
        }
    }

    /// WeChat carries the signature in the query string, not a header.
    fn signature_header(&self) -> Option<&str> {
        None
    }
}
