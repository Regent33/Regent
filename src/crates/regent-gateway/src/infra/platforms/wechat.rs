//! WeChat Official Account (公众号) adapter. WeChat verifies the endpoint with a
//! `GET` (`signature`/`timestamp`/`nonce`/`echostr` query) and delivers messages
//! as XML `POST`s — plaintext, or encrypted (WXBizMsgCrypt) with `msg_signature`
//! in the query and an `<Encrypt>` body (see [`super::wechat_crypto`]). The
//! signing material rides the **query string**, not headers. We ack the POST and
//! reply asynchronously via the Customer Service message API (operator-supplied
//! access token; auto-refresh is future work).

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
                wechat_crypto::signature(&[&self.token, ts, nonce, encrypt]) == sig
            }
            None => {
                let Some(sig) = Self::param(&query, "signature") else {
                    return false;
                };
                wechat_crypto::signature(&[&self.token, ts, nonce]) == sig
            }
        }
    }

    fn verify_get(&self, query: &str) -> Option<String> {
        let pairs = Self::parse_query(query);
        let ts = Self::param(&pairs, "timestamp")?;
        let nonce = Self::param(&pairs, "nonce")?;
        let echostr = Self::param(&pairs, "echostr")?;
        let signature = Self::param(&pairs, "signature")?;
        (wechat_crypto::signature(&[&self.token, ts, nonce]) == signature)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn aes_key() -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .encode([42u8; 32])
            .trim_end_matches('=')
            .to_owned()
    }

    #[test]
    fn get_handshake_echoes_on_valid_signature() {
        let adapter = WeChatAdapter::new("tok", None, None);
        let sig = wechat_crypto::signature(&["tok", "1700000000", "n1"]);
        let query = format!("signature={sig}&timestamp=1700000000&nonce=n1&echostr=HELLO");
        assert_eq!(adapter.verify_get(&query), Some("HELLO".to_owned()));

        let bad = "signature=deadbeef&timestamp=1700000000&nonce=n1&echostr=HELLO";
        assert_eq!(adapter.verify_get(bad), None);
    }

    #[test]
    fn verify_request_plaintext_checks_query_signature() {
        let adapter = WeChatAdapter::new("tok", None, None);
        let sig = wechat_crypto::signature(&["tok", "1700000000", "n1"]);
        let url = format!("https://x/webhook/wechat?signature={sig}&timestamp=1700000000&nonce=n1");
        let ok = WebhookRequest {
            url: &url,
            body: b"<xml/>",
            signature: None,
            timestamp: None,
            nonce: None,
        };
        assert!(adapter.verify_request(&ok));

        let bad_url = "https://x/webhook/wechat?signature=deadbeef&timestamp=1700000000&nonce=n1";
        let bad = WebhookRequest {
            url: bad_url,
            body: b"<xml/>",
            signature: None,
            timestamp: None,
            nonce: None,
        };
        assert!(!adapter.verify_request(&bad));
    }

    #[test]
    fn encrypted_mode_verifies_and_decrypts_a_message() {
        let key = aes_key();
        let adapter = WeChatAdapter::new("tok", Some(key.clone()), None);
        let inner = "<xml><FromUserName><![CDATA[openid1]]></FromUserName>\
                     <MsgType><![CDATA[text]]></MsgType><Content><![CDATA[hello]]></Content></xml>";
        let blob = wechat_crypto::encrypt(&key, inner.as_bytes(), "wxappid");
        let body = format!("<xml><Encrypt><![CDATA[{blob}]]></Encrypt></xml>");
        let sig = wechat_crypto::signature(&["tok", "1700000000", "n1", &blob]);
        let url =
            format!("https://x/webhook/wechat?msg_signature={sig}&timestamp=1700000000&nonce=n1");

        let req = WebhookRequest {
            url: &url,
            body: body.as_bytes(),
            signature: None,
            timestamp: None,
            nonce: None,
        };
        assert!(adapter.verify_request(&req));

        let events = adapter.parse_webhook(body.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].chat_id, "openid1");
        assert_eq!(events[0].text, "hello");
    }

    #[test]
    fn parses_plaintext_text_message_and_skips_non_text() {
        let adapter = WeChatAdapter::new("tok", None, None);
        let body = b"<xml><ToUserName><![CDATA[gh_1]]></ToUserName>\
            <FromUserName><![CDATA[openid1]]></FromUserName><MsgType><![CDATA[text]]></MsgType>\
            <Content><![CDATA[hi there]]></Content></xml>";
        let events = adapter.parse_webhook(body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].user_id, "openid1");
        assert_eq!(events[0].text, "hi there");

        let image = b"<xml><FromUserName><![CDATA[o]]></FromUserName><MsgType><![CDATA[image]]></MsgType></xml>";
        assert!(adapter.parse_webhook(image).unwrap().is_empty());
    }

    #[test]
    fn send_request_targets_custom_send_api() {
        let adapter = WeChatAdapter::new("tok", None, Some("AT".to_owned()));
        let req = adapter.send_request(&OutboundMessage {
            chat_id: "openid1".into(),
            text: "yo".into(),
        });
        assert!(
            req.url
                .contains("/cgi-bin/message/custom/send?access_token=AT")
        );
        let SendBody::Json(body) = &req.body else {
            panic!("json body")
        };
        assert_eq!(body["touser"], "openid1");
        assert_eq!(body["msgtype"], "text");
        assert_eq!(body["text"]["content"], "yo");
    }
}
