//! The in-process `regent` admin tool's dispatch path: `install_admin` wires the
//! self-handle + deps, and `run_admin_command` runs one daemon RPC through a
//! throwaway dispatcher over a local channel — no second daemon, no store
//! deadlock. Turn/session-driving methods are refused.

use super::SessionManager;
use crate::application::dispatcher::Dispatcher;
use crate::domain::entities::RpcRequest;
use regent_cron::JobRepository;
use regent_speech::HttpExecutor;
use serde_json::{Value, json};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// Extra dependencies the in-process `regent` admin tool needs to build a
/// dispatcher (cron jobs, the config snapshot, the speech executor). Installed
/// once at the composition root via [`SessionManager::install_admin`].
#[derive(Default)]
pub struct AdminDeps {
    pub cron: Option<Arc<dyn JobRepository>>,
    pub config: Option<crate::domain::config::DaemonConfig>,
    pub speech: Option<Arc<dyn HttpExecutor>>,
}

impl SessionManager {
    /// Installs the self-handle + admin deps so the in-process `regent` tool can
    /// route commands through this manager's dispatcher. Composition root only;
    /// idempotent (a second call is ignored).
    pub fn install_admin(self: &Arc<Self>, deps: AdminDeps) {
        let _ = self.self_ref.set(Arc::downgrade(self));
        let _ = self.admin.set(deps);
    }

    /// Runs one admin command (a daemon RPC `method` + `params`) in-process by
    /// dispatching it through a throwaway [`Dispatcher`] over a local channel —
    /// no second daemon, no store deadlock. Turn/session-lifecycle methods are
    /// refused (the agent must not drive its own live turn). Returns the RPC
    /// `result` value, or the dispatcher's error message.
    pub async fn run_admin_command(&self, method: &str, params: Value) -> Result<Value, String> {
        // These drive the live turn/session loop — self-running them recurses or
        // corrupts the in-flight session, so they're off-limits to the agent.
        const DENY: &[&str] = &[
            "session.create",
            "session.resume",
            "prompt.submit",
            "code.plan",
            "code.start",
            "turn.interrupt",
            "approval.respond",
        ];
        if DENY.contains(&method) {
            return Err(format!(
                "'{method}' drives the live turn/session and can't be run from here"
            ));
        }
        let Some(this) = self.self_ref.get().and_then(Weak::upgrade) else {
            return Err("admin dispatcher is not installed".to_owned());
        };
        let deps = self.admin.get();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let mut dispatcher = Dispatcher::new(this, tx);
        if let Some(cron) = deps.and_then(|d| d.cron.clone()) {
            dispatcher = dispatcher.with_cron(cron);
        }
        if let Some(config) = deps.and_then(|d| d.config.clone()) {
            dispatcher = dispatcher.with_config(config);
        }
        if let Some(speech) = deps.and_then(|d| d.speech.clone()) {
            dispatcher = dispatcher.with_speech_executor(speech);
        }
        let request = RpcRequest {
            jsonrpc: "2.0".to_owned(),
            method: method.to_owned(),
            params,
            id: Some(json!(1)),
        };
        // Some handlers stream progress notifications before the final response;
        // skip lines without an `id` (notifications) and take the response.
        let drive = async {
            dispatcher.handle(request).await;
            while let Some(line) = rx.recv().await {
                let value: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if value.get("id").is_none() {
                    continue; // a notification — not our response
                }
                if let Some(result) = value.get("result") {
                    return Ok(result.clone());
                }
                let message = value
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("admin command failed")
                    .to_owned();
                return Err(message);
            }
            Err("no response from dispatcher".to_owned())
        };
        match tokio::time::timeout(Duration::from_secs(120), drive).await {
            Ok(outcome) => outcome,
            Err(_) => Err(format!("'{method}' timed out")),
        }
    }
}
