//! Attachment staging: `attachment.put` writes a client-supplied file under
//! `$REGENT_HOME/attachments/<session_id>/<name>` so the next `prompt.submit`
//! can reference it by path (the agent's file tools then read it). Names are
//! sanitized against traversal and the decoded payload is capped, so a client
//! can never write outside the attachments root.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Hard cap on a single decoded attachment (20 MB).
const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;

impl Dispatcher {
    /// `attachment.put` — stage `data_base64` as a file the next prompt can cite.
    /// Params: `{session_id, name, mime?, data_base64}` → `{path, bytes}`.
    pub(super) fn attachment_put(&self, req: RpcRequest) {
        let (Some(session_id), Some(name), Some(data_b64)) = (
            req.params.get("session_id").and_then(|v| v.as_str()),
            req.params.get("name").and_then(|v| v.as_str()),
            req.params.get("data_base64").and_then(|v| v.as_str()),
        ) else {
            self.send(err_response(
                req.id,
                -32602,
                "missing session_id, name, or data_base64",
            ));
            return;
        };
        // `mime` is accepted but advisory — the agent's file tools sniff content;
        // we don't persist it. `name` and `session_id` both become path
        // components, so both are sanitized against traversal.
        let (Some(safe_session), Some(safe_name)) =
            (sanitize_component(session_id), sanitize_component(name))
        else {
            self.send(err_response(req.id, -32602, "invalid session_id or name"));
            return;
        };
        let bytes = match STANDARD.decode(data_b64) {
            Ok(b) => b,
            Err(_) => {
                self.send(err_response(
                    req.id,
                    -32602,
                    "data_base64 is not valid base64",
                ));
                return;
            }
        };
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            self.send(err_response(
                req.id,
                -32602,
                format!("attachment exceeds {MAX_ATTACHMENT_BYTES}-byte limit"),
            ));
            return;
        }
        let dir = attachments_root().join(&safe_session);
        if let Err(error) = std::fs::create_dir_all(&dir) {
            self.send(err_response(req.id, -32000, error.to_string()));
            return;
        }
        let path = dir.join(&safe_name);
        if let Err(error) = std::fs::write(&path, &bytes) {
            self.send(err_response(req.id, -32000, error.to_string()));
            return;
        }
        self.send(ok_response(
            req.id,
            json!({ "path": path.display().to_string(), "bytes": bytes.len() }),
        ));
    }
}

/// `$REGENT_HOME/attachments` — the only root a staged file may live under.
pub(super) fn attachments_root() -> PathBuf {
    crate::application::http_serve::regent_home().join("attachments")
}

/// Clean one path component (an attachment name or session id) or reject it.
/// Refuses anything that could escape a single directory level: empty, `.`/`..`,
/// a path separator, a parent ref, a NUL, or a drive/ADS `:` — so the result is
/// always a plain file/dir name that stays inside its parent.
pub(super) fn sanitize_component(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
        || trimmed.contains(':')
        || trimmed.contains('\0')
    {
        return None;
    }
    Some(trimmed.to_owned())
}

/// True when `candidate` resolves to a real path inside `root`. Both are
/// canonicalized, so `..` segments and symlinks can't smuggle a path out of the
/// attachments area; a candidate that doesn't exist (never staged) is rejected.
pub(super) fn attachment_within_root(root: &Path, candidate: &Path) -> bool {
    match (root.canonicalize(), candidate.canonicalize()) {
        (Ok(root), Ok(cand)) => cand.starts_with(&root),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{attachment_within_root, sanitize_component};

    #[test]
    fn sanitize_rejects_traversal_and_separators() {
        // Rejected: traversal, nesting, absolute/drive, dot names, empties.
        for bad in [
            "",
            "   ",
            ".",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "..\\evil",
            "C:\\x",
            "x:stream",
            "foo/../bar",
            "a\0b",
        ] {
            assert!(sanitize_component(bad).is_none(), "should reject {bad:?}");
        }
        // Accepted: plain names (trimmed).
        assert_eq!(
            sanitize_component("report.pdf").as_deref(),
            Some("report.pdf")
        );
        assert_eq!(sanitize_component("  a.txt ").as_deref(), Some("a.txt"));
        assert_eq!(
            sanitize_component("photo 2026.png").as_deref(),
            Some("photo 2026.png")
        );
    }

    #[test]
    fn within_root_gates_by_real_location() {
        let root = tempfile::tempdir().unwrap();
        let inside = root.path().join("s1");
        std::fs::create_dir_all(&inside).unwrap();
        let file = inside.join("a.txt");
        std::fs::write(&file, b"hi").unwrap();
        assert!(attachment_within_root(root.path(), &file));

        // A sibling directory outside the root is rejected.
        let outside = tempfile::tempdir().unwrap();
        let other = outside.path().join("b.txt");
        std::fs::write(&other, b"no").unwrap();
        assert!(!attachment_within_root(root.path(), &other));

        // A non-existent path can't be proven inside → rejected.
        assert!(!attachment_within_root(
            root.path(),
            &inside.join("ghost.txt")
        ));
    }
}
