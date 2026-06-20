//! Admin/query handlers: skills, model catalog/switch, config, cron CRUD, and
//! the memory write-approval surface.

use super::{Dispatcher, model_catalog};
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_cron::{CronJob, Schedule};
use serde_json::json;

impl Dispatcher {
    pub(super) fn skills_list(&self, req: RpcRequest) {
        match self.sessions.skills_list() {
            Ok(skills) => {
                let items: Vec<_> = skills
                    .iter()
                    .map(|s| json!({"name": s.name, "description": s.description, "tags": s.tags}))
                    .collect();
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
                self.send(err_response(req.id, -32602, "skills.create needs name, description, body"));
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

    pub(super) fn memory_pending(&self, req: RpcRequest) {
        let limit = req.params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as u32;
        match self.sessions.pending_memory_writes(limit) {
            Ok(writes) => {
                let items: Vec<_> = writes
                    .iter()
                    .map(|w| {
                        json!({
                            "id": w.id, "kind": w.kind, "name": w.name, "content": w.content,
                            "provenance": w.provenance, "trust": w.trust, "created_at": w.created_at,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_approve(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.approve_memory_write(id) {
            Ok(node_id) => self.send(ok_response(
                req.id,
                json!({"approved": node_id.is_some(), "node_id": node_id}),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_reject(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.reject_memory_write(id) {
            Ok(removed) => self.send(ok_response(req.id, json!({"removed": removed}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_list(&self, req: RpcRequest) {
        let limit = req.params.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
        match self.sessions.list_memory(limit) {
            Ok(nodes) => {
                let items: Vec<_> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "id": n.id, "kind": n.kind, "name": n.name,
                            "content": n.content, "pinned": n.pinned,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_pin(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.pin_memory(id) {
            Ok(found) => self.send(ok_response(req.id, json!({"found": found, "pinned": true}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_unpin(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.unpin_memory(id) {
            Ok(found) => self.send(ok_response(req.id, json!({"found": found, "pinned": false}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_forget(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.forget_memory(id) {
            Ok(forgotten) => self.send(ok_response(req.id, json!({"forgotten": forgotten}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn model_get(&self, req: RpcRequest) {
        self.send(ok_response(req.id, json!({"model": self.sessions.model()})));
    }

    pub(super) fn model_list(&self, req: RpcRequest) {
        let active = self.sessions.model();
        let items: Vec<_> = model_catalog()
            .iter()
            .map(|(id, label)| json!({"id": id, "display_name": label, "current": *id == active}))
            .collect();
        self.send(ok_response(req.id, json!(items)));
    }

    pub(super) fn model_set(&self, req: RpcRequest) {
        let Some(model) = req.params.get("model").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing model"));
            return;
        };
        self.sessions.set_model(model);
        self.send(ok_response(
            req.id,
            json!({
                "model": model,
                "note": "applies to new sessions; existing sessions keep their model",
            }),
        ));
    }

    pub(super) fn config_get(&self, req: RpcRequest) {
        match &self.config {
            Some(cfg) => match serde_json::to_value(cfg) {
                Ok(v) => self.send(ok_response(req.id, v)),
                Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
            },
            None => self.send(err_response(req.id, -32000, "config not wired")),
        }
    }

    pub(super) fn cron_list(&self, req: RpcRequest) {
        let Some(repo) = &self.cron_repo else {
            self.send(err_response(req.id, -32000, "cron not wired"));
            return;
        };
        match repo.load() {
            Ok(jobs) => {
                let items: Vec<_> = jobs
                    .iter()
                    .map(|j| {
                        json!({
                            "id": j.id, "name": j.name, "prompt": j.prompt,
                            "enabled": j.enabled, "next_run_at": j.next_run_at,
                            "last_run_at": j.last_run_at,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn cron_add(&self, req: RpcRequest) {
        let Some(repo) = &self.cron_repo else {
            self.send(err_response(req.id, -32000, "cron not wired"));
            return;
        };
        let (name, schedule_raw, prompt) = match (
            req.params.get("name").and_then(|v| v.as_str()),
            req.params.get("schedule").and_then(|v| v.as_str()),
            req.params.get("prompt").and_then(|v| v.as_str()),
        ) {
            (Some(n), Some(s), Some(p)) => (n, s, p),
            _ => {
                self.send(err_response(req.id, -32602, "cron.add needs name, schedule, prompt"));
                return;
            }
        };
        let schedule = match Schedule::parse(schedule_raw) {
            Ok(s) => s,
            Err(e) => {
                self.send(err_response(req.id, -32602, e.to_string()));
                return;
            }
        };
        let job = match CronJob::new(name, schedule, prompt, regent_store::now_epoch()) {
            Ok(j) => j,
            Err(e) => {
                self.send(err_response(req.id, -32602, e.to_string()));
                return;
            }
        };
        let result = repo.load().and_then(|mut jobs| {
            jobs.push(job.clone());
            repo.save(&jobs)
        });
        match result {
            Ok(()) => self.send(ok_response(req.id, json!({"id": job.id}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn cron_remove(&self, req: RpcRequest) {
        let Some(repo) = &self.cron_repo else {
            self.send(err_response(req.id, -32000, "cron not wired"));
            return;
        };
        let Some(job_id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        let result = repo.load().and_then(|mut jobs| {
            let before = jobs.len();
            jobs.retain(|j| j.id != job_id);
            let removed = jobs.len() < before;
            repo.save(&jobs).map(|()| removed)
        });
        match result {
            Ok(removed) => self.send(ok_response(req.id, json!({"removed": removed}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// Enable/disable a job. Re-enabling recomputes `next_run_at` from now so a
    /// long-paused job doesn't fire immediately on resume.
    pub(super) fn cron_set_enabled(&self, req: RpcRequest) {
        let Some(repo) = &self.cron_repo else {
            self.send(err_response(req.id, -32000, "cron not wired"));
            return;
        };
        let Some(job_id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        let enabled = req.params.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        let now = regent_store::now_epoch();
        let result = repo.load().and_then(|mut jobs| {
            let mut found = false;
            for j in &mut jobs {
                if j.id == job_id {
                    j.enabled = enabled;
                    if enabled && let Some(next) = j.schedule.next_after(now) {
                        j.next_run_at = next;
                    }
                    found = true;
                    break;
                }
            }
            repo.save(&jobs).map(|()| found)
        });
        match result {
            Ok(found) => {
                self.send(ok_response(req.id, json!({"id": job_id, "enabled": enabled, "found": found})));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// Mark a job due now (and enabled) so the next scheduler tick runs it.
    pub(super) fn cron_run(&self, req: RpcRequest) {
        let Some(repo) = &self.cron_repo else {
            self.send(err_response(req.id, -32000, "cron not wired"));
            return;
        };
        let Some(job_id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        let now = regent_store::now_epoch();
        let result = repo.load().and_then(|mut jobs| {
            let mut found = false;
            for j in &mut jobs {
                if j.id == job_id {
                    j.enabled = true;
                    j.next_run_at = now;
                    found = true;
                    break;
                }
            }
            repo.save(&jobs).map(|()| found)
        });
        match result {
            Ok(queued) => self.send(ok_response(req.id, json!({"queued": queued}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// Edit a job's name/schedule/prompt; a new schedule recomputes `next_run_at`.
    pub(super) fn cron_edit(&self, req: RpcRequest) {
        let Some(repo) = &self.cron_repo else {
            self.send(err_response(req.id, -32000, "cron not wired"));
            return;
        };
        let Some(job_id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        let new_name = req.params.get("name").and_then(|v| v.as_str()).map(str::to_owned);
        let new_prompt = req.params.get("prompt").and_then(|v| v.as_str()).map(str::to_owned);
        let new_schedule = match req.params.get("schedule").and_then(|v| v.as_str()) {
            Some(raw) => match Schedule::parse(raw) {
                Ok(s) => Some(s),
                Err(e) => {
                    self.send(err_response(req.id, -32602, e.to_string()));
                    return;
                }
            },
            None => None,
        };
        let now = regent_store::now_epoch();
        let result = repo.load().and_then(|mut jobs| {
            let mut found = false;
            for j in &mut jobs {
                if j.id == job_id {
                    if let Some(n) = &new_name {
                        j.name = n.clone();
                    }
                    if let Some(p) = &new_prompt {
                        j.prompt = p.clone();
                    }
                    if let Some(s) = &new_schedule {
                        j.schedule = s.clone();
                        if let Some(next) = j.schedule.next_after(now) {
                            j.next_run_at = next;
                        }
                    }
                    found = true;
                    break;
                }
            }
            repo.save(&jobs).map(|()| found)
        });
        match result {
            Ok(updated) => self.send(ok_response(req.id, json!({"id": job_id, "updated": updated}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

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

    /// Lists the core tool catalog with each tool's enabled state (a tool is
    /// disabled when its name is in config `tools.disabled`).
    pub(super) fn tools_list(&self, req: RpcRequest) {
        let catalog = match regent_tools::core_catalog_from_env() {
            Ok(c) => c,
            Err(e) => {
                self.send(err_response(req.id, -32000, e.to_string()));
                return;
            }
        };
        let disabled = self.config.as_ref().map(|c| c.tools.disabled.as_slice()).unwrap_or(&[]);
        let items: Vec<_> = catalog
            .definitions()
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
