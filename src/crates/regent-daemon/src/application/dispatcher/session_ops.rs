//! Session/turn lifecycle handlers: create/resume/list/search, the streamed
//! `prompt.submit` turn, interrupt, and approval response.

use super::Dispatcher;
use crate::domain::entities::{RpcNotification, RpcRequest, err_response, ok_response};
use crate::domain::errors::DaemonError;
use regent_kernel::{RegentError, SessionId};
use serde_json::json;
use std::sync::Arc;

impl Dispatcher {
    pub(super) async fn session_create(&self, req: RpcRequest) {
        match self.sessions.create_session().await {
            Ok(id) => self.send(ok_response(req.id, json!({"session_id": id.to_string()}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) async fn session_resume(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        match self.sessions.resume_session(SessionId::from_string(s)).await {
            Ok(id) => self.send(ok_response(req.id, json!({"session_id": id.to_string()}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn session_list(&self, req: RpcRequest) {
        let limit = req.params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
        match self.sessions.list_sessions(limit) {
            Ok(list) => {
                let items: Vec<_> = list
                    .iter()
                    .map(|m| {
                        json!({
                            "session_id": m.id, "source": m.source, "model": m.model,
                            "message_count": m.message_count, "started_at": m.started_at,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn session_search(&self, req: RpcRequest) {
        let Some(query) = req.params.get("query").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing query"));
            return;
        };
        let limit = req.params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
        match self.sessions.search_sessions(query, limit) {
            Ok(hits) => {
                let items: Vec<_> = hits
                    .iter()
                    .map(|h| {
                        json!({
                            "session_id": h.session_id, "role": h.role,
                            "snippet": h.snippet, "timestamp": h.timestamp,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// Submits a prompt and streams the turn: a `turn.started` notification,
    /// then (from a spawned task) `message.complete`/`turn.complete` (or
    /// `turn.interrupted`) followed by the final JSON-RPC response.
    pub(super) fn prompt_submit(&self, req: RpcRequest) {
        let id = req.id.clone();
        let Some(sid_str) =
            req.params.get("session_id").and_then(|v| v.as_str()).map(str::to_owned)
        else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let Some(text) = req.params.get("text").and_then(|v| v.as_str()).map(str::to_owned) else {
            self.send(err_response(req.id, -32602, "missing text"));
            return;
        };
        let session_id = SessionId::from_string(sid_str.clone());
        self.notify("turn.started", json!({"session_id": sid_str}));

        let sessions = Arc::clone(&self.sessions);
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let send = |payload: String| {
                out_tx.send(payload).ok();
            };
            let notify = |method: &str, params: serde_json::Value| {
                if let Ok(line) = serde_json::to_string(&RpcNotification::new(method, params)) {
                    out_tx.send(line).ok();
                }
            };
            match sessions.run_turn(&session_id, &text).await {
                Ok(reply) => {
                    notify(
                        "message.complete",
                        json!({"session_id": session_id.to_string(), "reply": reply}),
                    );
                    notify("turn.complete", json!({"session_id": session_id.to_string()}));
                    let resp = ok_response(
                        id,
                        json!({"reply": reply, "session_id": session_id.to_string()}),
                    );
                    if let Ok(line) = serde_json::to_string(&resp) {
                        send(line);
                    }
                }
                Err(error) => {
                    let interrupted = matches!(
                        &error,
                        DaemonError::Core(RegentError::Interrupted)
                    );
                    notify(
                        if interrupted { "turn.interrupted" } else { "turn.complete" },
                        json!({"session_id": session_id.to_string(), "error": error.to_string()}),
                    );
                    let resp = err_response(id, -32000, error.to_string());
                    if let Ok(line) = serde_json::to_string(&resp) {
                        send(line);
                    }
                }
            }
        });
    }

    pub(super) async fn turn_interrupt(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let cancelled = self.sessions.interrupt(&SessionId::from_string(s)).await;
        self.send(ok_response(req.id, json!({"cancelled": cancelled})));
    }

    pub(super) async fn approval_respond(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let approved = req.params.get("approved").and_then(|v| v.as_bool()).unwrap_or(false);
        let resolved = self.sessions.resolve_approval(&SessionId::from_string(s), approved).await;
        self.send(ok_response(req.id, json!({"resolved": resolved})));
    }
}
