//! Inbound attachment parsing: the shapes that used to be dropped silently.

use super::*;
use serde_json::json;

fn body(message: Value) -> Value {
    json!({"result": [{"update_id": 1, "message": message}]})
}

#[test]
fn picks_the_largest_photo_size_and_keeps_the_caption() {
    // Telegram sends a size ladder; the thumbnail is useless for vision.
    let items = parse_attachments(&body(json!({
        "chat": {"id": -100}, "from": {"id": 7},
        "caption": "what is this?",
        "photo": [
            {"file_id": "small", "width": 90},
            {"file_id": "large", "width": 1280}
        ]
    })));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].file_id, "large");
    assert_eq!(items[0].caption, "what is this?");
    assert_eq!(items[0].chat_id, "-100");
    assert!(items[0].file_name.is_none());
}

#[test]
fn documents_and_video_are_attachments_too_and_text_is_not() {
    let doc = parse_attachments(&body(json!({
        "chat": {"id": 1}, "from": {"id": 2},
        "document": {"file_id": "d1", "file_name": "report.pdf"}
    })));
    assert_eq!(doc[0].file_name.as_deref(), Some("report.pdf"));

    let video = parse_attachments(&body(json!({
        "chat": {"id": 1}, "from": {"id": 2}, "video": {"file_id": "v1"}
    })));
    assert_eq!(video.len(), 1);

    // A plain text message is the text path's business, not this one.
    let text = parse_attachments(&body(json!({
        "chat": {"id": 1}, "from": {"id": 2}, "text": "hello"
    })));
    assert!(text.is_empty());
}

#[test]
fn saved_names_are_unique_and_cannot_escape_the_inbox() {
    // Same name from two messages must not overwrite each other.
    let a = unique_name(Some("photo.jpg"), "AgACAgQAAx0");
    let b = unique_name(Some("photo.jpg"), "BbBBBgQAAx1");
    assert_ne!(a, b);
    assert!(a.ends_with(".jpg"), "extension preserved: {a}");

    // A hostile sender-supplied name stays a single file name.
    let evil = unique_name(Some("../../.ssh/authorized_keys"), "id123456");
    assert!(!evil.contains('/') && !evil.contains('\\'), "got {evil}");
    assert!(!evil.starts_with('.'), "got {evil}");

    // Photos have no name at all.
    assert!(unique_name(None, "id123456").starts_with("photo-"));
}
