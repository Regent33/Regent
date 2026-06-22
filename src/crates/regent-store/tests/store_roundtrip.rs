//! E2E store behavior against a real on-disk database (temp dir) — the
//! lesson: exercise the real path, mocks hide integration bugs.

use regent_kernel::{ChatMessage, Role, SessionId, ToolCall};
use regent_store::Store;

fn sample_tool_call() -> ToolCall {
    ToolCall {
        id: "call_1".into(),
        name: "terminal".into(),
        arguments: r#"{"command":"echo hi"}"#.into(),
    }
}

#[test]
fn conversation_round_trip_preserves_messages() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.db")).unwrap();
    let session = SessionId::generate();
    store
        .create_session(&session, "cli", Some("test-model"), Some("system text"), None)
        .unwrap();

    store.append_message(&session, &ChatMessage::user("run echo"), None, None).unwrap();
    let assistant = ChatMessage::assistant(None, vec![sample_tool_call()]);
    store.append_message(&session, &assistant, Some(12), Some("tool_calls")).unwrap();
    let tool = ChatMessage::tool_result("call_1", "terminal", r#"{"stdout":"hi"}"#);
    store.append_message(&session, &tool, None, None).unwrap();

    let rows = store.get_conversation(&session).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].message.role, Role::User);
    assert_eq!(rows[1].message.tool_calls, vec![sample_tool_call()]);
    assert_eq!(rows[1].finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(rows[2].message.tool_call_id.as_deref(), Some("call_1"));

    store.record_usage(&session, 100, 25).unwrap();
    store.end_session(&session, "user_exit").unwrap();

    // v2 surfaces: frozen prompt read-back + turn record
    assert_eq!(
        store.session_system_prompt(&session).unwrap().as_deref(),
        Some("system text")
    );
    store
        .record_turn(&session, "test-model", 2, "ok", None, regent_store::now_epoch())
        .unwrap();
}

#[test]
fn insights_rolls_up_usage_across_sessions() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.insights().unwrap(), regent_store::InsightsRollup::default());

    let a = SessionId::generate();
    let b = SessionId::generate();
    store.create_session(&a, "cli", Some("m"), None, None).unwrap();
    store.create_session(&b, "cli", Some("m"), None, None).unwrap();
    store.append_message(&a, &ChatMessage::user("hi"), None, None).unwrap();
    store.record_usage(&a, 100, 25).unwrap(); // also bumps api_call_count
    store.record_usage(&b, 40, 10).unwrap();
    store.record_turn(&a, "m", 2, "ok", None, 1.0).unwrap();
    store.record_turn(&b, "m", 1, "error", Some("boom"), 2.0).unwrap();

    let r = store.insights().unwrap();
    assert_eq!(r.sessions, 2);
    assert_eq!(r.input_tokens, 140);
    assert_eq!(r.output_tokens, 35);
    assert_eq!(r.api_calls, 2);
    assert_eq!(r.messages, 1);
    assert_eq!(r.turns, 2);
    assert_eq!(r.turns_ok, 1);
}

#[test]
fn v1_database_is_reconciled_to_v2_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    // Simulate a v1 database: sessions without system_prompt, no turns table.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (
                 id TEXT PRIMARY KEY, source TEXT NOT NULL, model TEXT,
                 parent_session_id TEXT, started_at REAL NOT NULL, ended_at REAL,
                 end_reason TEXT, title TEXT, message_count INTEGER NOT NULL DEFAULT 0,
                 input_tokens INTEGER NOT NULL DEFAULT 0,
                 output_tokens INTEGER NOT NULL DEFAULT 0,
                 api_call_count INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version (version) VALUES (1);
             INSERT INTO sessions (id, source, started_at) VALUES ('sess_old', 'cli', 1.0);",
        )
        .unwrap();
    }

    let store = Store::open(&path).unwrap();
    let old = SessionId::from_string("sess_old");
    // Old rows survive with a NULL prompt; new v2 APIs work immediately.
    assert_eq!(store.session_system_prompt(&old).unwrap(), None);
    store.record_turn(&old, "m", 1, "ok", None, 2.0).unwrap();
}

#[test]
fn unknown_session_is_a_typed_error() {
    let store = Store::open_in_memory().unwrap();
    let missing = SessionId::from_string("sess_missing");
    assert!(matches!(
        store.get_conversation(&missing),
        Err(regent_store::StoreError::UnknownSession(_))
    ));
}

#[test]
fn fts_search_finds_content_and_tool_names() {
    let store = Store::open_in_memory().unwrap();
    let session = SessionId::generate();
    store.create_session(&session, "cli", None, None, None).unwrap();
    store
        .append_message(&session, &ChatMessage::user("deploy the docker container"), None, None)
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
