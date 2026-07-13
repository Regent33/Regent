//! Artifacts viewer (M10): read-only listing and fetch of files under
//! `$REGENT_HOME/artifacts/<slug>/…` for the desktop Artifacts window. The
//! webview has no filesystem access, so `artifacts.get` inlines small text and
//! images (base64 data URI) and otherwise returns an absolute path the UI opens
//! externally. Both methods are additive and never write; traversal is gated by
//! the same canonicalized within-root check as `attachment.put`.

use super::Dispatcher;
use super::attachment_ops::attachment_within_root;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Inline text only up to this size (256 KB); larger files are opened externally.
const MAX_TEXT_BYTES: u64 = 256 * 1024;
/// Inline image bytes only up to this size (5 MB) as a base64 data URI.
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

impl Dispatcher {
    /// `artifacts.list {}` → one object per slug dir under the artifacts root,
    /// newest first, each carrying its files' name/rel/bytes/kind. A missing or
    /// empty root is `[]`, never an error.
    pub(super) fn artifacts_list(&self, req: RpcRequest) {
        self.send(ok_response(req.id, list_artifacts(&artifacts_root())));
    }

    /// `artifacts.get { path }` — resolve `<slug>/<file>` (a `rel` from list)
    /// under the artifacts root and return its mime/kind plus inlined text or
    /// image bytes when small enough. Anything escaping the root is -32602.
    pub(super) fn artifacts_get(&self, req: RpcRequest) {
        let Some(rel) = req.params.get("path").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing path"));
            return;
        };
        match get_artifact(&artifacts_root(), rel) {
            Ok(value) => self.send(ok_response(req.id, value)),
            Err(message) => self.send(err_response(req.id, -32602, message)),
        }
    }
}

/// `$REGENT_HOME/artifacts` — the only root artifacts are read from.
pub(super) fn artifacts_root() -> PathBuf {
    crate::application::http_serve::regent_home().join("artifacts")
}

/// Build the `artifacts.list` array: one entry per slug directory, newest
/// first (by dir mtime). Dotfiles and non-directories at the top level are
/// skipped; an unreadable root yields `[]`.
pub(super) fn list_artifacts(root: &Path) -> Value {
    let Ok(entries) = std::fs::read_dir(root) else {
        return json!([]);
    };
    let mut slugs: Vec<(f64, Value)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = file_name_str(&path) else {
            continue;
        };
        if name.starts_with('.') || !path.is_dir() {
            continue;
        }
        let created_at = dir_mtime(&path);
        slugs.push((
            created_at,
            json!({
                "name": name,
                "created_at": created_at,
                "files": list_files(&path, &name),
            }),
        ));
    }
    slugs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Value::Array(slugs.into_iter().map(|(_, value)| value).collect())
}

/// Files directly inside one slug dir (dotfiles and subdirs skipped), each as
/// `{name, rel, bytes, kind}`, sorted by name for a stable order.
fn list_files(dir: &Path, slug: &str) -> Value {
    let mut files: Vec<Value> = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Value::Array(files);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = file_name_str(&path) else {
            continue;
        };
        if name.starts_with('.') || !path.is_file() {
            continue;
        }
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        files.push(json!({
            "name": name,
            "rel": format!("{slug}/{name}"),
            "bytes": bytes,
            "kind": classify_kind(&name),
        }));
    }
    files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Value::Array(files)
}

/// Resolve `rel` under `root` (traversal-safe) and build the `artifacts.get`
/// result, inlining small text (`text`) or images (`data_base64`). Returns an
/// error message (mapped to -32602) for a path that escapes the root or is gone.
pub(super) fn get_artifact(root: &Path, rel: &str) -> Result<Value, String> {
    let candidate = root.join(rel);
    if !attachment_within_root(root, &candidate) {
        return Err("path escapes the artifacts root".to_owned());
    }
    let abs = candidate
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let kind = classify_kind(rel);
    let mut out = json!({
        "path": rel,
        "abs": abs.display().to_string(),
        "mime": guess_mime(rel),
        "kind": kind,
    });
    let len = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(u64::MAX);
    if kind == "text" && len <= MAX_TEXT_BYTES {
        if let Ok(bytes) = std::fs::read(&abs) {
            out["text"] = Value::String(String::from_utf8_lossy(&bytes).into_owned());
        }
    } else if kind == "image"
        && len <= MAX_IMAGE_BYTES
        && let Ok(bytes) = std::fs::read(&abs)
    {
        out["data_base64"] = Value::String(STANDARD.encode(bytes));
    }
    Ok(out)
}

/// Classify a file name by extension into the three viewer buckets.
pub(super) fn classify_kind(name: &str) -> &'static str {
    match extension(name).as_str() {
        "md" | "markdown" | "txt" | "json" | "csv" | "log" | "toml" | "yaml" | "yml" | "rs"
        | "ts" | "tsx" | "js" | "py" | "html" | "css" | "xml" | "sh" => "text",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => "image",
        _ => "other",
    }
}

/// Best-effort MIME from extension; unknown types fall back to octet-stream.
pub(super) fn guess_mime(name: &str) -> &'static str {
    match extension(name).as_str() {
        "md" | "markdown" => "text/markdown",
        "txt" | "log" => "text/plain",
        "json" => "application/json",
        "csv" => "text/csv",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "html" => "text/html",
        "css" => "text/css",
        "xml" => "application/xml",
        "js" => "text/javascript",
        "rs" | "ts" | "tsx" | "py" | "sh" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Lowercased final extension of `name` (empty when it has none). `docx.pdf`
/// resolves to `pdf`, matching how the shell would open it.
fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// The final path component as an owned `String`, or `None` if not valid UTF-8.
fn file_name_str(path: &Path) -> Option<String> {
    path.file_name().and_then(|n| n.to_str()).map(str::to_owned)
}

/// Directory mtime as f64 epoch seconds (0.0 when unavailable).
fn dir_mtime(path: &Path) -> f64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
#[path = "artifacts_ops_tests.rs"]
mod tests;
