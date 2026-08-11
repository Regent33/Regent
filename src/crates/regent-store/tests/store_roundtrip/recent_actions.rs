//! "Pull up the last website we pulled up" — the URL was always in the store;
//! nothing could read it back by recency.

use regent_kernel::{ChatMessage, SessionId, ToolCall};
use regent_store::Store;

fn session(store: &Store, source: &str) -> SessionId {
    let id = SessionId::generate();
    store
        .create_session(&id, source, None, Some("prompt"), None)
        .unwrap();
    id
}

/// One `open_url` call + its result, as the agent records them.
fn opened(store: &Store, id: &SessionId, url: &str) {
    let call = ToolCall {
        id: format!("call-{url}"),
        name: "open_url".to_owned(),
        arguments: serde_json::json!({ "url": url }).to_string(),
    };
    store
        .append_message(
            id,
            &ChatMessage::assistant(None, vec![call.clone()]),
            None,
            None,
        )
        .unwrap();
    store
        .append_message(
            id,
            &ChatMessage::tool_result(&call.id, "open_url", format!("{{\"opened\":\"{url}\"}}")),
            None,
            None,
        )
        .unwrap();
}

#[test]
fn the_last_website_is_recoverable_across_surfaces() {
    let store = Store::open_in_memory().unwrap();
    // The reported shape: the site was opened by VOICE, and the user then asked
    // about it from a DIFFERENT (chat) session. A per-session lookup finds
    // nothing, which is exactly why this query spans sessions.
    let voice = session(&store, "voice");
    opened(&store, &voice, "https://example.com/first");
    opened(&store, &voice, "https://example.com/second");
    let chat = session(&store, "chat");

    let actions = store
        .recent_actions(Some(&["open_url".to_owned()]), 10, None)
        .unwrap();

    assert_eq!(actions.len(), 2, "both opens are in the log");
    assert!(
        actions[0].result.contains("second"),
        "newest first, got {:?}",
        actions[0].result
    );
    assert_eq!(actions[0].tool_name, "open_url");
    assert_eq!(actions[0].source, "voice", "the surface is reported");
    assert_ne!(
        actions[0].session_id,
        chat.to_string(),
        "the hit comes from the voice session, not the one asking"
    );
}

#[test]
fn the_originating_call_arguments_travel_with_the_result() {
    let store = Store::open_in_memory().unwrap();
    let id = session(&store, "chat");
    opened(&store, &id, "https://example.com/only");

    let actions = store.recent_actions(None, 10, None).unwrap();
    let args = actions[0].args.as_deref().expect("call args are carried");
    assert!(
        args.contains("https://example.com/only"),
        "a tool whose result omits its input is still recoverable, got {args}"
    );
}

#[test]
fn filters_by_tool_and_by_recency_window() {
    let store = Store::open_in_memory().unwrap();
    let id = session(&store, "chat");
    opened(&store, &id, "https://example.com/site");
    let call = ToolCall {
        id: "call-play".to_owned(),
        name: "play".to_owned(),
        arguments: "{}".to_owned(),
    };
    store
        .append_message(
            &id,
            &ChatMessage::assistant(None, vec![call.clone()]),
            None,
            None,
        )
        .unwrap();
    store
        .append_message(
            &id,
            &ChatMessage::tool_result(&call.id, "play", "playing"),
            None,
            None,
        )
        .unwrap();

    assert_eq!(store.recent_actions(None, 10, None).unwrap().len(), 2);
    assert_eq!(
        store
            .recent_actions(Some(&["open_url".to_owned()]), 10, None)
            .unwrap()
            .len(),
        1,
        "tool filter applies"
    );
    // A window in the future excludes everything already recorded.
    assert!(
        store
            .recent_actions(None, 10, Some(regent_store::now_epoch() + 60.0))
            .unwrap()
            .is_empty(),
        "recency window applies"
    );
}

#[test]
fn a_store_with_no_tool_calls_reports_nothing_rather_than_erroring() {
    let store = Store::open_in_memory().unwrap();
    let id = session(&store, "chat");
    store
        .append_message(&id, &ChatMessage::user("just talking"), None, None)
        .unwrap();
    assert!(store.recent_actions(None, 10, None).unwrap().is_empty());
}
