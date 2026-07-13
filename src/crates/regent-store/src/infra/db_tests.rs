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
