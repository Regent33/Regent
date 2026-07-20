//! Unit tests for `hooks` (extracted for the file-size rule; same module tree
//! via #[path] — `use super::*` still sees the parent).

use super::*;
use serde_json::json;
use tokio::sync::mpsc::unbounded_channel;

#[test]
fn escalation_hook_fires_only_on_agentic_trigger_tools() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let pending = Arc::new(AtomicBool::new(false));
    let hook = EscalationHook {
        pending: Arc::clone(&pending),
    };
    // Ordinary light-profile tools never escalate.
    for benign in ["memory_search", "current_time", "skill_view", "load_toolsx"] {
        hook.before_dispatch(benign, &json!({}));
        assert!(
            !pending.load(Ordering::Acquire),
            "{benign} must not escalate"
        );
    }
    for trigger in ["load_tools", "code_task", "delegate_task"] {
        pending.store(false, Ordering::Release);
        hook.before_dispatch(trigger, &json!({}));
        assert!(pending.load(Ordering::Acquire), "{trigger} escalates");
    }
}

#[tokio::test]
async fn delivery_emits_a_message_outbound_notification() {
    let (tx, mut rx) = unbounded_channel();
    let cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
    let _ = cell.set("sess_1".to_owned());
    let sink = NotificationDelivery {
        session_id: cell,
        out_tx: tx,
    };

    sink.deliver("", "build is green").await.unwrap();
    let line = rx.recv().await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["method"], "message.outbound");
    assert_eq!(v["params"]["target"], "home");
    assert_eq!(v["params"]["text"], "build is green");
    assert_eq!(v["params"]["session_id"], "sess_1");
}

#[test]
fn code_detail_discloses_only_bounded_edit_inputs() {
    let edit = code_detail(
        "file_edit",
        &json!({"path": "src/main.rs", "old_string": "old", "new_string": "new"}),
    )
    .unwrap();
    assert_eq!(edit["kind"], "replace");
    assert_eq!(edit["after"], "new");
    assert!(code_detail("terminal", &json!({"command": "echo secret"})).is_none());

    let huge = "x".repeat(CODE_DETAIL_MAX_CHARS + 10);
    let write = code_detail("write_file", &json!({"path": "x", "content": huge})).unwrap();
    assert!(write["after"].as_str().unwrap().ends_with("clipped …"));
}
