//! Board dispatcher behavior across the three review policies. Public-API
//! integration tests (stub runner/reviewer, in-memory store) — kept out of the
//! source module so `board/` files stay within the line budget.

use async_trait::async_trait;
use regent_agent::{BoardDispatcher, ReviewVerdict, Reviewer, TaskRunner};
use regent_kernel::RegentError;
use regent_store::{KanbanTaskRow, ReviewPolicy, Store};
use std::sync::Arc;

/// Succeeds for every task except one titled "boom".
struct StubRunner;
#[async_trait]
impl TaskRunner for StubRunner {
    async fn run(&self, task: &KanbanTaskRow) -> Result<String, RegentError> {
        if task.title == "boom" {
            Err(RegentError::Tool {
                tool: "stub".into(),
                message: "kaboom".into(),
            })
        } else {
            Ok(format!("did {}", task.title))
        }
    }
}

/// Approves everything except tasks titled "reject-me".
struct StubReviewer;
#[async_trait]
impl Reviewer for StubReviewer {
    async fn review(
        &self,
        task: &KanbanTaskRow,
        _work: &str,
    ) -> Result<ReviewVerdict, RegentError> {
        if task.title == "reject-me" {
            Ok(ReviewVerdict::Reject("needs work".into()))
        } else {
            Ok(ReviewVerdict::Approve)
        }
    }
}

fn dispatcher() -> (Arc<Store>, BoardDispatcher) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let dispatcher = BoardDispatcher::new(Arc::clone(&store), Arc::new(StubRunner), "worker-1");
    (store, dispatcher)
}

fn reviewing_dispatcher() -> (Arc<Store>, BoardDispatcher) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let dispatcher = BoardDispatcher::new(Arc::clone(&store), Arc::new(StubRunner), "worker-1")
        .with_reviewer(Arc::new(StubReviewer));
    (store, dispatcher)
}

#[tokio::test]
async fn success_submits_for_review_and_assigns_worker() {
    let (store, dispatcher) = dispatcher();
    store.create_task("t1", "alpha", "ship", "").unwrap();

    let outcome = dispatcher.dispatch_once("alpha").await.unwrap().unwrap();
    assert_eq!(
        outcome.status, "in_review",
        "clean run awaits review, not auto-done"
    );
    assert_eq!(outcome.summary, "did ship");
    let task = store.find_task("t1").unwrap().unwrap();
    assert_eq!(task.status, "in_review");
    assert_eq!(task.assignee.as_deref(), Some("worker-1"));
    // The dispatcher never marks done itself — a reviewer does.
    assert!(store.list_tasks("alpha", Some("done")).unwrap().is_empty());
}

#[tokio::test]
async fn failure_auto_blocks() {
    let (store, dispatcher) = dispatcher();
    store.create_task("t1", "alpha", "boom", "").unwrap();

    let outcome = dispatcher.dispatch_once("alpha").await.unwrap().unwrap();
    assert_eq!(outcome.status, "blocked");
    assert!(outcome.summary.contains("kaboom"));
    assert_eq!(store.find_task("t1").unwrap().unwrap().status, "blocked");
}

#[tokio::test]
async fn empty_board_and_completed_tasks_dispatch_nothing() {
    let (store, dispatcher) = dispatcher();
    assert!(dispatcher.dispatch_once("alpha").await.unwrap().is_none());

    store.create_task("t1", "alpha", "one", "").unwrap();
    dispatcher.dispatch_once("alpha").await.unwrap(); // → in_review
    // Nothing left in `todo` — a claimed/reviewed task isn't re-dispatched.
    assert!(dispatcher.dispatch_once("alpha").await.unwrap().is_none());
}

#[tokio::test]
async fn dispatch_pending_drains_up_to_the_per_tick_cap() {
    let (store, dispatcher) = dispatcher();
    for i in 0..5 {
        store
            .create_task(&format!("t{i}"), "alpha", "ship", "")
            .unwrap();
    }

    // Cap of 3 → exactly 3 dispatched this tick, two left in `todo`.
    let outcomes = dispatcher.dispatch_pending("alpha", 3).await.unwrap();
    assert_eq!(outcomes.len(), 3);
    assert_eq!(store.list_tasks("alpha", Some("todo")).unwrap().len(), 2);

    // Next tick drains the rest and stops when the board runs dry.
    let outcomes = dispatcher.dispatch_pending("alpha", 10).await.unwrap();
    assert_eq!(outcomes.len(), 2, "stops early once nothing is claimable");
    assert!(store.list_tasks("alpha", Some("todo")).unwrap().is_empty());
}

#[tokio::test]
async fn auto_policy_self_approves_to_done() {
    let (store, dispatcher) = dispatcher();
    store
        .set_board_policy("alpha", ReviewPolicy::Auto, None)
        .unwrap();
    store.create_task("t1", "alpha", "ship", "").unwrap();

    let outcome = dispatcher.dispatch_once("alpha").await.unwrap().unwrap();
    assert_eq!(outcome.status, "done", "auto boards skip the review gate");
    assert_eq!(store.find_task("t1").unwrap().unwrap().status, "done");
}

#[tokio::test]
async fn agent_policy_approves_to_done() {
    let (store, dispatcher) = reviewing_dispatcher();
    store
        .set_board_policy("alpha", ReviewPolicy::Agent, Some("rev"))
        .unwrap();
    store.create_task("t1", "alpha", "ship", "").unwrap();

    let outcome = dispatcher.dispatch_once("alpha").await.unwrap().unwrap();
    assert_eq!(outcome.status, "done");
    assert_eq!(store.find_task("t1").unwrap().unwrap().status, "done");
}

#[tokio::test]
async fn agent_policy_reject_sends_back_to_in_progress() {
    let (store, dispatcher) = reviewing_dispatcher();
    store
        .set_board_policy("alpha", ReviewPolicy::Agent, Some("rev"))
        .unwrap();
    store.create_task("t1", "alpha", "reject-me", "").unwrap();

    let outcome = dispatcher.dispatch_once("alpha").await.unwrap().unwrap();
    assert_eq!(
        outcome.status, "in_progress",
        "rejection sends it back for rework"
    );
    assert!(outcome.summary.contains("needs work"));
    assert_eq!(
        store.find_task("t1").unwrap().unwrap().status,
        "in_progress"
    );
    // Not re-dispatched (only `todo` is claimable) — no retry storm.
    assert!(dispatcher.dispatch_once("alpha").await.unwrap().is_none());
}

#[tokio::test]
async fn agent_policy_without_reviewer_falls_back_to_human() {
    // Policy says `agent`, but no reviewer is wired → hold for a human.
    let (store, dispatcher) = dispatcher();
    store
        .set_board_policy("alpha", ReviewPolicy::Agent, Some("rev"))
        .unwrap();
    store.create_task("t1", "alpha", "ship", "").unwrap();

    let outcome = dispatcher.dispatch_once("alpha").await.unwrap().unwrap();
    assert_eq!(
        outcome.status, "in_review",
        "fail-safe: never auto-completes"
    );
    assert_eq!(store.find_task("t1").unwrap().unwrap().status, "in_review");
}
