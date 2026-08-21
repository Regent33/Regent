//! Unit tests for `discord` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn identify_carries_token_and_message_content_intent() {
    let p = identify_payload("bot-tok");
    assert_eq!(p["op"], 2);
    assert_eq!(p["d"]["token"], "bot-tok");
    // MESSAGE_CONTENT (1<<15) must be set or content is empty.
    assert_eq!(p["d"]["intents"].as_u64().unwrap() & (1 << 15), 1 << 15);
}

#[test]
fn heartbeat_uses_null_then_the_last_sequence() {
    assert!(heartbeat_payload(None)["d"].is_null());
    assert_eq!(heartbeat_payload(Some(7))["d"], 7);
}

#[test]
fn parses_a_user_message_and_skips_bots_and_non_messages() {
    let msg = json!({"op":0,"t":"MESSAGE_CREATE","d":{
        "id":"M1","channel_id":"C1","content":"hi there","author":{"id":"U1","bot":false}}});
    let (event, message_id) = parse_message_create(&msg).unwrap();
    assert_eq!(event.chat_id, "C1");
    assert_eq!(event.user_id, "U1");
    assert_eq!(event.text, "hi there");
    // Carried beside the event so a later "react to that" has something to name.
    assert_eq!(message_id, "M1");

    let bot = json!({"op":0,"t":"MESSAGE_CREATE","d":{
        "id":"M2","channel_id":"C1","content":"x","author":{"id":"B1","bot":true}}});
    assert!(
        parse_message_create(&bot).is_none(),
        "bot messages are skipped"
    );

    let typing = json!({"op":0,"t":"TYPING_START","d":{}});
    assert!(parse_message_create(&typing).is_none());

    let empty = json!({"op":0,"t":"MESSAGE_CREATE","d":{
        "id":"M3","channel_id":"C1","content":"","author":{"id":"U1"}}});
    assert!(
        parse_message_create(&empty).is_none(),
        "empty content skipped"
    );
}

/// A message with no `id` cannot be reacted to, so it is not a usable event —
/// better to skip it than to hand the runner an event whose reaction target is
/// a lie.
#[test]
fn a_message_without_an_id_is_skipped() {
    let no_id = json!({"op":0,"t":"MESSAGE_CREATE","d":{
        "channel_id":"C1","content":"hi","author":{"id":"U1"}}});
    assert!(parse_message_create(&no_id).is_none());
}
