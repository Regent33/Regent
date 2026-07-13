//! Unit tests for `voice` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn parses_voice_and_audio_file_ids_and_skips_text() {
    let body = json!({"ok": true, "result": [
        {"update_id": 10, "message": {
            "voice": {"file_id": "VID", "file_size": 1000}, "chat": {"id": 7}, "from": {"id": 3}}},
        {"update_id": 11, "message": {
            "audio": {"file_id": "AID"}, "chat": {"id": 8}, "from": {"id": 4}}},
        {"update_id": 12, "message": {"text": "hi", "chat": {"id": 1}, "from": {"id": 2}}}
    ]});
    let voices = parse_voice(&body);
    assert_eq!(voices.len(), 2);
    assert_eq!(
        voices[0],
        ("7".to_owned(), "3".to_owned(), "VID".to_owned())
    );
    assert_eq!(
        voices[1],
        ("8".to_owned(), "4".to_owned(), "AID".to_owned())
    );
    assert!(
        parse_voice(&json!({"result": [
            {"update_id": 1, "message": {"text": "x", "chat": {"id": 1}, "from": {"id": 2}}}
        ]}))
        .is_empty()
    );
}

#[test]
fn voice_mode_tracks_per_chat() {
    let a = TelegramAdapter::new("t");
    assert!(!a.is_voice_chat("7"));
    a.mark_voice("7");
    assert!(a.is_voice_chat("7"));
    a.clear_voice("7");
    assert!(!a.is_voice_chat("7"));
    assert!(a.tts.is_none()); // no speech wired
}
