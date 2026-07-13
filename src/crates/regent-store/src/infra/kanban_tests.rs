//! Unit tests for `kanban` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn create_and_list_by_board_and_status() {
    let store = Store::open_in_memory().unwrap();
    store.create_task("t1", "alpha", "first", "").unwrap();
    store.create_task("t2", "alpha", "second", "").unwrap();
    store.create_task("t3", "beta", "other board", "").unwrap();

    assert_eq!(store.list_tasks("alpha", None).unwrap().len(), 2);
    assert_eq!(store.list_tasks("alpha", Some("todo")).unwrap().len(), 2);
    assert_eq!(
        store.list_tasks("beta", None).unwrap().len(),
        1,
        "boards are isolated"
    );
}

#[test]
fn claim_is_atomic_and_single_winner() {
    let store = Store::open_in_memory().unwrap();
    store.create_task("t1", "alpha", "task", "").unwrap();

    assert!(
        store.claim_task("t1", "worker-a").unwrap(),
        "first claim wins"
    );
    assert!(
        !store.claim_task("t1", "worker-b").unwrap(),
        "second claim loses the race"
    );

    let task = store.find_task("t1").unwrap().unwrap();
    assert_eq!(task.status, "in_progress");
    assert_eq!(task.assignee.as_deref(), Some("worker-a"));
}

#[test]
fn status_moves_and_filters() {
    let store = Store::open_in_memory().unwrap();
    store.create_task("t1", "alpha", "task", "").unwrap();
    store.claim_task("t1", "w").unwrap();
    assert!(store.set_task_status("t1", "done").unwrap());

    assert!(store.list_tasks("alpha", Some("todo")).unwrap().is_empty());
    assert_eq!(store.list_tasks("alpha", Some("done")).unwrap().len(), 1);
    assert!(
        !store.set_task_status("nope", "done").unwrap(),
        "missing task → false"
    );
}

#[test]
fn transition_enforces_the_from_column() {
    let store = Store::open_in_memory().unwrap();
    store.create_task("t1", "alpha", "task", "").unwrap();
    store.claim_task("t1", "w").unwrap(); // → in_progress

    // worker submits for review, reviewer approves
    assert!(
        store
            .transition_task("t1", "in_progress", "in_review")
            .unwrap()
    );
    assert!(store.transition_task("t1", "in_review", "done").unwrap());
    // approving again (no longer in_review) is rejected
    assert!(!store.transition_task("t1", "in_review", "done").unwrap());
    // a transition whose `from` doesn't match is a no-op
    assert!(!store.transition_task("t1", "todo", "in_progress").unwrap());
}
