//! Unit tests for `video_analyze` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

#[test]
fn mime_by_extension() {
    assert_eq!(mime_for("a.webm"), "video/webm");
    assert_eq!(mime_for("clip.MOV"), "video/mov");
    assert_eq!(mime_for("x.mkv"), "video/mp4");
    assert_eq!(mime_for("noext"), "video/mp4");
}

#[tokio::test]
async fn resolves_a_local_video_and_missing_param_errors() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("v.mp4"), b"\x00\x00\x00fake")
        .await
        .unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let (mime, _bytes) = resolve_video("v.mp4", &ctx).await.unwrap();
    assert_eq!(mime, "video/mp4");

    let out = VideoAnalyzeTool.execute(json!({}), &ctx).await.unwrap();
    assert!(out.contains("missing required parameter"), "got: {out}");
}
