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
use crate::domain::entities::{
    RpcNotification, RpcRequest, RpcResponse, err_response, ok_response,
};
use regent_cron::JobRepository;
use regent_speech::HttpExecutor;
use serde_json::{Value, json};
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

    /// Config snapshot for read handlers (clone-out keeps handler code simple).
    pub(super) fn config_snapshot(&self) -> Option<DeaconConfig> {
        self.config.read().unwrap().clone()
    }

    /// Refresh the snapshot + fire the live-reload hook (config.set/env.set).
    /// Bumping the routing epoch marks every OPEN session's provider stale, so
    /// the change reaches their next turn — not just new sessions.
    pub(super) fn apply_config(&self, config: DeaconConfig) {
        if let Some(reload) = &self.reload {
            reload(&config);
        }
        *self.config.write().unwrap() = Some(config);
        self.sessions.bump_routing();
    }

    /// Re-fires the reload hook with the CURRENT config — used by `env.set`,
    /// where the config file didn't change but key resolution must re-run.
    pub(super) fn reapply_config(&self) {
        if let (Some(reload), Some(cfg)) = (&self.reload, self.config.read().unwrap().as_ref()) {
            reload(cfg);
        }
        self.sessions.bump_routing();
    }

    #[must_use]
    pub fn with_reload(mut self, reload: ConfigReload) -> Self {
        self.reload = Some(reload);
        self
    }

    #[must_use]
    pub fn with_cron(mut self, repo: Arc<dyn JobRepository>) -> Self {
        self.cron_repo = Some(repo);
        self
    }

    #[must_use]
    pub fn with_config(mut self, config: DeaconConfig) -> Self {
        self.config = RwLock::new(Some(config));
        self
    }

    #[must_use]
    pub fn with_speech_executor(mut self, exec: Arc<dyn HttpExecutor>) -> Self {
        self.speech_exec = Some(exec);
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
            "memory.pin" => self.memory_pin(req),
            "memory.unpin" => self.memory_unpin(req),
            "memory.forget" => self.memory_forget(req),
            "model.get" => self.model_get(req),
            "model.list" => self.model_list(req),
            "model.set" => self.model_set(req),
            "providers.list" => self.providers_list(req),
            "providers.models" => self.providers_models(req),
            "providers.test" => self.providers_test(req).await,
            "mom.run" => self.mom_run(req).await,
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
            "prompt.submit" => self.prompt_submit(req),
            "attachment.put" => self.attachment_put(req),
            "artifacts.list" => self.artifacts_list(req),
            "artifacts.get" => self.artifacts_get(req),
            "code.plan" => self.code_plan(req).await,
            "code.start" => self.code_start(req).await,
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

/// The in-chat `/` slash menu, mirrored from the CLI's slash surface
/// (`src/regent-cli/src/app/config/commands.ts::SLASH_COMMANDS`) so the desktop
/// advertises the same set the terminal does. Each row carries an additive
/// `executable` flag: `true` when the deacon has a JSON-RPC path that fulfils
/// the command (the desktop can run it), `false` for controls the UI handles
/// locally or terminal-only tools it can only explain — so the UI routes or
/// explains instead of firing an RPC that would fail. Extra fields are ignored
/// by older clients (they read only `name`/`description`).
fn commands_list() -> Value {
    json!([
        // Chat controls.
        {"name": "help",      "description": "List commands and usage",              "executable": false},
        {"name": "new",       "description": "Start a fresh conversation",           "executable": true},
        {"name": "clear",     "description": "Clear the conversation",               "executable": false},
        {"name": "stop",      "description": "Interrupt the running turn",           "executable": true},
        {"name": "approve",   "description": "Approve the pending action",           "executable": true},
        {"name": "deny",      "description": "Deny the pending action",              "executable": true},
        // Session / knowledge.
        {"name": "status",    "description": "Agent + provider status",              "executable": true},
        {"name": "sessions",  "description": "List or resume sessions",              "executable": true},
        {"name": "memory",    "description": "Browse and manage memory",             "executable": true},
        {"name": "learn",     "description": "Teach Regent a new skill",             "executable": true},
        {"name": "skills",    "description": "List available skills",                "executable": true},
        {"name": "insights",  "description": "Show usage insights",                  "executable": true},
        // Board.
        {"name": "kanban",    "description": "View and manage the board",            "executable": true},
        {"name": "agents",    "description": "Manage named persistent agents",       "executable": true},
        // Model / tools / providers.
        {"name": "model",     "description": "Show or set the model",                "executable": true},
        {"name": "providers", "description": "Manage model providers",               "executable": true},
        {"name": "tools",     "description": "List or toggle tools",                 "executable": true},
        {"name": "keys",      "description": "Manage provider API keys",             "executable": true},
        // Persona.
        {"name": "persona",   "description": "Show persona (soul + about)",          "executable": true},
        {"name": "soul",      "description": "Show or edit the soul",                "executable": true},
        {"name": "about",     "description": "Show or edit the about",               "executable": true},
        // Config / ops.
        {"name": "config",    "description": "Show configuration",                   "executable": true},
        {"name": "voice",     "description": "Voice (ASR/TTS): setup, enable, status", "executable": true},
        {"name": "cron",      "description": "Schedule recurring tasks",             "executable": true},
        {"name": "version",   "description": "Show the version",                     "executable": true},
        // No deacon RPC path — UI must route to a terminal or explain.
        {"name": "profile",   "description": "Switch or manage profiles",            "executable": false},
        {"name": "gateway",   "description": "Start/stop the messaging gateway",     "executable": false},
        {"name": "auth",      "description": "Manage gateway authorization",         "executable": false},
        {"name": "logs",      "description": "Tail deacon logs",                     "executable": false},
        {"name": "doctor",    "description": "Diagnose configuration",               "executable": false},
        {"name": "security",  "description": "Review security settings",             "executable": false},
    ])
}
