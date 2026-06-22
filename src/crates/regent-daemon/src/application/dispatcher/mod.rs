//! JSON-RPC 2.0 method router. `handle` routes each method to a handler; the
//! handlers live in `session_ops` (session/turn lifecycle) and `admin_ops`
//! (model / cron / memory / skills / config). All responses and notifications
//! flow through the shared OutboundTx so sync and async methods use one path.

mod admin_ops;
mod session_ops;
mod voice_ops;

use crate::application::session_manager::SessionManager;
use crate::domain::config::DaemonConfig;
use crate::domain::contracts::OutboundTx;
use crate::domain::entities::{
    RpcNotification, RpcRequest, RpcResponse, err_response, ok_response,
};
use regent_cron::JobRepository;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct Dispatcher {
    sessions: Arc<SessionManager>,
    out_tx: OutboundTx,
    /// Cron job store (None until the composition root wires it).
    cron_repo: Option<Arc<dyn JobRepository>>,
    /// Loaded config snapshot for the `config.get` surface.
    config: Option<DaemonConfig>,
}

impl Dispatcher {
    #[must_use]
    pub fn new(sessions: Arc<SessionManager>, out_tx: OutboundTx) -> Self {
        Self {
            sessions,
            out_tx,
            cron_repo: None,
            config: None,
        }
    }

    #[must_use]
    pub fn with_cron(mut self, repo: Arc<dyn JobRepository>) -> Self {
        self.cron_repo = Some(repo);
        self
    }

    #[must_use]
    pub fn with_config(mut self, config: DaemonConfig) -> Self {
        self.config = Some(config);
        self
    }

    fn send(&self, resp: RpcResponse) {
        if let Ok(line) = serde_json::to_string(&resp) {
            self.out_tx.send(line).ok();
        }
    }

    fn notify(&self, method: &str, params: Value) {
        if let Ok(line) = serde_json::to_string(&RpcNotification::new(method, params)) {
            self.out_tx.send(line).ok();
        }
    }

    pub async fn handle(&self, req: RpcRequest) {
        match req.method.as_str() {
            "health" => self.send(ok_response(
                req.id,
                json!({"status": "ok", "version": "0.1.0"}),
            )),
            "version" => self.send(ok_response(req.id, json!({"version": "0.1.0"}))),
            "status.get" => self.status_get(req).await,
            "insights.get" => self.insights_get(req),
            "persona.get" => self.persona_get(req),
            "persona.set" => self.persona_set(req),
            "kanban.create" => self.kanban_create(req),
            "kanban.list" => self.kanban_list(req),
            "kanban.show" => self.kanban_show(req),
            "kanban.assign" => self.kanban_assign(req),
            "kanban.set_status" => self.kanban_set_status(req),
            "commands.list" => self.send(ok_response(req.id, commands_list())),
            "skills.list" => self.skills_list(req),
            "skills.view" => self.skills_view(req),
            "skills.create" => self.skills_create(req),
            "skills.opt_out" => self.skills_opt_out(req),
            "tools.list" => self.tools_list(req),
            "memory.pending" => self.memory_pending(req),
            "memory.approve" => self.memory_approve(req),
            "memory.reject" => self.memory_reject(req),
            "memory.list" => self.memory_list(req),
            "memory.pin" => self.memory_pin(req),
            "memory.unpin" => self.memory_unpin(req),
            "memory.forget" => self.memory_forget(req),
            "model.get" => self.model_get(req),
            "model.list" => self.model_list(req),
            "model.set" => self.model_set(req),
            "config.get" => self.config_get(req),
            "voice.status" => self.voice_status(req),
            "voice.models" => self.voice_models(req),
            "cron.list" => self.cron_list(req),
            "cron.add" => self.cron_add(req),
            "cron.remove" => self.cron_remove(req),
            "cron.set_enabled" => self.cron_set_enabled(req),
            "cron.run" => self.cron_run(req),
            "cron.edit" => self.cron_edit(req),
            "session.create" => self.session_create(req).await,
            "session.resume" => self.session_resume(req).await,
            "session.list" => self.session_list(req),
            "session.search" => self.session_search(req),
            "prompt.submit" => self.prompt_submit(req),
            "turn.interrupt" => self.turn_interrupt(req).await,
            "approval.respond" => self.approval_respond(req).await,
            method => {
                self.send(err_response(
                    req.id,
                    -32601,
                    format!("method not found: {method}"),
                ));
            }
        }
    }
}

/// Known Claude models offered by `model.list` (id, display name). `model.set`
/// accepts any string, so custom/self-hosted ids still work — this is the
/// menu, not an allowlist.
pub(super) fn model_catalog() -> &'static [(&'static str, &'static str)] {
    &[
        ("claude-fable-5", "Claude Fable 5"),
        ("claude-opus-4-8", "Claude Opus 4.8"),
        ("claude-sonnet-4-6", "Claude Sonnet 4.6"),
        ("claude-haiku-4-5", "Claude Haiku 4.5"),
    ]
}

fn commands_list() -> Value {
    json!([
        {"name": "help",    "description": "Show available commands"},
        {"name": "new",     "description": "Reset the current session"},
        {"name": "stop",    "description": "Interrupt the running turn"},
        {"name": "approve", "description": "Approve a pending dangerous action"},
        {"name": "deny",    "description": "Deny a pending dangerous action"},
    ])
}
