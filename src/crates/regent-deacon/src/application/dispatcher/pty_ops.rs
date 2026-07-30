//! `pty.*` — interactive terminals for the workspace panel (ADR-044).
//!
//! **Reachable over stdio only.** The HTTP ingress serves `/health` and
//! `/v1/chat` and nothing else, and the gateway is a separate binary that does
//! not route this dispatcher, so these four methods have no remote surface. That
//! containment is structural, not a check in this file — and it is the reason a
//! deliberately unconfined shell is acceptable here. If the dispatcher is ever
//! exposed over a socket, `pty.*` becomes a remote shell and must be gated at
//! that boundary.
//!
//! No `pty` tool is registered anywhere, so the agent cannot reach any of this.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use regent_kernel::SessionId;
use serde_json::{Value, json};

/// Clamped rather than trusted: xterm reports 0×0 for one frame while its
/// container is still laying out, and a 0-column pty makes the shell emit
/// nothing at all — which looks exactly like a terminal that failed to start.
fn dimension(params: &Value, key: &str, fallback: u16) -> u16 {
    let raw = params.get(key).and_then(Value::as_u64).unwrap_or(0);
    u16::try_from(raw).unwrap_or(fallback).clamp(2, 1000)
}

impl Dispatcher {
    /// `pty.open { session_id?, cols?, rows? }` → `{ pty_id }`.
    ///
    /// `session_id` is optional and only used to seed the working directory from
    /// that session's workspace root. A terminal is not owned by a conversation —
    /// closing the chat should not kill the shell someone is typing in.
    pub(super) async fn pty_open(&self, req: RpcRequest) {
        let cols = dimension(&req.params, "cols", 80);
        let rows = dimension(&req.params, "rows", 24);
        let cwd = match req.params.get("session_id").and_then(Value::as_str) {
            Some(raw) => {
                self.sessions
                    .workspace_root(&SessionId::from_string(raw.to_owned()))
                    .await
            }
            None => None,
        };

        let id = format!("pty_{}", uuid::Uuid::new_v4().simple());
        let out_for_data = self.out_tx.clone();
        let out_for_exit = self.out_tx.clone();
        let emit = Box::new(move |pty_id: &str, payload: &str| {
            let notif = crate::domain::entities::RpcNotification::new(
                "pty.data",
                json!({ "pty_id": pty_id, "data": payload }),
            );
            if let Ok(line) = serde_json::to_string(&notif) {
                out_for_data.send(line).ok();
            }
        });
        let on_exit = Box::new(move |pty_id: &str| {
            let notif = crate::domain::entities::RpcNotification::new(
                "pty.exit",
                json!({ "pty_id": pty_id }),
            );
            if let Ok(line) = serde_json::to_string(&notif) {
                out_for_exit.send(line).ok();
            }
        });

        match self
            .ptys
            .open(id.clone(), cwd.as_deref(), (cols, rows), emit, on_exit)
        {
            Ok(()) => {
                tracing::info!(pty = %id, cols, rows, "terminal opened");
                self.send(ok_response(req.id, json!({ "pty_id": id })));
            }
            Err(e) => self.send(err_response(req.id, -32602, e.to_string())),
        }
    }

    /// `pty.write { pty_id, data }` — `data` is base64 keystrokes.
    pub(super) fn pty_write(&self, req: RpcRequest) {
        let Some(id) = req.params.get("pty_id").and_then(Value::as_str) else {
            self.send(err_response(req.id, -32602, "pty_id is required"));
            return;
        };
        let Some(encoded) = req.params.get("data").and_then(Value::as_str) else {
            self.send(err_response(req.id, -32602, "data is required"));
            return;
        };
        // Base64 in this direction too, so a control byte (Ctrl+C is 0x03) is
        // carried exactly rather than depending on JSON string escaping.
        let Ok(bytes) = B64.decode(encoded) else {
            self.send(err_response(req.id, -32602, "data must be base64"));
            return;
        };
        match self.ptys.write(id, &bytes) {
            Ok(()) => self.send(ok_response(req.id, json!({ "written": bytes.len() }))),
            Err(e) => self.send(err_response(req.id, -32602, e.to_string())),
        }
    }

    /// `pty.resize { pty_id, cols, rows }`.
    pub(super) fn pty_resize(&self, req: RpcRequest) {
        let Some(id) = req.params.get("pty_id").and_then(Value::as_str) else {
            self.send(err_response(req.id, -32602, "pty_id is required"));
            return;
        };
        let cols = dimension(&req.params, "cols", 80);
        let rows = dimension(&req.params, "rows", 24);
        match self.ptys.resize(id, cols, rows) {
            Ok(()) => self.send(ok_response(req.id, json!({ "cols": cols, "rows": rows }))),
            Err(e) => self.send(err_response(req.id, -32602, e.to_string())),
        }
    }

    /// `pty.close { pty_id }`. Idempotent — a client closes on unmount, and
    /// closing an already-dead terminal is not a failure worth reporting.
    pub(super) fn pty_close(&self, req: RpcRequest) {
        if let Some(id) = req.params.get("pty_id").and_then(Value::as_str) {
            self.ptys.close(id);
        }
        self.send(ok_response(req.id, json!({ "closed": true })));
    }
}
