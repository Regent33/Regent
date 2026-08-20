//! `image.get` — inline a local image as a base64 data URI for the chat
//! transcript. The webview has no filesystem access and the CSP carries no
//! `asset:` scheme, so a local path in model output (a camera capture, a
//! generated document, an artifact) or on a staged attachment renders as a
//! broken image. This is the same answer `artifacts.get` already gives the
//! Artifacts window, scoped to images: canonicalized within-root check, 5 MB
//! cap, -32602 for anything that escapes.
//!
//! Roots: `$REGENT_HOME/attachments` (plus the caller's own session folder,
//! so a transcript chip that only kept the file NAME still resolves),
//! `$REGENT_HOME/artifacts`, and the session's workspace when one is BOUND.
//! Deliberately NOT the deacon's default cwd, which `workspace_root` falls
//! back to — the coding panel opts into that folder, a chat image reference
//! never did.

use super::Dispatcher;
use super::artifacts_ops::{artifacts_root, classify_kind, guess_mime};
use super::attachment_ops::{attachment_within_root, attachments_root, sanitize_component};
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use regent_kernel::SessionId;
use serde_json::{Value, json};
use std::path::PathBuf;

/// Inline image bytes only up to this size (5 MB) — the same ceiling
/// `artifacts_ops` inlines an image at.
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

impl Dispatcher {
    /// `image.get { path, session_id? }` → `{mime, data_uri}`. `path` is
    /// either absolute or relative to one of the allowed roots. Anything
    /// outside them, anything that isn't an image, and anything over the cap
    /// is -32602.
    pub(super) async fn image_get(&self, req: RpcRequest) {
        let Some(path) = req.params.get("path").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing path"));
            return;
        };
        let roots = self.image_roots(req.params.get("session_id")).await;
        match read_image(&roots, path) {
            Ok(value) => self.send(ok_response(req.id, value)),
            Err(message) => self.send(err_response(req.id, -32602, message)),
        }
    }

    /// Every root this session may read an image out of, most specific first.
    async fn image_roots(&self, session_id: Option<&Value>) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(raw) = session_id.and_then(Value::as_str) {
            // The id becomes a path component, so it is sanitized exactly as
            // `attachment.put` sanitizes it on the way in — an unchecked one
            // could name a root that is already outside the attachments area,
            // and a within-root check against THAT root proves nothing.
            if let Some(safe) = sanitize_component(raw) {
                roots.push(attachments_root().join(safe));
            }
            let id = SessionId::from_string(raw);
            if self.sessions.workspace_is_default(&id).await == Some(false)
                && let Some(root) = self.sessions.workspace_root(&id).await
            {
                roots.push(root);
            }
        }
        roots.push(attachments_root());
        roots.push(artifacts_root());
        roots
    }
}

/// First root `rel` resolves inside, as a real canonical path. `Path::join`
/// with an absolute `rel` yields `rel` itself, so absolute and relative
/// references take the same gate.
fn resolve_within(roots: &[PathBuf], rel: &str) -> Option<PathBuf> {
    roots.iter().find_map(|root| {
        let candidate = root.join(rel);
        if attachment_within_root(root, &candidate) {
            candidate.canonicalize().ok()
        } else {
            None
        }
    })
}

/// Resolve `rel` against `roots` (traversal-safe) and inline it as a data
/// URI. Returns the message mapped to -32602 for a path outside every root, a
/// non-image, or one over the cap.
pub(super) fn read_image(roots: &[PathBuf], rel: &str) -> Result<Value, String> {
    if classify_kind(rel) != "image" {
        return Err("not an image".to_owned());
    }
    let abs = resolve_within(roots, rel).ok_or("path escapes the readable image roots")?;
    let len = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(u64::MAX);
    if len > MAX_IMAGE_BYTES {
        return Err(format!("image exceeds {MAX_IMAGE_BYTES}-byte limit"));
    }
    let bytes = std::fs::read(&abs).map_err(|error| error.to_string())?;
    let mime = guess_mime(rel);
    Ok(json!({
        "mime": mime,
        "data_uri": format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
    }))
}

#[cfg(test)]
#[path = "image_ops_tests.rs"]
mod tests;
