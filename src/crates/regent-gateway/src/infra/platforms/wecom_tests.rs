//! Unit tests for `wecom` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

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
    let url = format!("https://x/webhook/wecom?msg_signature={sig}&timestamp=1700000000&nonce=n1");
    let ok = WebhookRequest {
        url: &url,
        body: body.as_bytes(),
        signature: None,
        timestamp: None,
        nonce: None,
    };
    assert!(adapter.verify_request(&ok));

    let bad_url = "https://x/webhook/wecom?msg_signature=deadbeef&timestamp=1700000000&nonce=n1";
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
