//! Unit tests for `checkpoint` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn store() -> (tempfile::TempDir, CheckpointStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = CheckpointStore::new(dir.path().join("checkpoints"));
    (dir, store)
}

#[test]
fn rollback_restores_modified_content() {
    let (dir, store) = store();
    let file = dir.path().join("notes.txt");
    fs::write(&file, "original").unwrap();

    let id = store
        .snapshot("edit notes", std::slice::from_ref(&file))
        .unwrap();
    fs::write(&file, "clobbered").unwrap();
    store.rollback(&id).unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), "original");
}

#[test]
fn rollback_deletes_a_file_that_did_not_exist_at_snapshot() {
    let (dir, store) = store();
    let file = dir.path().join("new.txt");

    let id = store
        .snapshot("create file", std::slice::from_ref(&file))
        .unwrap();
    fs::write(&file, "freshly created").unwrap();
    store.rollback(&id).unwrap();

    assert!(
        !file.exists(),
        "a file created after the snapshot is removed on rollback"
    );
}

#[test]
fn list_reports_checkpoints_and_unknown_rollback_errors() {
    let (dir, store) = store();
    let file = dir.path().join("a.txt");
    fs::write(&file, "x").unwrap();
    store.snapshot("first", &[file]).unwrap();

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].label, "first");
    assert_eq!(listed[0].file_count, 1);

    assert!(store.rollback("ckpt_does_not_exist").is_err());
}
