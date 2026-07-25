//! `git.*` — status/diff/commit/push for a session's workspace, backing the
//! Desktop coding panel's Commit / Commit+Push / Push buttons. Thin wrappers:
//! all git behavior (and its tests) lives in `regent_code::git_ops`.
//!
//! Error codes follow `code_ops`, not `artifacts_ops`: -32602 for bad INPUT
//! (missing session_id/message, unknown session), -32000 for a git EXECUTION
//! failure carrying git's own stderr — not-a-repo, nothing-to-commit, no
//! upstream, auth, network. Those aren't client mistakes, and the fix is in
//! git's own wording.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_kernel::SessionId;
use serde_json::json;
use std::path::PathBuf;

impl Dispatcher {
    /// `git.status { session_id }` → branch/upstream/ahead/behind + changed paths.
    pub(super) async fn git_status(&self, req: RpcRequest) {
        let Some(root) = self.workspace_for(&req).await else {
            return;
        };
        match regent_code::git_status(&root).await {
            Ok(status) => self.send(ok_response(
                req.id,
                json!({
                    "is_repo": status.is_repo,
                    "branch": status.branch,
                    "upstream": status.upstream,
                    "ahead": status.ahead,
                    "behind": status.behind,
                    "dirty": status.dirty(),
                    "entries": status.entries.iter().map(|e| json!({
                        "path": e.path, "status": e.status, "staged": e.staged,
                    })).collect::<Vec<_>>(),
                }),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// `git.diff { session_id }` → the working tree's diff against HEAD.
    pub(super) async fn git_diff(&self, req: RpcRequest) {
        let Some(root) = self.workspace_for(&req).await else {
            return;
        };
        match regent_code::git_diff(&root).await {
            Some(diff) => self.send(ok_response(req.id, json!({"is_repo": true, "diff": diff}))),
            None => self.send(ok_response(req.id, json!({"is_repo": false, "diff": ""}))),
        }
    }

    /// `git.commit { session_id, message }` → `{sha}`. Stages everything first.
    pub(super) async fn git_commit(&self, req: RpcRequest) {
        let message = req
            .params
            .get("message")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default()
            .to_owned();
        if message.is_empty() {
            self.send(err_response(req.id, -32602, "missing commit message"));
            return;
        }
        let Some(root) = self.workspace_for(&req).await else {
            return;
        };
        match regent_code::git_commit(&root, &message).await {
            Ok(sha) => self.send(ok_response(req.id, json!({"sha": sha}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// `git.push { session_id }` — DETACHED, like `code.start`: a push over a
    /// slow link can take a while, and the dispatcher's read loop is serial, so
    /// awaiting it here would freeze every other request behind it. The response
    /// still carries the original request id (stdio JSON-RPC matches by id).
    pub(super) async fn git_push(&self, req: RpcRequest) {
        let Some(root) = self.workspace_for(&req).await else {
            return;
        };
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let resp = match regent_code::git_push(&root).await {
                Ok(summary) => ok_response(req.id, json!({"ok": true, "summary": summary})),
                Err(e) => err_response(req.id, -32000, e.to_string()),
            };
            if let Ok(line) = serde_json::to_string(&resp) {
                out_tx.send(line).ok();
            }
        });
    }

    /// The session's workspace root, replying -32602 and yielding `None` when
    /// `session_id` is missing or names a session that isn't live.
    async fn workspace_for(&self, req: &RpcRequest) -> Option<PathBuf> {
        let Some(raw) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id.clone(), -32602, "missing session_id"));
            return None;
        };
        match self
            .sessions
            .workspace_root(&SessionId::from_string(raw))
            .await
        {
            Some(root) => Some(root),
            None => {
                self.send(err_response(req.id.clone(), -32602, "unknown session"));
                None
            }
        }
    }
}

/// Validate a client-supplied workspace path for `session.create`: it must
/// exist and be a directory. Rejecting up front beats creating a session whose
/// every later file/git call fails against a path that was never there.
pub(super) fn resolve_workspace_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("workspace must not be empty".to_owned());
    }
    let path = PathBuf::from(trimmed);
    if !path.is_dir() {
        return Err(format!("workspace is not a directory: {trimmed}"));
    }
    path.canonicalize()
        .map_err(|e| format!("workspace could not be resolved: {e}"))
}

#[cfg(test)]
#[path = "git_ops_tests.rs"]
mod tests;
