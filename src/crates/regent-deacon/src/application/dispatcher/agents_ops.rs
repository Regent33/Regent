//! `agents.*` handlers — named persistent agents (ADR-023).

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    pub(super) fn agents_list(&self, req: RpcRequest) {
        match self.sessions.agents_list() {
            Ok(agents) => {
                let items: Vec<_> = agents.iter().map(agent_json).collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn agents_show(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        match self.sessions.agents_show(name) {
            Ok(Some(a)) => self.send(ok_response(req.id, agent_json(&a))),
            Ok(None) => self.send(err_response(req.id, -32004, format!("no agent '{name}'"))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn agents_set(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        if name.trim().is_empty() || name.contains(char::is_whitespace) {
            self.send(err_response(
                req.id,
                -32602,
                "name must be a single non-empty word",
            ));
            return;
        }
        let get = |k: &str| req.params.get(k).and_then(|v| v.as_str());
        let description = get("description").unwrap_or("");
        let system_prompt = get("system_prompt").unwrap_or("");
        // Empty model/tools strings mean "unset" (inherit / full catalog).
        let model = get("model").filter(|s| !s.trim().is_empty());
        let tools = get("tools").filter(|s| !s.trim().is_empty());
        match self
            .sessions
            .agents_set(name, description, system_prompt, model, tools)
        {
            Ok(()) => self.send(ok_response(req.id, json!({ "ok": true }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn agents_remove(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        match self.sessions.agents_remove(name) {
            Ok(removed) => self.send(ok_response(req.id, json!({ "removed": removed }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}

/// JSON shape for one named agent (the `agents.*` surface).
fn agent_json(a: &regent_store::AgentRow) -> serde_json::Value {
    json!({
        "name": a.name,
        "description": a.description,
        "system_prompt": a.system_prompt,
        "model": a.model,
        "tools": a.tools,
        "created_at": a.created_at,
        "updated_at": a.updated_at,
    })
}
