use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

fn ctx_at(dir: &std::path::Path) -> ToolContext {
    ToolContext::new(dir.to_path_buf(), Arc::new(DenyAll))
}

#[tokio::test]
async fn terminal_rendering_comes_back_inline() {
    let out = QrCodeTool
        .execute(
            json!({"text": "https://example.com"}),
            &ctx_at(std::path::Path::new(".")),
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let qr = v["qr"].as_str().unwrap();
    assert!(qr.lines().count() > 10, "looks like a QR block: {qr}");
}

#[tokio::test]
async fn svg_and_png_files_are_written_and_wellformed() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx_at(dir.path());

    QrCodeTool
        .execute(
            json!({"text": "hello", "output": "svg", "path": "qr.svg"}),
            &ctx,
        )
        .await
        .unwrap();
    let svg = std::fs::read_to_string(dir.path().join("qr.svg")).unwrap();
    assert!(svg.starts_with("<?xml") && svg.contains("<svg"), "{svg}");

    QrCodeTool
        .execute(
            json!({"text": "hello", "output": "png", "path": "qr.png"}),
            &ctx,
        )
        .await
        .unwrap();
    let png = std::fs::read(dir.path().join("qr.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG magic bytes");
}

#[tokio::test]
async fn missing_text_and_missing_path_error_clearly() {
    let ctx = ctx_at(std::path::Path::new("."));
    let e = QrCodeTool
        .execute(json!({}), &ctx)
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("`text`"), "{e}");

    let e = QrCodeTool
        .execute(json!({"text": "x", "output": "png"}), &ctx)
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("`path` is required"), "{e}");
}
