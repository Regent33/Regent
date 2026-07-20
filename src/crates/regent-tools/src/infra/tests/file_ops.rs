//! Unit tests for `file_ops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::{ApprovalHandler, DenyAll};
use std::sync::Arc;

struct AllowAll;
#[async_trait]
impl ApprovalHandler for AllowAll {
    async fn request(&self, _: &str, _: &str, _: &str) -> ApprovalDecision {
        ApprovalDecision::Approve
    }
}

fn ctx(dir: &Path, approval: Arc<dyn ApprovalHandler>) -> ToolContext {
    ToolContext::new(dir.to_path_buf(), approval)
}

#[tokio::test]
async fn copy_then_move_then_delete_with_approval() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), b"hi").unwrap();
    let ctx = ctx(dir.path(), Arc::new(AllowAll));

    let copied = CopyFileTool
        .execute(json!({"source": "a.txt", "destination": "sub/b.txt"}), &ctx)
        .await
        .unwrap();
    assert!(copied.contains("\"success\":true"));
    assert!(dir.path().join("sub/b.txt").exists());

    let moved = MoveFileTool
        .execute(json!({"source": "sub/b.txt", "destination": "c.txt"}), &ctx)
        .await
        .unwrap();
    assert!(moved.contains("\"success\":true"));
    assert!(dir.path().join("c.txt").exists() && !dir.path().join("sub/b.txt").exists());

    let deleted =
        DeleteFileTool.execute(json!({"path": "c.txt"}), &ctx).await.unwrap();
    assert!(deleted.contains("\"success\":true"));
    assert!(!dir.path().join("c.txt").exists());
}

#[tokio::test]
async fn denied_approval_does_not_touch_disk() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("keep.txt"), b"x").unwrap();
    let out = DeleteFileTool
        .execute(json!({"path": "keep.txt"}), &ctx(dir.path(), Arc::new(DenyAll)))
        .await
        .unwrap();
    assert!(out.contains("denied by approval"));
    assert!(dir.path().join("keep.txt").exists(), "file must survive a denial");
}
