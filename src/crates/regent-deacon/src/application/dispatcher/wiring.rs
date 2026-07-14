//! Dispatcher wiring: config snapshot/apply and the `with_*` builders.
//! Split from `dispatcher/mod.rs` (file-size rule).

use super::{ConfigReload, Dispatcher};
use crate::domain::config::DeaconConfig;
use crate::domain::entities::{RpcNotification, RpcResponse};
use regent_cron::JobRepository;
use regent_speech::HttpExecutor;
use serde_json::Value;
use std::sync::Arc;
use std::sync::RwLock;

impl Dispatcher {
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
        // Auto mode is a live flag shared by every session's approval handler
        // — store it here so `config.set tools.auto_approve` applies to open
        // sessions immediately, no restart or new session needed.
        self.sessions.set_auto_approve(config.tools.auto_approve);
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

    pub(super) fn send(&self, resp: RpcResponse) {
        if let Ok(line) = serde_json::to_string(&resp) {
            self.out_tx.send(line).ok();
        }
    }

    pub(super) fn notify(&self, method: &str, params: Value) {
        if let Ok(line) = serde_json::to_string(&RpcNotification::new(method, params)) {
            self.out_tx.send(line).ok();
        }
    }
}
