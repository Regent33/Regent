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
    assert_eq!(
        parse_action(&json!({"action": "list_windows"})).unwrap(),
        Action::ListWindows
    );
    assert_eq!(
        parse_action(&json!({"action": "focus_window", "window_id": 42})).unwrap(),
        Action::FocusWindow { window_id: 42 }
    );
    assert_eq!(
        parse_action(&json!({"action": "close_window", "window_id": 42})).unwrap(),
        Action::CloseWindow { window_id: 42 }
    );
    assert_eq!(
        parse_action(&json!({"action": "list_tabs", "window_id": 42})).unwrap(),
        Action::ListTabs { window_id: 42 }
    );
    assert_eq!(
        parse_action(&json!({
            "action": "close_tab",
            "window_id": 42,
            "target": "Regent issue"
        }))
        .unwrap(),
        Action::CloseTab {
            window_id: 42,
            target: "Regent issue".into()
        }
    );
    assert!(parse_action(&json!({"action": "close_window"})).is_err());
    assert!(parse_action(&json!({"action": "close_tab", "window_id": 42})).is_err());
}

#[test]
fn screen_and_target_safety_are_explicit_in_the_schema() {
    let def = definition();
    assert!(def.description.contains("screen in ONE call"));
    assert!(def.description.contains("NEVER use blind alt+f4"));
    assert!(def.description.contains("NEVER ask permission"));
    let actions = def.parameters["properties"]["action"]["enum"]
        .as_array()
        .unwrap();
    for action in [
        "screenshot",
        "list_windows",
        "close_window",
        "list_tabs",
        "close_tab",
    ] {
        assert!(actions.iter().any(|value| value == action), "{action}");
    }
}

#[test]
fn observation_is_ungated_but_target_changes_are_mutating() {
    assert!(!Action::Screenshot.is_mutating());
    assert!(!Action::ListWindows.is_mutating());
    assert!(!Action::ListTabs { window_id: 42 }.is_mutating());
    assert!(Action::FocusWindow { window_id: 42 }.is_mutating());
    assert!(Action::CloseWindow { window_id: 42 }.is_mutating());
    assert!(
        Action::CloseTab {
            window_id: 42,
            target: "docs".into()
        }
        .is_mutating()
    );
}

#[test]
fn blind_close_shortcuts_are_blocked_before_the_focused_app_can_receive_them() {
    for combo in [
        "alt+f4",
        " ALT + F4 ",
        "ctrl+w",
        "control+w",
        "cmd+w",
        "cmd+q",
    ] {
        assert!(is_blind_close_combo(combo), "{combo}");
    }
    for combo in ["ctrl+s", "ctrl+t", "alt+tab", "enter"] {
        assert!(!is_blind_close_combo(combo), "{combo}");
    }
}

#[test]
fn nested_vision_errors_fail_the_combined_screen_action() {
    assert!(!vision_succeeded(
        &json!({"error": "vision route unavailable"})
    ));
    assert!(!vision_succeeded(&json!({"success": false})));
    assert!(vision_succeeded(
        &json!({"success": true, "analysis": "screen"})
    ));
    assert!(vision_succeeded(&Value::Null));
}

// One env-touching test (REGENT_COMPUTER_USE is process-global — keeping it in a
// single test avoids a parallel-test race on the variable).
fn catalog_has_computer_use() -> bool {
    crate::application::registry::core_catalog_from_env()
        .unwrap()
        .definitions()
        .iter()
        .any(|definition| definition.name == "computer_use")
}

#[tokio::test]
async fn feature_flag_then_approval_gating() {
    unsafe { std::env::remove_var("REGENT_COMPUTER_USE") };
    // Registration is gated on the same flag as execution: with the flag unset
    // the tool is not even in the catalog — this is exactly the gateway bug
    // (the flag never reached the process, so chat had no computer_use).
    assert!(
        !catalog_has_computer_use(),
        "computer_use must be absent when the flag is unset"
    );
    let tool = ComputerUseTool::new(Arc::new(MockBackend {
        last: Mutex::new(None),
    }));
    let out = tool
        .execute(json!({"action": "screenshot"}), &ctx(Arc::new(DenyAll)))
        .await
        .unwrap();
    assert!(out.contains("REGENT_COMPUTER_USE"), "disabled: {out}");

    unsafe { std::env::set_var("REGENT_COMPUTER_USE", "1") };
    assert!(
        catalog_has_computer_use(),
        "computer_use must be registered once the flag is on"
    );
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
