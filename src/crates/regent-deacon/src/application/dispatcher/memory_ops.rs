//! `memory.*` handlers — the write-approval queue plus browse/pin/forget.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    pub(super) fn memory_pending(&self, req: RpcRequest) {
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as u32;
        match self.sessions.pending_memory_writes(limit) {
            Ok(writes) => {
                let items: Vec<_> = writes
                    .iter()
                    .map(|w| {
                        json!({
                            "id": w.id, "kind": w.kind, "name": w.name, "content": w.content,
                            "provenance": w.provenance, "trust": w.trust, "created_at": w.created_at,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_approve(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.approve_memory_write(id) {
            Ok(node_id) => self.send(ok_response(
                req.id,
                json!({"approved": node_id.is_some(), "node_id": node_id}),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_reject(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.reject_memory_write(id) {
            Ok(removed) => self.send(ok_response(req.id, json!({"removed": removed}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_list(&self, req: RpcRequest) {
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as u32;
        match self.sessions.list_memory(limit) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "id": n.id, "kind": n.kind, "name": n.name,
                            "content": n.content, "pinned": n.pinned,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_pin(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.pin_memory(id) {
            Ok(found) => self.send(ok_response(req.id, json!({"found": found, "pinned": true}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_unpin(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.unpin_memory(id) {
            Ok(found) => self.send(ok_response(
                req.id,
                json!({"found": found, "pinned": false}),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_forget(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.forget_memory(id) {
            Ok(forgotten) => self.send(ok_response(req.id, json!({"forgotten": forgotten}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}
