//! `persona.*` handlers — the DB-backed soul / user profile.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    pub(super) fn persona_get(&self, req: RpcRequest) {
        let key = req
            .params
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("soul");
        match self.sessions.persona_get(key) {
            Ok(content) => self.send(ok_response(
                req.id,
                json!({ "key": key, "content": content }),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn persona_set(&self, req: RpcRequest) {
        let key = req.params.get("key").and_then(|v| v.as_str()).unwrap_or("");
        let content = req
            .params
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !regent_store::is_valid_persona_key(key) {
            self.send(err_response(
                req.id,
                -32602,
                "key must be 'soul', 'about', or 'about.<identity|preferences|habits|constraints|goals>'",
            ));
            return;
        }
        match self.sessions.persona_set(key, content) {
            Ok(()) => self.send(ok_response(req.id, json!({ "ok": true }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}
