//! Introspection handlers: `status.get`, `insights.get`, `config.get`, and the
//! tool catalog listing.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    /// Aggregate health/state snapshot for `regent status`: active model, live
    /// in-memory sessions, and a cron summary (jobs, enabled, soonest run).
    pub(super) async fn status_get(&self, req: RpcRequest) {
        let active_sessions = self.sessions.active_sessions().await;
        let model = self.sessions.model();
        let cron = match &self.cron_repo {
            Some(repo) => match repo.load() {
                Ok(jobs) => {
                    let enabled = jobs.iter().filter(|j| j.enabled).count();
                    // next_run_at is f64 (not Ord) → fold with f64::min.
                    let next_run_at = jobs
                        .iter()
                        .filter(|j| j.enabled)
                        .map(|j| j.next_run_at)
                        .fold(None::<f64>, |acc, t| Some(acc.map_or(t, |a| a.min(t))));
                    json!({"jobs": jobs.len(), "enabled": enabled, "next_run_at": next_run_at})
                }
                Err(_) => json!(null),
            },
            None => json!(null),
        };
        self.send(ok_response(
            req.id,
            json!({"model": model, "active_sessions": active_sessions, "cron": cron}),
        ));
    }

    /// Aggregate usage rollup across every session + the turns ledger.
    pub(super) fn insights_get(&self, req: RpcRequest) {
        match self.sessions.insights() {
            Ok(r) => self.send(ok_response(
                req.id,
                json!({
                    "sessions": r.sessions,
                    "turns": r.turns,
                    "turns_ok": r.turns_ok,
                    "input_tokens": r.input_tokens,
                    "output_tokens": r.output_tokens,
                    "api_calls": r.api_calls,
                    "messages": r.messages,
                }),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn config_get(&self, req: RpcRequest) {
        match self.config_snapshot() {
            Some(cfg) => match serde_json::to_value(cfg) {
                Ok(v) => self.send(ok_response(req.id, v)),
                Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
            },
            None => self.send(err_response(req.id, -32000, "config not wired")),
        }
    }

    /// Lists the core tool catalog with each tool's enabled state (a tool is
    /// disabled when its name is in config `tools.disabled`).
    pub(super) async fn tools_list(&self, req: RpcRequest) {
        // The FULL session catalog (core + memory/skills/kanban/persona/keys/
        // delegate/message/regent/browser/…), not just the bare core set — so
        // `regent tools list` and the welcome panel show everything the agent has.
        let defs = match self.sessions.list_tool_definitions().await {
            Ok(defs) => defs,
            Err(e) => {
                self.send(err_response(req.id, -32000, e.to_string()));
                return;
            }
        };
        let disabled = self
            .config_snapshot()
            .map(|c| c.tools.disabled)
            .unwrap_or_default();
        let items: Vec<_> = defs
            .iter()
            .map(|d| {
                json!({
                    "name": d.name, "description": d.description, "toolset": d.toolset,
                    "enabled": !disabled.iter().any(|n| n == &d.name),
                })
            })
            .collect();
        self.send(ok_response(req.id, json!(items)));
    }
}
