//! Unit tests for `vision_analyze` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00";

#[test]
fn sniffs_known_image_magic_bytes() {
    assert_eq!(sniff_mime(PNG_SIG).as_deref(), Some("image/png"));
    assert_eq!(
        sniff_mime(&[0xff, 0xd8, 0xff, 0x00]).as_deref(),
        Some("image/jpeg")
    );
    assert_eq!(sniff_mime(b"GIF89a...").as_deref(), Some("image/gif"));
    assert_eq!(
        sniff_mime(b"RIFF1234WEBPxxxx").as_deref(),
        Some("image/webp")
    );
    assert_eq!(sniff_mime(b"not an image"), None);
}

#[tokio::test]
async fn resolves_a_data_url() {
    let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll));
    let data_url = format!("data:image/png;base64,{}", B64.encode(PNG_SIG));
    let (mime, bytes) = resolve_image(&data_url, &ctx).await.unwrap();
    assert_eq!(mime, "image/png");
    assert_eq!(bytes, PNG_SIG);
}

#[tokio::test]
async fn resolves_a_local_image_file() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("a.png"), PNG_SIG)
        .await
        .unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let (mime, _bytes) = resolve_image("a.png", &ctx).await.unwrap();
    assert_eq!(mime, "image/png");
}

#[tokio::test]
async fn non_image_file_is_rejected_before_any_network_call() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("a.txt"), "hello")
        .await
        .unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = VisionAnalyzeTool
        .execute(json!({"image_url": "a.txt", "question": "?"}), &ctx)
        .await
        .unwrap();
    assert!(out.contains("not a recognized image"), "got: {out}");
}
