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

    /// `profile.list` → `{profiles: [..], active: name}`.
    pub(super) fn profile_list(&self, req: RpcRequest) {
        match self.sessions.profile_list() {
            Ok((profiles, active)) => self.send(ok_response(
                req.id,
                json!({ "profiles": profiles, "active": active }),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// `profile.create {name}` — new empty profile; does not switch to it.
    pub(super) fn profile_create(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "profile.create needs 'name'"));
            return;
        };
        match self.sessions.profile_create(name) {
            Ok(()) => self.send(ok_response(req.id, json!({ "ok": true, "name": name }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// `profile.switch {name}` — takes effect for sessions built afterwards.
    pub(super) fn profile_switch(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "profile.switch needs 'name'"));
            return;
        };
        match self.sessions.profile_switch(name) {
            Ok(()) => self.send(ok_response(req.id, json!({ "ok": true, "active": name }))),
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
