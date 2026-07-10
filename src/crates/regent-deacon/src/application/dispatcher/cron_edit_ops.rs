//! `cron.set_enabled/run/edit` — in-place job mutations (CRUD lives in
//! `cron_ops`).

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_cron::Schedule;
use serde_json::json;

impl Dispatcher {
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
        let enabled = req
            .params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let now = regent_store::now_epoch();
        let mut found = false;
        let result = repo
            .mutate(&mut |jobs| {
                for j in jobs.iter_mut() {
                    if j.id == job_id {
                        j.enabled = enabled;
                        if enabled && let Some(next) = j.schedule.next_after(now) {
                            j.next_run_at = next;
                        }
                        found = true;
                        break;
                    }
                }
            })
            .map(|()| found);
        match result {
            Ok(found) => {
                self.send(ok_response(
                    req.id,
                    json!({"id": job_id, "enabled": enabled, "found": found}),
                ));
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
        let mut found = false;
        let result = repo
            .mutate(&mut |jobs| {
                for j in jobs.iter_mut() {
                    if j.id == job_id {
                        j.enabled = true;
                        j.next_run_at = now;
                        found = true;
                        break;
                    }
                }
            })
            .map(|()| found);
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
        let new_name = req
            .params
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let new_prompt = req
            .params
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
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
        let mut found = false;
        let result = repo
            .mutate(&mut |jobs| {
                for j in jobs.iter_mut() {
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
            })
            .map(|()| found);
        match result {
            Ok(updated) => self.send(ok_response(
                req.id,
                json!({"id": job_id, "updated": updated}),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}
