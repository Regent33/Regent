//! Unit tests for `whatsapp` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn verifies_a_valid_signature_and_rejects_others() {
    let adapter = WhatsAppAdapter::new("app-secret", "tok", "PHONE");
    let body = br#"{"object":"whatsapp_business_account"}"#;
    assert!(adapter.verify(body, Some(&sign("app-secret", body)), None));
    assert!(!adapter.verify(body, Some("sha256=deadbeef"), None));
    assert!(!adapter.verify(body, None, None));
    assert!(!adapter.verify(body, Some(&sign("wrong", body)), None));
}

#[test]
fn parses_text_messages_and_skips_status_callbacks() {
    let adapter = WhatsAppAdapter::new("s", "t", "PHONE");
    let body = br#"{"entry":[{"changes":[
        {"value":{"messages":[{"from":"15551234","type":"text","text":{"body":"hi"}}]}},
        {"value":{"statuses":[{"status":"delivered"}]}}
    ]}]}"#;
    let events = adapter.parse_webhook(body).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "15551234");
    assert_eq!(events[0].text, "hi");
    assert_eq!(events[0].user_key(), "whatsapp:15551234");
}

#[test]
fn send_request_targets_the_cloud_api() {
    let adapter = WhatsAppAdapter::new("s", "WA_TOKEN", "PHONE42");
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "15551234".into(),
        text: "hey".into(),
    });
    assert_eq!(req.url, "https://graph.facebook.com/v21.0/PHONE42/messages");
    assert_eq!(req.auth, SendAuth::Bearer("WA_TOKEN".into()));
    let SendBody::Json(body) = &req.body else {
        panic!("expected json body")
    };
    assert_eq!(body["messaging_product"], "whatsapp");
    assert_eq!(body["to"], "15551234");
    assert_eq!(body["text"]["body"], "hey");
}

#[test]
fn mime_is_inferred_from_the_extension() {
    assert_eq!(wa_mime_for(Path::new("a.PNG")), "image/png");
    assert_eq!(wa_mime_for(Path::new("a.jpeg")), "image/jpeg");
    assert_eq!(wa_mime_for(Path::new("report.pdf")), "application/pdf");
    assert_eq!(wa_mime_for(Path::new("clip.mp4")), "video/mp4");
    assert_eq!(wa_mime_for(Path::new("noext")), "application/octet-stream");
}

#[test]
fn message_type_buckets_by_mime_prefix() {
    assert_eq!(wa_message_type("image/png"), "image");
    assert_eq!(wa_message_type("video/mp4"), "video");
    assert_eq!(wa_message_type("application/pdf"), "document");
    // arbitrary audio rides as a document (voice notes are a separate path).
    assert_eq!(wa_message_type("audio/mpeg"), "document");
}

#[test]
fn media_body_attaches_id_caption_and_filename_per_type() {
    // Document: id + caption + filename, under the "document" key.
    let doc = wa_media_body("1555", "MID1", "document", "report.pdf", "see this");
    assert_eq!(doc["type"], "document");
    assert_eq!(doc["to"], "1555");
    assert_eq!(doc["document"]["id"], "MID1");
    assert_eq!(doc["document"]["caption"], "see this");
    assert_eq!(doc["document"]["filename"], "report.pdf");

    // Image: id + caption, no filename.
    let img = wa_media_body("1555", "MID2", "image", "p.png", "hi");
    assert_eq!(img["image"]["id"], "MID2");
    assert_eq!(img["image"]["caption"], "hi");
    assert!(img["image"].get("filename").is_none());

    // Empty caption is omitted entirely.
    let bare = wa_media_body("1555", "MID3", "image", "p.png", "");
    assert!(bare["image"].get("caption").is_none());
}
