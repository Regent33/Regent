//! `workspace.*` — the file tree/read/write surface behind the Desktop coding
//! panel, scoped to ONE session's workspace root (its opened folder, else the
//! deacon's cwd). The webview has no filesystem access, so every read and save
//! travels through here, gated by the same canonicalized within-root check
//! `attachment.put`/`artifacts.*` use.
//!
//! Two deliberate differences from `artifacts_ops`, both because this path is
//! read-then-EDIT-then-write-back rather than read-only display:
//!
//! * Decoding is strict UTF-8. `artifacts.get` uses `from_utf8_lossy`, which is
//!   harmless for a preview — but here a lossily-decoded file would be saved
//!   back with real replacement characters, permanently corrupting it.
//! * Oversized files are REFUSED, never truncated. A truncated read followed by
//!   a save would truncate the file on disk.

use super::Dispatcher;
use super::attachment_ops::attachment_within_root;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_kernel::SessionId;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Editable-file ceiling (5 MB), mirroring `artifacts_ops`'s image cap. The
/// 20K-char `CODE_DETAIL_MAX_CHARS` is deliberately NOT the model here: that
/// bounds a chat-transcript disclosure, and most real source files exceed it.
const MAX_EDIT_BYTES: u64 = 5 * 1024 * 1024;

/// Never surfaced in the tree: VCS internals and build output. Hardcoded rather
/// than `.gitignore`-aware — parsing ignore rules is a real project, and these
/// seven cover the noise that actually buries a file list.
const IGNORED: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "out",
    ".next",
    ".turbo",
];

impl Dispatcher {
    /// `workspace.get { session_id }` → `{root, is_default}`.
    pub(super) async fn workspace_get(&self, req: RpcRequest) {
        let Some(id) = self.session_id_param(&req) else {
            return;
        };
        match self.sessions.workspace_root(&id).await {
            Some(root) => {
                let is_default = self
                    .sessions
                    .workspace_is_default(&id)
                    .await
                    .unwrap_or(true);
                self.send(ok_response(
                    req.id,
                    json!({"root": root.display().to_string(), "is_default": is_default}),
                ));
            }
            None => self.send(err_response(req.id, -32602, "unknown session")),
        }
    }

    /// `workspace.tree { session_id, path? }` → one directory level.
    pub(super) async fn workspace_tree(&self, req: RpcRequest) {
        let Some(id) = self.session_id_param(&req) else {
            return;
        };
        let rel = req
            .params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let Some(root) = self.sessions.workspace_root(&id).await else {
            self.send(err_response(req.id, -32602, "unknown session"));
            return;
        };
        match tree_at(&root, rel) {
            Ok(value) => self.send(ok_response(req.id, value)),
            Err(message) => self.send(err_response(req.id, -32602, message)),
        }
    }

    /// `workspace.read { session_id, path }` → `{path, bytes, rev, text|binary}`.
    pub(super) async fn workspace_read(&self, req: RpcRequest) {
        let Some(id) = self.session_id_param(&req) else {
            return;
        };
        let Some(rel) = req.params.get("path").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing path"));
            return;
        };
        let Some(root) = self.sessions.workspace_root(&id).await else {
            self.send(err_response(req.id, -32602, "unknown session"));
            return;
        };
        match read_file_at(&root, rel) {
            Ok(value) => self.send(ok_response(req.id, value)),
            Err(message) => self.send(err_response(req.id, -32602, message)),
        }
    }

    /// `workspace.write { session_id, path, content, rev }` → `{path, bytes, rev}`.
    /// `rev` is the token from the read this edit started from; a mismatch means
    /// the file moved underneath the editor and the save is refused.
    pub(super) async fn workspace_write(&self, req: RpcRequest) {
        let Some(id) = self.session_id_param(&req) else {
            return;
        };
        let (Some(rel), Some(content)) = (
            req.params.get("path").and_then(|v| v.as_str()),
            req.params.get("content").and_then(|v| v.as_str()),
        ) else {
            self.send(err_response(req.id, -32602, "missing path or content"));
            return;
        };
        let rev = req.params.get("rev").and_then(|v| v.as_str()).unwrap_or("");
        let Some(root) = self.sessions.workspace_root(&id).await else {
            self.send(err_response(req.id, -32602, "unknown session"));
            return;
        };
        match write_file_at(&root, rel, content, rev) {
            Ok(value) => self.send(ok_response(req.id, value)),
            Err(message) => self.send(err_response(req.id, -32602, message)),
        }
    }

    /// Shared `session_id` extraction: replies -32602 and yields `None` when the
    /// param is absent, so each handler above stays a straight line.
    fn session_id_param(&self, req: &RpcRequest) -> Option<SessionId> {
        match req.params.get("session_id").and_then(|v| v.as_str()) {
            Some(raw) => Some(SessionId::from_string(raw)),
            None => {
                // Cloned: the caller still owns `req` and needs its id for the
                // success reply on the path where this returns Some.
                self.send(err_response(req.id.clone(), -32602, "missing session_id"));
                None
            }
        }
    }
}

/// Resolve `rel` under `root`, refusing anything that escapes it.
fn resolve_within(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let candidate = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    if !attachment_within_root(root, &candidate) {
        return Err("path escapes the workspace root".to_owned());
    }
    candidate.canonicalize().map_err(|error| error.to_string())
}

/// A file's revision token: mtime-nanos + size. Cheap, needs no hashing, and
/// changes on any write the editor didn't make — enough to catch the
/// lost-update case (a stale buffer saved over a newer file).
fn revision(path: &Path) -> String {
    let meta = std::fs::metadata(path);
    let mtime = meta
        .as_ref()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let len = meta.map(|m| m.len()).unwrap_or(0);
    format!("{mtime}-{len}")
}

/// One directory level under `root`: directories first, then files, each
/// alphabetical. Build/VCS noise is dropped so the panel shows source, not
/// dependency trees.
pub(super) fn tree_at(root: &Path, rel: &str) -> Result<Value, String> {
    let abs = resolve_within(root, rel)?;
    if !abs.is_dir() {
        return Err("not a directory".to_owned());
    }
    let entries = std::fs::read_dir(&abs).map_err(|error| error.to_string())?;
    let mut dirs: Vec<Value> = Vec::new();
    let mut files: Vec<Value> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            continue;
        };
        if IGNORED.contains(&name.as_str()) {
            continue;
        }
        // `rel` is the client's own path vocabulary, so build children from it
        // (never from the canonicalized abs, whose prefix the client never saw).
        let child_rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{}/{name}", rel.trim_end_matches('/'))
        };
        if path.is_dir() {
            dirs.push(json!({"name": name, "path": child_rel, "kind": "dir"}));
        } else {
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            files.push(json!({
                "name": name, "path": child_rel, "kind": "file", "bytes": bytes,
            }));
        }
    }
    let by_name = |a: &Value, b: &Value| a["name"].as_str().cmp(&b["name"].as_str());
    dirs.sort_by(by_name);
    files.sort_by(by_name);
    dirs.append(&mut files);
    Ok(json!({"path": rel, "entries": dirs}))
}

/// Read one editable file. Non-UTF-8 content reports `binary: true` rather than
/// decoding lossily, and anything over the cap is refused outright.
pub(super) fn read_file_at(root: &Path, rel: &str) -> Result<Value, String> {
    let abs = resolve_within(root, rel)?;
    if !abs.is_file() {
        return Err("not a file".to_owned());
    }
    let bytes = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
    if bytes > MAX_EDIT_BYTES {
        return Err(format!(
            "file is {bytes} bytes — too large to edit here (limit {MAX_EDIT_BYTES})"
        ));
    }
    let raw = std::fs::read(&abs).map_err(|error| error.to_string())?;
    let rev = revision(&abs);
    match String::from_utf8(raw) {
        Ok(text) => Ok(json!({"path": rel, "bytes": bytes, "rev": rev, "text": text})),
        // Strict, not lossy: saving a lossily-decoded file back would write the
        // replacement characters to disk and destroy the original bytes.
        Err(_) => Ok(json!({"path": rel, "bytes": bytes, "rev": rev, "binary": true})),
    }
}

/// Overwrite an existing file, but only if it still matches the `rev` the
/// caller read. v1 edits existing files only — no create-by-save.
pub(super) fn write_file_at(
    root: &Path,
    rel: &str,
    content: &str,
    rev: &str,
) -> Result<Value, String> {
    let abs = resolve_within(root, rel)?;
    if !abs.is_file() {
        return Err("not a file".to_owned());
    }
    if content.len() as u64 > MAX_EDIT_BYTES {
        return Err("content exceeds the editable size limit".to_owned());
    }
    // The lost-update guard. "Disable saving while the turn runs" does not cover
    // a buffer opened BEFORE a code task and saved just AFTER it finished.
    let current = revision(&abs);
    if !rev.is_empty() && rev != current {
        return Err("file changed on disk since it was opened — reload before saving".to_owned());
    }
    std::fs::write(&abs, content).map_err(|error| error.to_string())?;
    Ok(json!({
        "path": rel,
        "bytes": content.len(),
        "rev": revision(&abs),
    }))
}

#[cfg(test)]
#[path = "workspace_ops_tests.rs"]
mod tests;
