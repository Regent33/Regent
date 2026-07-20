//! FTS search, session organization (rename/pin/archive/delete), and the
//! empty-session sweep.

use regent_kernel::{ChatMessage, SessionId};
use regent_store::Store;

#[test]
fn fts_search_finds_content_and_tool_names() {
    let store = Store::open_in_memory().unwrap();
    let session = SessionId::generate();
    store
        .create_session(&session, "cli", None, None, None)
        .unwrap();
    store
        .append_message(
            &session,
            &ChatMessage::user("deploy the docker container"),
            None,
            None,
        )
        .unwrap();
    store
        .append_message(
            &session,
            &ChatMessage::tool_result("c1", "terminal", r#"{"stdout":"done"}"#),
            None,
            None,
        )
        .unwrap();

    let hits = store.search_messages("docker", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.contains(">>>docker<<<"));

    // tool_name column is searchable too
    let hits = store.search_messages("terminal", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].role, "tool");

    // sanitizer prevents FTS syntax errors from raw input
    assert!(store.search_messages("\"", 10).unwrap().is_empty());
}

#[test]
fn session_organization_round_trip() {
    // rename/pin/archive land in list_sessions; delete removes row + messages.
    let store = Store::open_in_memory().unwrap();
    let session = SessionId::generate();
    store
        .create_session(&session, "cli", None, None, None)
        .unwrap();
    store
        .append_message(&session, &ChatMessage::user("hello"), None, None)
        .unwrap();

    assert!(store.rename_session(&session, Some("My chat")).unwrap());
    assert!(store.set_session_pinned(&session, true).unwrap());
    assert!(store.set_session_archived(&session, true).unwrap());
    let meta = &store.list_sessions(10).unwrap()[0];
    assert_eq!(meta.title.as_deref(), Some("My chat"));
    assert!(meta.pinned);
    assert!(meta.archived);

    assert!(store.delete_session(&session).unwrap());
    assert!(store.list_sessions(10).unwrap().is_empty());
    assert!(store.get_conversation(&session).is_err(), "history gone");
    // A stale id is a soft miss, not an error.
    assert!(!store.delete_session(&session).unwrap());
}

#[test]
fn empty_session_sweep_spares_content_children_and_fresh_rows() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.db")).unwrap();

    // An old empty session — the sweep's target.
    let empty = SessionId::generate();
    store
        .create_session(&empty, "deacon", None, None, None)
        .unwrap();
    // An old session WITH a message — must survive.
    let with_msg = SessionId::generate();
    store
        .create_session(&with_msg, "deacon", None, None, None)
        .unwrap();
    store
        .append_message(&with_msg, &ChatMessage::user("hi"), None, None)
        .unwrap();
    // An old empty session that PARENTS a child (delegation) — must survive.
    let parent = SessionId::generate();
    store
        .create_session(&parent, "deacon", None, None, None)
        .unwrap();
    let child = SessionId::generate();
    store
        .create_session(&child, "delegate", None, None, Some(&parent))
        .unwrap();
    store
        .append_message(&child, &ChatMessage::user("task"), None, None)
        .unwrap();

    // Age is measured against started_at: min_age 0 makes every row "old"
    // EXCEPT ones the grace period is meant to protect — prove that with a
    // large min_age first: nothing qualifies, nothing is deleted.
    assert_eq!(store.delete_empty_sessions(3600.0).unwrap(), 0);
    // With no grace: exactly the empty leaf goes; content, parent, child stay.
    assert_eq!(store.delete_empty_sessions(0.0).unwrap(), 1);
    let ids: Vec<String> = store
        .list_sessions(100)
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert!(!ids.contains(&empty.to_string()));
    assert!(ids.contains(&with_msg.to_string()));
    assert!(ids.contains(&parent.to_string()));
    assert!(ids.contains(&child.to_string()));
}
