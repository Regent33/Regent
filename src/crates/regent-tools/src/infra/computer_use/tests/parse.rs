use super::*;
use serde_json::json;

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
    assert!(parse_action(&json!({"action": "click"})).is_err());
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
            "action": "close_tab", "window_id": 42, "target": "Regent issue"
        }))
        .unwrap(),
        Action::CloseTab {
            window_id: 42,
            target: "Regent issue".into()
        }
    );
    assert_eq!(
        parse_action(&json!({
            "action": "select_tab", "window_id": 42, "target": "Regent issue"
        }))
        .unwrap(),
        Action::SelectTab {
            window_id: 42,
            target: "Regent issue".into()
        }
    );
    assert!(parse_action(&json!({"action": "close_window"})).is_err());
    assert!(parse_action(&json!({"action": "close_tab", "window_id": 42})).is_err());
    assert!(parse_action(&json!({"action": "select_tab", "window_id": 42})).is_err());
}
