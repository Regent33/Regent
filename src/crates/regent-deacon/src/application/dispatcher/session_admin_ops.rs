//! Session-organization handlers: rename, pin, archive, and delete. Additive
//! surface (M7) — `session.list` gained the matching title/pinned/archived
//! fields; these mutate them. All return `{found}` (or `{deleted}`) so a stale
//! session id is a soft miss, not a JSON-RPC error.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_kernel::SessionId;
use serde_json::json;

impl Dispatcher {
    pub(super) fn session_rename(&self, req: RpcRequest) {
        let (Some(s), Some(title)) = (
            req.params.get("session_id").and_then(|v| v.as_str()),
            req.params.get("title").and_then(|v| v.as_str()),
        ) else {
            self.send(err_response(req.id, -32602, "missing session_id or title"));
            return;
        };
        match self
            .sessions
            .rename_session(&SessionId::from_string(s), title)
        {
            Ok(found) => self.send(ok_response(req.id, json!({"found": found}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn session_pin(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let pinned = req
            .params
            .get("pinned")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        match self
            .sessions
            .set_session_pinned(&SessionId::from_string(s), pinned)
        {
            Ok(found) => self.send(ok_response(
                req.id,
                json!({"found": found, "pinned": pinned}),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn session_archive(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let archived = req
            .params
            .get("archived")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        match self
            .sessions
            .set_session_archived(&SessionId::from_string(s), archived)
        {
            Ok(found) => self.send(ok_response(
                req.id,
                json!({"found": found, "archived": archived}),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn session_delete(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        match self.sessions.delete_session(&SessionId::from_string(s)) {
            Ok(deleted) => self.send(ok_response(req.id, json!({"deleted": deleted}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// One-shot backfill: names untitled sessions that hold a real exchange,
    /// reusing first-turn titling's model call. `limit` (default 50) caps model
    /// calls per invocation so a call is bounded; the reply's `remaining` lets a
    /// caller repeat until every eligible session is titled. Additive op —
    /// `session.backfill_titles`.
    pub(super) async fn session_backfill_titles(&self, req: RpcRequest) {
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;
        match self.sessions.backfill_titles(limit).await {
            Ok(report) => self.send(ok_response(
                req.id,
                json!({
                    "titled": report.titled,
                    "skipped": report.skipped,
                    "remaining": report.remaining,
                }),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}
