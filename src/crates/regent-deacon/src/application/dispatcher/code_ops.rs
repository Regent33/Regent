//! Coding-harness handlers: `code.plan` (read-only → PLAN) and `code.start`
//! (snapshot → execute the approved plan → verify → revert-on-fail). The execute
//! turn streams + approves through the same session path as `prompt.submit` —
//! and like it, BOTH run DETACHED: the dispatcher's read loop is serial, so an
//! awaited multi-minute code run queued every other request behind it (Stop
//! generating, chat turns, settings — the whole app froze until it finished).
//! The response still carries the original request id, sent when the run ends;
//! stdio JSON-RPC responses are matched by id, not order.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;
use std::sync::Arc;

impl Dispatcher {
    pub(super) fn code_plan(&self, req: RpcRequest) {
        let Some(task) = req.params.get("task").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing task"));
            return;
        };
        let task = task.to_owned();
        let skill = req
            .params
            .get("skill")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let sessions = Arc::clone(&self.sessions);
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let resp = match sessions.code_plan(&task, skill.as_deref()).await {
                Ok((session_id, plan)) => ok_response(
                    req.id,
                    json!({"session_id": session_id.to_string(), "plan": plan}),
                ),
                Err(e) => err_response(req.id, -32000, e.to_string()),
            };
            if let Ok(line) = serde_json::to_string(&resp) {
                out_tx.send(line).ok();
            }
        });
    }

    pub(super) fn code_start(&self, req: RpcRequest) {
        let Some(task) = req.params.get("task").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing task"));
            return;
        };
        let Some(plan) = req.params.get("plan").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing plan"));
            return;
        };
        let (task, plan) = (task.to_owned(), plan.to_owned());
        let skill = req
            .params
            .get("skill")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        // Wave 3e: review skill names — a string or an array of strings.
        let review: Vec<String> = match req.params.get("review") {
            Some(serde_json::Value::String(one)) => vec![one.clone()],
            Some(serde_json::Value::Array(many)) => many
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        };
        let sessions = Arc::clone(&self.sessions);
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let resp = match sessions
                .code_start(&task, &plan, skill.as_deref(), &review)
                .await
            {
                Ok(result) => {
                    let verify = result
                        .verify
                        .map(|v| json!({"passed": v.passed, "summary": v.summary}));
                    ok_response(
                        req.id,
                        json!({
                            "session_id": result.session_id.to_string(),
                            "report": result.report,
                            "verify": verify,
                            "fix_attempts": result.fix_attempts,
                            "reverted": result.reverted,
                        }),
                    )
                }
                Err(e) => err_response(req.id, -32000, e.to_string()),
            };
            if let Ok(line) = serde_json::to_string(&resp) {
                out_tx.send(line).ok();
            }
        });
    }
}
