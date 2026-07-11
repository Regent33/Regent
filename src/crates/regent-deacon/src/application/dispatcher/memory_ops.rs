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

    pub(super) fn memory_graph(&self, req: RpcRequest) {
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(2000) as u32;
        match self.sessions.memory_graph(limit) {
            Ok(graph) => {
                let nodes: Vec<_> = graph
                    .nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "id": n.id, "kind": n.kind, "name": n.name,
                            "content": truncate_content(&n.content), "pinned": n.pinned,
                        })
                    })
                    .collect();
                let edges: Vec<_> = graph
                    .edges
                    .iter()
                    .map(|e| {
                        json!({
                            "src": e.src, "dst": e.dst,
                            "relation": e.relation, "weight": e.weight,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!({"nodes": nodes, "edges": edges})));
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

/// Caps graph-dump node content at 500 chars on a char boundary (payloads stay
/// light; the full node is one `memory.list` away). `char_indices` guarantees a
/// valid boundary — never a byte-slice panic on multi-byte UTF-8.
fn truncate_content(content: &str) -> String {
    match content.char_indices().nth(500) {
        Some((idx, _)) => content[..idx].to_string(),
        None => content.to_string(),
    }
}
