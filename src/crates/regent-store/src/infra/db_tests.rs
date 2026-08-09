use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A reader must not queue behind a held write transaction (WAL + the
/// dedicated read connection). Guards the P2-003 fix.
#[test]
fn read_does_not_block_behind_held_write() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("test.db")).unwrap());

    let writing = Arc::new(AtomicBool::new(false));
    let writer = {
        let (store, writing) = (Arc::clone(&store), Arc::clone(&writing));
        std::thread::spawn(move || {
            store
                .with_write(|tx| {
                    tx.execute("UPDATE persona SET content = 'busy' WHERE key = 'soul'", [])?;
                    writing.store(true, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(500));
                    Ok(())
                })
                .unwrap();
        })
    };

    let wait_started = std::time::Instant::now();
    while !writing.load(Ordering::SeqCst) {
        assert!(
            wait_started.elapsed() < Duration::from_secs(5),
            "writer never started (did the UPDATE fail?)"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = std::time::Instant::now();
    let count: i64 = store
        .with_read(|conn| conn.query_row("SELECT count(*) FROM persona", [], |r| r.get(0)))
        .unwrap();
    assert!(count >= 1);
    assert!(
        started.elapsed() < Duration::from_millis(300),
        "read waited {:?} behind the write transaction",
        started.elapsed()
    );
    writer.join().unwrap();
}

/// v8 → v9 retro-stamp: a `background_task` child born with source `deacon`
/// stops posing as a user chat, while a real chat that merely QUOTES the
/// wrapper phrase mid-message is left alone. Runs the statement directly —
/// building a genuine v8 database would mean shipping a fixture of the old
/// schema, and the statement IS the migration.
#[test]
fn v9_retro_stamps_background_children_without_touching_real_chats() {
    use regent_kernel::{ChatMessage, SessionId};

    let store = Store::open_in_memory().unwrap();
    let child = SessionId::generate();
    let chat = SessionId::generate();
    let quoter = SessionId::generate();
    for id in [&child, &chat, &quoter] {
        store
            .create_session(id, "deacon", None, None, None)
            .unwrap();
    }
    store
        .append_message(
            &child,
            &ChatMessage::user(
                "[Background job — no user is present to answer questions; work autonomously.]\n\nbuild it",
            ),
            None,
            None,
        )
        .unwrap();
    store
        .append_message(&chat, &ChatMessage::user("what does this do?"), None, None)
        .unwrap();
    // The phrase, but not at the start — a person pasting a log, not a job.
    store
        .append_message(
            &quoter,
            &ChatMessage::user("it printed [Background job — no user is present…] and hung"),
            None,
            None,
        )
        .unwrap();

    store
        .with_write(|tx| tx.execute(MIGRATE_V9_BACKGROUND_SOURCE, []))
        .unwrap();

    let source = |id: &SessionId| -> String {
        store
            .with_read(|conn| {
                conn.query_row(
                    "SELECT source FROM sessions WHERE id = ?1",
                    [id.to_string()],
                    |r| r.get(0),
                )
            })
            .unwrap()
    };
    assert_eq!(
        source(&child),
        "background",
        "the job stops posing as a chat"
    );
    assert_eq!(source(&chat), "deacon", "a real chat is untouched");
    assert_eq!(source(&quoter), "deacon", "quoting the phrase is not a job");

    // Idempotent: a re-run (interrupted upgrade, repeated open) changes nothing.
    let changed = store
        .with_write(|tx| tx.execute(MIGRATE_V9_BACKGROUND_SOURCE, []))
        .unwrap();
    assert_eq!(changed, 0);
}

#[test]
fn v11_marks_historical_usage_unverified_without_inventing_missing_call_counts() {
    use crate::infra::schema::MIGRATE_V11_USAGE_GAPS;
    use regent_kernel::SessionId;

    let store = Store::open_in_memory().unwrap();
    let session = SessionId::generate();
    store
        .create_session(&session, "deacon", None, None, None)
        .unwrap();
    store.record_usage(&session, 100, 20, true).unwrap();
    store.record_usage(&session, 50, 10, true).unwrap();

    let changed = store
        .with_write(|tx| tx.execute(MIGRATE_V11_USAGE_GAPS, []))
        .unwrap();
    assert_eq!(changed, 1);
    let insights = store.insights().unwrap();
    assert!(insights.legacy_usage_unverified);
    assert_eq!(insights.unreported_usage_calls, 0);

    let changed = store
        .with_write(|tx| tx.execute(MIGRATE_V11_USAGE_GAPS, []))
        .unwrap();
    assert_eq!(changed, 0);
    let insights = store.insights().unwrap();
    assert!(insights.legacy_usage_unverified);
    assert_eq!(insights.unreported_usage_calls, 0);
}
