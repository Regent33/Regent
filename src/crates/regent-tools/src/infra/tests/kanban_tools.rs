//! Unit tests for `kanban_tools` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn store() -> Arc<Store> {
    Arc::new(Store::open_in_memory().unwrap())
}

fn id_of(create_result: &str) -> String {
    serde_json::from_str::<Value>(create_result).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn create_claim_submit_approve_review_flow() {
    let store = store();
    let created = run_kanban_action(
        &store,
        "alpha",
        "worker-1",
        &json!({"action": "create", "title": "ship it", "description": "the thing"}),
    );
    let id = id_of(&created);

    let listed = run_kanban_action(&store, "alpha", "worker-1", &json!({"action": "list"}));
    let v: Value = serde_json::from_str(&listed).unwrap();
    assert_eq!(v["count"], 1);
    assert_eq!(v["tasks"][0]["status"], "todo");

    let claimed = run_kanban_action(
        &store,
        "alpha",
        "worker-1",
        &json!({"action": "claim", "id": id}),
    );
    assert!(claimed.contains("\"claimed\":true"));

    // Finished work goes to review first — not straight to done.
    let submitted = run_kanban_action(
        &store,
        "alpha",
        "worker-1",
        &json!({"action": "submit", "id": id}),
    );
    assert!(submitted.contains("\"status\":\"in_review\""));
    assert!(
        run_kanban_action(
            &store,
            "alpha",
            "worker-1",
            &json!({"action": "list", "status": "done"})
        )
        .contains("\"count\":0")
    );

    // Reviewer approves → done.
    let approved = run_kanban_action(
        &store,
        "alpha",
        "reviewer",
        &json!({"action": "approve", "id": id}),
    );
    assert!(approved.contains("\"status\":\"done\""));
    assert!(
        run_kanban_action(
            &store,
            "alpha",
            "worker-1",
            &json!({"action": "list", "status": "todo"})
        )
        .contains("\"count\":0")
    );
}

#[test]
fn approve_requires_review_and_reject_sends_back() {
    let store = store();
    let id = id_of(&run_kanban_action(
        &store,
        "alpha",
        "w1",
        &json!({"action": "create", "title": "t"}),
    ));
    run_kanban_action(&store, "alpha", "w1", &json!({"action": "claim", "id": id}));

    // Can't approve straight from in_progress — review is mandatory.
    let premature = run_kanban_action(
        &store,
        "alpha",
        "rev",
        &json!({"action": "approve", "id": id}),
    );
    assert!(premature.contains("\"success\":false"));

    run_kanban_action(
        &store,
        "alpha",
        "w1",
        &json!({"action": "submit", "id": id}),
    );
    // Reviewer rejects → back to in_progress for rework.
    let rejected = run_kanban_action(
        &store,
        "alpha",
        "rev",
        &json!({"action": "reject", "id": id}),
    );
    assert!(rejected.contains("\"status\":\"in_progress\""));
    assert!(
        run_kanban_action(
            &store,
            "alpha",
            "w1",
            &json!({"action": "list", "status": "in_progress"})
        )
        .contains("\"count\":1")
    );
}

#[test]
fn claim_is_single_winner_through_the_tool() {
    let store = store();
    let id = id_of(&run_kanban_action(
        &store,
        "alpha",
        "w1",
        &json!({"action": "create", "title": "t"}),
    ));
    let first = run_kanban_action(&store, "alpha", "w1", &json!({"action": "claim", "id": id}));
    let second = run_kanban_action(&store, "alpha", "w2", &json!({"action": "claim", "id": id}));
    assert!(first.contains("\"claimed\":true"));
    assert!(second.contains("\"claimed\":false"));
}

#[test]
fn bad_input_is_a_tool_error() {
    let store = store();
    assert!(run_kanban_action(&store, "a", "w", &json!({"action": "create"})).contains("error"));
    assert!(run_kanban_action(&store, "a", "w", &json!({"action": "nope"})).contains("error"));
}
