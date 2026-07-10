//! Handshake, signature, decrypt, and media-body behavior.

use super::media::{wechat_media_body, wechat_media_kind};
use super::{WeChatAdapter, wechat_crypto};
use crate::domain::contracts::{SendBody, WebhookAdapter, WebhookRequest};
use crate::domain::entities::OutboundMessage;
use std::path::Path;

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

#[test]
fn media_kind_maps_supported_types_and_rejects_documents() {
    assert_eq!(wechat_media_kind(Path::new("a.JPG")), Some("image"));
    assert_eq!(wechat_media_kind(Path::new("a.mp3")), Some("voice"));
    assert_eq!(wechat_media_kind(Path::new("a.mp4")), Some("video"));
    assert_eq!(wechat_media_kind(Path::new("a.pdf")), None);
    assert_eq!(wechat_media_kind(Path::new("noext")), None);
}

#[test]
fn media_body_nests_media_id_under_the_msgtype() {
    let body = wechat_media_body("openid1", "image", "MID9");
    assert_eq!(body["touser"], "openid1");
    assert_eq!(body["msgtype"], "image");
    assert_eq!(body["image"]["media_id"], "MID9");
}
