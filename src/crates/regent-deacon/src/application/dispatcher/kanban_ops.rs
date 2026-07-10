//! `kanban.*` handlers — the shared task board.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    pub(super) fn kanban_create(&self, req: RpcRequest) {
        let title = req
            .params
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if title.is_empty() {
            self.send(err_response(req.id, -32602, "missing title"));
            return;
        }
        let body = req
            .params
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match self.sessions.kanban_create(title, body) {
            Ok(id) => self.send(ok_response(req.id, json!({ "id": id }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn kanban_list(&self, req: RpcRequest) {
        let status = req.params.get("status").and_then(|v| v.as_str());
        match self.sessions.kanban_list(status) {
            Ok(tasks) => {
                let items: Vec<_> = tasks.iter().map(task_json).collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn kanban_show(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.kanban_show(id) {
            Ok(Some(task)) => self.send(ok_response(req.id, task_json(&task))),
            Ok(None) => self.send(err_response(req.id, -32004, format!("no task {id}"))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn kanban_assign(&self, req: RpcRequest) {
        let (Some(id), Some(worker)) = (
            req.params.get("id").and_then(|v| v.as_str()),
            req.params.get("worker").and_then(|v| v.as_str()),
        ) else {
            self.send(err_response(req.id, -32602, "missing id or worker"));
            return;
        };
        match self.sessions.kanban_assign(id, worker) {
            Ok(claimed) => self.send(ok_response(req.id, json!({ "claimed": claimed }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn kanban_set_status(&self, req: RpcRequest) {
        let (Some(id), Some(status)) = (
            req.params.get("id").and_then(|v| v.as_str()),
            req.params.get("status").and_then(|v| v.as_str()),
        ) else {
            self.send(err_response(req.id, -32602, "missing id or status"));
            return;
        };
        match self.sessions.kanban_set_status(id, status) {
            Ok(ok) => self.send(ok_response(req.id, json!({ "ok": ok }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}

/// JSON shape for one kanban task (the `kanban.*` surface).
fn task_json(t: &regent_store::KanbanTaskRow) -> serde_json::Value {
    json!({
        "id": t.id,
        "board": t.board,
        "title": t.title,
        "description": t.description,
        "status": t.status,
        "assignee": t.assignee,
        "created_at": t.created_at,
        "updated_at": t.updated_at,
    })
}
