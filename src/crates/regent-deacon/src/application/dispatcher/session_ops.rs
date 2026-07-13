//! Session/turn lifecycle handlers: create/resume/list/search, the streamed
//! `prompt.submit` turn, interrupt, and approval response.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_kernel::SessionId;
use serde_json::json;

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
        match self
            .sessions
            .resume_session(SessionId::from_string(s))
            .await
        {
            Ok(id) => self.send(ok_response(req.id, json!({"session_id": id.to_string()}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn session_list(&self, req: RpcRequest) {
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;
        match self.sessions.list_sessions(limit) {
            Ok(list) => {
                let items: Vec<_> = list
                    .iter()
                    .map(|m| {
                        json!({
                            "session_id": m.id, "source": m.source, "model": m.model,
                            "message_count": m.message_count, "started_at": m.started_at,
                            // Additive organization fields (M7): present but
                            // null/false for sessions that were never touched.
                            "title": m.title, "pinned": m.pinned, "archived": m.archived,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// Stored transcript for one session (user/assistant text rows only —
    /// tool plumbing stays internal). Additive API: `session.history`.
    pub(super) fn session_history(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        match self.sessions.session_history(&SessionId::from_string(s)) {
            Ok(messages) => {
                let items: Vec<_> = messages
                    .iter()
                    .filter(|m| {
                        matches!(
                            m.message.role,
                            regent_kernel::Role::User | regent_kernel::Role::Assistant
                        ) && (m.message.content.as_deref().is_some_and(|c| !c.is_empty())
                            || !m.message.tool_calls.is_empty())
                    })
                    .map(|m| {
                        let tools: Vec<&str> = m
                            .message
                            .tool_calls
                            .iter()
                            .map(|c| c.name.as_str())
                            .collect();
                        json!({
                            "role": m.message.role.as_str(),
                            "text": m.message.content.as_deref().unwrap_or_default(),
                            "reasoning": m.message.reasoning,
                            "tool_calls": tools,
                            "timestamp": m.timestamp,
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
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as u32;
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
        let approved = req
            .params
            .get("approved")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Additive: free text riding a denial — deny-reason or ask_user answer.
        let feedback = req
            .params
            .get("feedback")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let resolved = self
            .sessions
            .resolve_approval(&SessionId::from_string(s), approved, feedback)
            .await;
        self.send(ok_response(req.id, json!({"resolved": resolved})));
    }
}
