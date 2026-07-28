//! W11: a search hit alone is often unreadable, so the surrounding turns come
//! back with it. Split from `search_window.rs` (file-size rule).

use super::*;
use regent_kernel::{ChatMessage, SessionId};

/// The exact shape the feature exists for: the hit is *"yes, do that"*, which
/// answers a question it does not contain.
fn conversation() -> (Store, SessionId, i64) {
    let store = Store::open_in_memory().unwrap();
    let session = SessionId::generate();
    store
        .create_session(&session, "cli", None, None, None)
        .unwrap();
    let mut hit_id = 0;
    let turns = [
        ChatMessage::user("how should we handle the failover cap?"),
        ChatMessage::assistant(
            Some("I'd bound it to two hops and cool down.".into()),
            vec![],
        ),
        ChatMessage::user("yes, do that"),
        ChatMessage::assistant(Some("Done — capped at two hops.".into()), vec![]),
        ChatMessage::user("now update the changelog"),
    ];
    for message in &turns {
        let text = message.content.clone().unwrap_or_default();
        let id = store.append_message(&session, message, None, None).unwrap();
        if text == "yes, do that" {
            hit_id = id;
        }
    }
    (store, session, hit_id)
}

#[test]
fn the_window_reads_forwards_and_is_signed_by_distance() {
    let (store, session, hit) = conversation();
    let window = store.message_window(session.as_str(), hit, 1).unwrap();

    assert_eq!(window.len(), 2);
    assert_eq!(window[0].offset, -1);
    assert_eq!(window[0].content, "I'd bound it to two hops and cool down.");
    assert_eq!(window[1].offset, 1);
    assert_eq!(window[1].content, "Done — capped at two hops.");
    assert!(
        !window.iter().any(|m| m.content == "yes, do that"),
        "the hit itself is excluded — the caller already has it"
    );
}

/// Nearest-first-out: at radius 2 the earlier turn is -2, not -1. Getting this
/// backwards would render the exchange in the wrong order.
#[test]
fn a_wider_radius_orders_by_distance_not_by_scan_order() {
    let (store, session, hit) = conversation();
    let window = store.message_window(session.as_str(), hit, 2).unwrap();

    let offsets: Vec<i64> = window.iter().map(|m| m.offset).collect();
    assert_eq!(offsets, vec![-2, -1, 1, 2]);
    assert_eq!(window[0].content, "how should we handle the failover cap?");
    assert_eq!(window[3].content, "now update the changelog");
}

/// A hit at the very start of a session has nothing before it. Returning fewer
/// than `2 * radius` is correct, not an error.
#[test]
fn a_hit_at_the_edge_of_a_session_returns_only_what_exists() {
    let (store, session, _) = conversation();
    let first = store
        .search_messages("failover", 5)
        .unwrap()
        .into_iter()
        .find(|h| h.role == "user")
        .expect("the opening question is findable");

    let window = store
        .message_window(session.as_str(), first.message_id, 3)
        .unwrap();
    assert!(
        window.iter().all(|m| m.offset > 0),
        "nothing precedes the first message: {window:?}"
    );
}

#[test]
fn radius_zero_reads_nothing() {
    let (store, session, hit) = conversation();
    assert!(
        store
            .message_window(session.as_str(), hit, 0)
            .unwrap()
            .is_empty()
    );
}

/// The window never leaks across sessions — a neighbouring `id` can belong to
/// a different conversation entirely, since `id` is global.
#[test]
fn the_window_never_crosses_a_session_boundary() {
    let (store, session, hit) = conversation();
    let other = SessionId::generate();
    store
        .create_session(&other, "cli", None, None, None)
        .unwrap();
    store
        .append_message(
            &other,
            &ChatMessage::user("a different conversation entirely"),
            None,
            None,
        )
        .unwrap();

    let window = store.message_window(session.as_str(), hit, 5).unwrap();
    assert!(
        !window
            .iter()
            .any(|m| m.content == "a different conversation entirely"),
        "ids are global; the filter must be per-session: {window:?}"
    );
}
