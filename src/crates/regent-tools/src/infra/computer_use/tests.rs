use super::*;
use crate::domain::contracts::{ApprovalDecision, ApprovalHandler, DenyAll};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

struct MockBackend {
    last: Mutex<Option<Action>>,
}
#[async_trait]
impl ComputerBackend for MockBackend {
    async fn act(&self, action: &Action) -> Result<ActOutput, RegentError> {
        *self.last.lock().unwrap() = Some(action.clone());
        Ok(ActOutput {
            note: "mock".into(),
            image_path: None,
        })
    }
}

fn ctx(approval: Arc<dyn ApprovalHandler>) -> ToolContext {
    ToolContext::new(std::env::temp_dir(), approval)
}

#[test]
fn on_path_rejects_a_missing_binary_and_finds_a_real_one() {
    assert!(!on_path("definitely-not-a-real-binary-xyz"));
    // An explicit path is checked directly, not searched.
    assert!(!on_path("C:/definitely/not/here/cua-driver.exe"));
    assert!(on_path(if cfg!(windows) { "cmd" } else { "sh" }));
}

#[test]
fn parses_each_action() {
    assert_eq!(
        parse_action(&json!({"action": "screenshot"})).unwrap(),
        Action::Screenshot
    );
    assert_eq!(
        parse_action(&json!({"action": "click", "x": 10, "y": 20})).unwrap(),
        Action::Click { x: 10, y: 20 }
    );
    assert!(
        parse_action(&json!({"action": "click"})).is_err(),
        "click needs x/y"
    );
    assert!(parse_action(&json!({"action": "bogus"})).is_err());
}

// One env-touching test (REGENT_COMPUTER_USE is process-global — keeping it in a
// single test avoids a parallel-test race on the variable).
#[tokio::test]
async fn feature_flag_then_approval_gating() {
    unsafe { std::env::remove_var("REGENT_COMPUTER_USE") };
    let tool = ComputerUseTool::new(Arc::new(MockBackend {
        last: Mutex::new(None),
    }));
    let out = tool
        .execute(json!({"action": "screenshot"}), &ctx(Arc::new(DenyAll)))
        .await
        .unwrap();
    assert!(out.contains("REGENT_COMPUTER_USE"), "disabled: {out}");

    unsafe { std::env::set_var("REGENT_COMPUTER_USE", "1") };
    let out = tool
        .execute(json!({"action": "screenshot"}), &ctx(Arc::new(DenyAll)))
        .await
        .unwrap();
    assert!(out.contains("\"ok\":true"), "screenshot ungated: {out}");

    struct Rec(AtomicBool);
    #[async_trait]
    impl ApprovalHandler for Rec {
        async fn request(&self, _: &str, _: &str, _: &str) -> ApprovalDecision {
            self.0.store(true, Ordering::SeqCst);
            ApprovalDecision::Deny
        }
    }
    let rec = Arc::new(Rec(AtomicBool::new(false)));
    let out = tool
        .execute(
            json!({"action": "click", "x": 1, "y": 2}),
            &ctx(rec.clone()),
        )
        .await
        .unwrap();
    assert!(out.contains("denied by approval"), "click gated: {out}");
    assert!(rec.0.load(Ordering::SeqCst), "approval gate consulted");
    unsafe { std::env::remove_var("REGENT_COMPUTER_USE") };
}
