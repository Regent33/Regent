//! Shared helpers for the create_document test modules (tests/round_trip.rs,
//! tests/images.rs, tests/edit.rs) — all included from mod.rs via #[path].

use super::*;
use crate::domain::contracts::DenyAll;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Runs `create_document` with `args` and returns the created file path.
pub async fn create(dir: &TempDir, args: Value) -> PathBuf {
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool.execute(args, &ctx).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("error").is_none(), "tool returned an error: {v}");
    assert!(v["bytes"].as_u64().unwrap() > 0, "empty file: {v}");
    PathBuf::from(v["created"].as_str().unwrap())
}

/// Reads one entry out of a ZIP-backed document into a string.
pub fn zip_entry(path: &PathBuf, entry: &str) -> String {
    let file = std::fs::File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut out = String::new();
    archive
        .by_name(entry)
        .unwrap_or_else(|_| panic!("missing zip part {entry}"))
        .read_to_string(&mut out)
        .unwrap();
    out
}

pub fn zip_names(path: &PathBuf) -> Vec<String> {
    let file = std::fs::File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_owned())
        .collect()
}
