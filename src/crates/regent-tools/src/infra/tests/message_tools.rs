//! Unit tests for `message_tools` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::NoDelivery;
use std::sync::Mutex;

/// Records what it was asked to deliver.
#[derive(Default)]
struct StubSink {
    sent: Mutex<Vec<(String, String)>>,
}
#[async_trait]
impl DeliverySink for StubSink {
    async fn deliver(&self, target: &str, text: &str) -> Result<(), RegentError> {
        self.sent
            .lock()
            .unwrap()
            .push((target.to_owned(), text.to_owned()));
        Ok(())
    }
    fn targets(&self) -> Vec<String> {
        vec!["telegram:home".to_owned()]
    }
}

fn ctx() -> ToolContext {
    ToolContext::new(
        std::path::PathBuf::from("."),
        Arc::new(crate::domain::contracts::DenyAll),
    )
}

#[tokio::test]
async fn delivers_text_to_the_named_target() {
    let sink = Arc::new(StubSink::default());
    let tool = SendMessageTool {
        sink: Arc::clone(&sink) as Arc<dyn DeliverySink>,
    };
    let out = tool
        .execute(
            json!({"text": "build is green", "target": "telegram:home"}),
            &ctx(),
        )
        .await
        .unwrap();
    assert!(out.contains("\"success\":true"));
    assert!(out.contains("telegram:home"));
    let sent = sink.sent.lock().unwrap();
    assert_eq!(
        sent.as_slice(),
        &[("telegram:home".to_owned(), "build is green".to_owned())]
    );
}

#[tokio::test]
async fn missing_or_empty_text_is_a_tool_error_not_a_send() {
    let sink = Arc::new(StubSink::default());
    let tool = SendMessageTool {
        sink: Arc::clone(&sink) as Arc<dyn DeliverySink>,
    };
    let out = tool.execute(json!({"target": "x"}), &ctx()).await.unwrap();
    assert!(out.contains("error"));
    assert!(sink.sent.lock().unwrap().is_empty(), "nothing was sent");
}

#[tokio::test]
async fn no_delivery_sink_declines_cleanly() {
    let tool = SendMessageTool {
        sink: Arc::new(NoDelivery),
    };
    let out = tool.execute(json!({"text": "hi"}), &ctx()).await.unwrap();
    assert!(out.contains("error"));
    assert!(out.contains("no delivery channels"));
}

#[test]
fn definition_lists_targets_for_the_model() {
    let def = send_message_definition(&["telegram:home".to_owned(), "discord:ops".to_owned()]);
    assert!(def.description.contains("telegram:home"));
    assert!(def.description.contains("discord:ops"));
}

#[test]
fn send_file_guard_allows_cwd_files_and_blocks_escapes_and_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path();
    std::fs::write(cwd.join("report.txt"), b"hi").unwrap();
    std::fs::write(cwd.join(".env"), b"SECRET=1").unwrap();

    // A normal file under cwd resolves.
    assert!(resolve_sendable("report.txt", cwd).is_ok());
    // A secret-named file inside cwd is blocked.
    assert!(
        resolve_sendable(".env", cwd)
            .unwrap_err()
            .contains("blocked")
    );
    // Traversal outside the allowed roots is refused (canonicalize + prefix).
    let outside = resolve_sendable("../../../../../../etc/passwd", cwd);
    assert!(outside.is_err());
    // A missing file is a clean error, not a panic.
    assert!(resolve_sendable("nope.txt", cwd).is_err());
}
