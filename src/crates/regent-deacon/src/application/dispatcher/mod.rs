//! JSON-RPC 2.0 method router. `handle` routes each method to a handler; the
//! handlers live in one `*_ops` module per feature (sessions, model/providers,
//! cron, memory, skills, voice, config, …). All responses and notifications
//! flow through the shared OutboundTx so sync and async methods use one path.

mod agents_ops;
mod artifacts_ops;
mod attachment_ops;
mod code_ops;
mod config_ops;
mod cron_edit_ops;
mod cron_ops;
mod env_ops;
mod kanban_ops;
mod memory_ops;
mod model_ops;
mod mom_ops;
mod persona_ops;
mod prompt_ops;
mod providers_ops;
mod session_admin_ops;
mod session_ops;
mod skills_ops;
mod speech_yaml;
mod status_ops;
mod voice_ops;
mod voice_set_ops;
mod voice_weights_ops;

use crate::application::session_manager::SessionManager;
use crate::domain::config::DeaconConfig;
use crate::domain::contracts::OutboundTx;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_cron::JobRepository;
use regent_speech::HttpExecutor;
use serde_json::json;
use std::sync::{Arc, RwLock};

/// Called with the freshly-validated config after every successful
/// `config.set` / `env.set` so runtime routing (provider registry, chain,
/// keys) applies changes to the NEXT session without a restart.
pub type ConfigReload = Arc<dyn Fn(&DeaconConfig) + Send + Sync>;

pub struct Dispatcher {
    sessions: Arc<SessionManager>,
    out_tx: OutboundTx,
    /// Cron job store (None until the composition root wires it).
    cron_repo: Option<Arc<dyn JobRepository>>,
    /// Loaded config snapshot for the `config.get` surface — refreshed in
    /// place by `config.set`, so reads never go stale mid-process.
    config: RwLock<Option<DeaconConfig>>,
    /// HTTP executor for the speech backends (None until wired); enables
    /// `voice.test` and the live transcribe/synthesize path.
    speech_exec: Option<Arc<dyn HttpExecutor>>,
    /// Live-reload hook (None until the composition root wires it).
    reload: Option<ConfigReload>,
}

impl Dispatcher {
    #[must_use]
    pub fn new(sessions: Arc<SessionManager>, out_tx: OutboundTx) -> Self {
        Self {
            sessions,
            out_tx,
            cron_repo: None,
            config: RwLock::new(None),
            speech_exec: None,
            reload: None,
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
            "agents.list" => self.agents_list(req),
            "agents.show" => self.agents_show(req),
            "agents.set" => self.agents_set(req),
            "agents.remove" => self.agents_remove(req),
            "commands.list" => self.send(ok_response(req.id, commands_list())),
            "skills.list" => self.skills_list(req),
            "skills.view" => self.skills_view(req),
            "skills.create" => self.skills_create(req),
            "skills.opt_out" => self.skills_opt_out(req),
            "skills.opt_in" => self.skills_opt_in(req),
            "tools.list" => self.tools_list(req).await,
            "memory.pending" => self.memory_pending(req),
            "memory.approve" => self.memory_approve(req),
            "memory.reject" => self.memory_reject(req),
            "memory.list" => self.memory_list(req),
            "memory.graph" => self.memory_graph(req),
            "memory.pin" => self.memory_pin(req),
            "memory.unpin" => self.memory_unpin(req),
            "memory.forget" => self.memory_forget(req),
            "model.get" => self.model_get(req),
            "model.list" => self.model_list(req),
            "model.set" => self.model_set(req),
            "providers.catalog" => self.providers_catalog(req),
            "providers.list" => self.providers_list(req),
            "providers.models" => self.providers_models(req),
            "providers.test" => self.providers_test(req),
            "mom.run" => self.mom_run(req),
            "config.get" => self.config_get(req),
            "config.set" => self.config_set(req),
            "env.list" => self.env_list(req),
            "env.set" => self.env_set(req),
            "env.unset" => self.env_unset(req),
            "env.activate" => self.env_activate(req),
            "voice.status" => self.voice_status(req),
            "voice.models" => self.voice_models(req),
            "voice.set" => self.voice_set(req),
            "voice.ensure_models" => self.voice_ensure_models(req).await,
            "voice.test" => self.voice_test(req).await,
            "cron.list" => self.cron_list(req),
            "cron.add" => self.cron_add(req),
            "cron.remove" => self.cron_remove(req),
            "cron.set_enabled" => self.cron_set_enabled(req),
            "cron.run" => self.cron_run(req),
            "cron.edit" => self.cron_edit(req),
            "session.create" => self.session_create(req).await,
            "session.resume" => self.session_resume(req).await,
            "session.list" => self.session_list(req),
            "session.history" => self.session_history(req),
            "session.search" => self.session_search(req),
            "session.rename" => self.session_rename(req),
            "session.pin" => self.session_pin(req),
            "session.archive" => self.session_archive(req),
            "session.delete" => self.session_delete(req),
            "session.backfill_titles" => self.session_backfill_titles(req),
            "context.budget" => self.context_budget(req).await,
            "prompt.submit" => self.prompt_submit(req),
            "attachment.put" => self.attachment_put(req),
            "artifacts.list" => self.artifacts_list(req),
            "artifacts.get" => self.artifacts_get(req),
            "code.plan" => self.code_plan(req),
            "code.start" => self.code_start(req),
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

use catalog_data::commands_list;
pub(super) use catalog_data::model_catalog;

mod catalog_data;
mod wiring;
