//! WeChat Work / WeCom (企业微信) callback adapter. WeCom uses the **same**
//! WXBizMsgCrypt scheme as WeChat (see [`super::wechat_crypto`]) but is *always*
//! encrypted: the `GET` verification `echostr` is itself ciphertext that must be
//! decrypted and echoed back as plaintext, and message `POST`s carry an
//! `<Encrypt>` body signed by `msg_signature` in the query string. Replies go
//! out-of-band via the corp message API with an operator-supplied access token.

use super::wechat_crypto;
use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter, WebhookRequest};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use serde_json::json;

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
        wechat_crypto::signature(&[&self.token, ts, nonce, encrypt]) == sig
    }

    /// WeCom's `echostr` is **encrypted**: verify the query signature, then
    /// decrypt and echo the plaintext challenge (unlike WeChat's plaintext one).
    fn verify_get(&self, query: &str) -> Option<String> {
        let pairs = Self::parse_query(query);
        let sig = Self::param(&pairs, "msg_signature")?;
        let ts = Self::param(&pairs, "timestamp")?;
        let nonce = Self::param(&pairs, "nonce")?;
        let echostr = Self::param(&pairs, "echostr")?;
        if wechat_crypto::signature(&[&self.token, ts, nonce, echostr]) != sig {
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

    fn adapter() -> WeComAdapter {
        WeComAdapter::new("tok", aes_key(), Some("AT".to_owned()), "1000002")
    }

    #[test]
    fn get_handshake_decrypts_and_echoes_on_valid_signature() {
        let key = aes_key();
        let adapter = adapter();
        let echostr = wechat_crypto::encrypt(&key, b"the-echo-message", "corpid");
        let sig = wechat_crypto::signature(&["tok", "1700000000", "n1", &echostr]);
        let query = format!("msg_signature={sig}&timestamp=1700000000&nonce=n1&echostr={echostr}");
        assert_eq!(
            adapter.verify_get(&query),
            Some("the-echo-message".to_owned())
        );
    }

    #[test]
    fn get_handshake_rejects_bad_signature() {
        let key = aes_key();
        let adapter = adapter();
        let echostr = wechat_crypto::encrypt(&key, b"the-echo-message", "corpid");
        let bad = format!("msg_signature=deadbeef&timestamp=1700000000&nonce=n1&echostr={echostr}");
        assert_eq!(adapter.verify_get(&bad), None);
        // Missing params also fail closed.
        assert_eq!(adapter.verify_get("timestamp=1&nonce=n"), None);
    }

    #[test]
    fn verify_request_accepts_valid_and_rejects_tampering() {
        let key = aes_key();
        let adapter = adapter();
        let inner = "<xml><FromUserName><![CDATA[wecomuser]]></FromUserName>\
                     <MsgType><![CDATA[text]]></MsgType><Content><![CDATA[hi]]></Content></xml>";
        let blob = wechat_crypto::encrypt(&key, inner.as_bytes(), "corpid");
        let body = format!("<xml><Encrypt><![CDATA[{blob}]]></Encrypt></xml>");
        let sig = wechat_crypto::signature(&["tok", "1700000000", "n1", &blob]);
        let url =
            format!("https://x/webhook/wecom?msg_signature={sig}&timestamp=1700000000&nonce=n1");
        let ok = WebhookRequest {
            url: &url,
            body: body.as_bytes(),
            signature: None,
            timestamp: None,
            nonce: None,
        };
        assert!(adapter.verify_request(&ok));

        let bad_url =
            "https://x/webhook/wecom?msg_signature=deadbeef&timestamp=1700000000&nonce=n1";
        let bad = WebhookRequest {
            url: bad_url,
            body: body.as_bytes(),
            signature: None,
            timestamp: None,
            nonce: None,
        };
        assert!(!adapter.verify_request(&bad));
    }

    #[test]
    fn parse_webhook_decrypts_text_message() {
        let key = aes_key();
        let adapter = adapter();
        let inner = "<xml><FromUserName><![CDATA[wecomuser]]></FromUserName>\
                     <MsgType><![CDATA[text]]></MsgType><Content><![CDATA[hello wecom]]></Content></xml>";
        let blob = wechat_crypto::encrypt(&key, inner.as_bytes(), "corpid");
        let body = format!("<xml><Encrypt><![CDATA[{blob}]]></Encrypt></xml>");
        let events = adapter.parse_webhook(body.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].platform, "wecom");
        assert_eq!(events[0].chat_id, "wecomuser");
        assert_eq!(events[0].user_id, "wecomuser");
        assert_eq!(events[0].text, "hello wecom");
    }

    #[test]
    fn parse_webhook_skips_non_text_and_errors_on_undecryptable() {
        let key = aes_key();
        let adapter = adapter();
        let inner = "<xml><FromUserName><![CDATA[u]]></FromUserName>\
                     <MsgType><![CDATA[image]]></MsgType></xml>";
        let blob = wechat_crypto::encrypt(&key, inner.as_bytes(), "corpid");
        let body = format!("<xml><Encrypt><![CDATA[{blob}]]></Encrypt></xml>");
        assert!(adapter.parse_webhook(body.as_bytes()).unwrap().is_empty());

        // No <Encrypt> at all → empty (not a WeCom message envelope).
        assert!(adapter.parse_webhook(b"<xml/>").unwrap().is_empty());

        // Garbage ciphertext → fail closed with a Parse error.
        let garbage = "<xml><Encrypt><![CDATA[not-base64!!!]]></Encrypt></xml>";
        assert!(matches!(
            adapter.parse_webhook(garbage.as_bytes()),
            Err(GatewayError::Parse(_))
        ));
    }

    #[test]
    fn send_request_targets_corp_send_api_with_numeric_agentid() {
        let adapter = adapter();
        let req = adapter.send_request(&OutboundMessage {
            chat_id: "wecomuser".into(),
            text: "yo".into(),
        });
        assert!(req.url.contains("/cgi-bin/message/send?access_token=AT"));
        assert_eq!(req.auth, SendAuth::None);
        let SendBody::Json(body) = &req.body else {
            panic!("json body")
        };
        assert_eq!(body["touser"], "wecomuser");
        assert_eq!(body["msgtype"], "text");
        assert_eq!(body["agentid"], 1_000_002_i64);
        assert!(body["agentid"].is_i64());
        assert_eq!(body["text"]["content"], "yo");
    }

    #[test]
    fn send_request_keeps_non_numeric_agentid_as_string() {
        let adapter = WeComAdapter::new("tok", aes_key(), None, "agent-x");
        let req = adapter.send_request(&OutboundMessage {
            chat_id: "u".into(),
            text: "t".into(),
        });
        // No access token → empty token in URL.
        assert!(req.url.ends_with("access_token="));
        let SendBody::Json(body) = &req.body else {
            panic!("json body")
        };
        assert_eq!(body["agentid"], "agent-x");
        assert!(body["agentid"].is_string());
    }
}
