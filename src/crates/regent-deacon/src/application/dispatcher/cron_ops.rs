//! `cron.list/add/remove` — the cron job store's CRUD surface. Enable/run/edit
//! live in `cron_edit_ops`.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_cron::{CronJob, Schedule};
use serde_json::json;

impl Dispatcher {
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
                self.send(err_response(
                    req.id,
                    -32602,
                    "cron.add needs name, schedule, prompt",
                ));
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
        let result = repo.mutate(&mut |jobs| jobs.push(job.clone()));
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
        let mut removed = false;
        let result = repo.mutate(&mut |jobs| {
            let before = jobs.len();
            jobs.retain(|j| j.id != job_id);
            removed = jobs.len() < before;
        });
        let result = result.map(|()| removed);
        match result {
            Ok(removed) => self.send(ok_response(req.id, json!({"removed": removed}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}
