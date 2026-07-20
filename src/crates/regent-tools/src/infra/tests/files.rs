//! Unit tests for `files` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

fn ctx(dir: &std::path::Path) -> ToolContext {
    ToolContext::new(dir.to_path_buf(), Arc::new(DenyAll))
}

#[tokio::test]
async fn write_then_read_round_trip_with_line_range() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path());
    let written = WriteFileTool
        .execute(
            json!({"path": "notes/a.txt", "content": "one\ntwo\nthree"}),
            &ctx,
        )
        .await
        .unwrap();
    assert!(written.contains("bytes_written"));

    let read = ReadFileTool
        .execute(
            json!({"path": "notes/a.txt", "offset": 2, "limit": 1}),
            &ctx,
        )
        .await
        .unwrap();
    let value: Value = serde_json::from_str(&read).unwrap();
    assert_eq!(value["content"], "two");
    assert_eq!(value["truncated"], false);
}

#[tokio::test]
async fn missing_file_is_error_json() {
    let dir = tempfile::tempdir().unwrap();
    let out = ReadFileTool
        .execute(json!({"path": "nope.txt"}), &ctx(dir.path()))
        .await
        .unwrap();
    assert!(out.contains("error"));
}

#[tokio::test]
async fn sandboxed_write_outside_root_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let jailed = ToolContext::new_sandboxed(
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        Arc::new(DenyAll),
    );
    let out = WriteFileTool
        .execute(json!({"path": "../escape.txt", "content": "x"}), &jailed)
        .await
        .unwrap();
    assert!(out.contains("escapes the sandbox"), "got: {out}");
    // And the file must not have been created outside the jail.
    assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
}
