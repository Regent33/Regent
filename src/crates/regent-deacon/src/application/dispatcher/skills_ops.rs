//! `skills.*` handlers — list/view/create plus the archive (opt-out/in) toggle.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    pub(super) fn skills_list(&self, req: RpcRequest) {
        match self.sessions.skills_list() {
            Ok(skills) => {
                let mut items: Vec<_> = skills
                    .iter()
                    .map(|s| json!({"name": s.name, "description": s.description, "tags": s.tags}))
                    .collect();
                // Additive: `include_archived: true` appends retired skills,
                // each flagged — so the opt-out toggle can be flipped back on.
                if req.params.get("include_archived").and_then(|v| v.as_bool()) == Some(true) {
                    match self.sessions.skills_list_archived() {
                        Ok(archived) => items.extend(archived.iter().map(|s| {
                            json!({
                                "name": s.name, "description": s.description,
                                "tags": s.tags, "archived": true,
                            })
                        })),
                        Err(e) => {
                            self.send(err_response(req.id, -32000, e.to_string()));
                            return;
                        }
                    }
                }
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn skills_view(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        match self.sessions.skill_view(name) {
            Ok(rec) => self.send(ok_response(
                req.id,
                json!({
                    "name": rec.meta.name, "description": rec.meta.description,
                    "tags": rec.meta.tags, "body": rec.body, "files": rec.files,
                }),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn skills_create(&self, req: RpcRequest) {
        let (name, description, body) = match (
            req.params.get("name").and_then(|v| v.as_str()),
            req.params.get("description").and_then(|v| v.as_str()),
            req.params.get("body").and_then(|v| v.as_str()),
        ) {
            (Some(n), Some(d), Some(b)) => (n, d, b),
            _ => {
                self.send(err_response(
                    req.id,
                    -32602,
                    "skills.create needs name, description, body",
                ));
                return;
            }
        };
        match self.sessions.skill_create(name, description, body) {
            Ok(()) => self.send(ok_response(req.id, json!({"created": name}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn skills_opt_out(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        match self.sessions.skill_archive(name) {
            Ok(()) => self.send(ok_response(req.id, json!({"archived": name}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// Inverse of `skills.opt_out`: restore a previously archived skill.
    pub(super) fn skills_opt_in(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        match self.sessions.skill_unarchive(name) {
            Ok(()) => self.send(ok_response(req.id, json!({"restored": name}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}
