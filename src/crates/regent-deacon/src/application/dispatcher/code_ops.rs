//! Coding-harness handlers: `code.plan` (read-only → PLAN) and `code.start`
//! (snapshot → execute the approved plan → verify → revert-on-fail). The execute
//! turn streams + approves through the same session path as `prompt.submit`.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    pub(super) async fn code_plan(&self, req: RpcRequest) {
        let Some(task) = req.params.get("task").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing task"));
            return;
        };
        match self.sessions.code_plan(task).await {
            Ok((session_id, plan)) => self.send(ok_response(
                req.id,
                json!({"session_id": session_id.to_string(), "plan": plan}),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) async fn code_start(&self, req: RpcRequest) {
        let Some(task) = req.params.get("task").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing task"));
            return;
        };
        let Some(plan) = req.params.get("plan").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing plan"));
            return;
        };
        match self.sessions.code_start(task, plan).await {
            Ok(result) => {
                let verify = result
                    .verify
                    .map(|v| json!({"passed": v.passed, "summary": v.summary}));
                self.send(ok_response(
                    req.id,
                    json!({
                        "session_id": result.session_id.to_string(),
                        "report": result.report,
                        "verify": verify,
                        "reverted": result.reverted,
                    }),
                ));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}
