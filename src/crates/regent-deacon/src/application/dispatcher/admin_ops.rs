//! Admin/query handlers: skills, model catalog/switch, config, cron CRUD, and
//! the memory write-approval surface.

use super::{Dispatcher, model_catalog};
use crate::application::provider_registry::ProviderRegistry;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_agent::MomRunner;
use regent_cron::{CronJob, Schedule};
use regent_kernel::{ChatMessage, ModelRef};
use regent_providers::ChatRequest;
use serde_json::json;

impl Dispatcher {
    pub(super) fn skills_list(&self, req: RpcRequest) {
        match self.sessions.skills_list() {
            Ok(skills) => {
                let mut items: Vec<_> = skills
                    .iter()
                    .map(|s| json!({"name": s.name, "description": s.description, "tags": s.tags}))
                    .collect();
                // Additive: `include_archived: true` appends retired skills,
                // each flagged — so the opt-out toggle can be flipped back on.
                if req.params.get("include_archived").and_then(|v| v.as_bool()) == Some(true) {
                    match self.sessions.skills_list_archived() {
                        Ok(archived) => items.extend(archived.iter().map(|s| {
                            json!({
                                "name": s.name, "description": s.description,
                                "tags": s.tags, "archived": true,
                            })
                        })),
                        Err(e) => {
                            self.send(err_response(req.id, -32000, e.to_string()));
                            return;
                        }
                    }
                }
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
                self.send(err_response(
                    req.id,
                    -32602,
                    "skills.create needs name, description, body",
                ));
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

    /// Inverse of `skills.opt_out`: restore a previously archived skill.
    pub(super) fn skills_opt_in(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        match self.sessions.skill_unarchive(name) {
            Ok(()) => self.send(ok_response(req.id, json!({"restored": name}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn memory_pending(&self, req: RpcRequest) {
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as u32;
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
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as u32;
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
            Ok(found) => self.send(ok_response(
                req.id,
                json!({"found": found, "pinned": false}),
            )),
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
        let mut items: Vec<_> = model_catalog()
            .iter()
            .map(|(id, label)| json!({"id": id, "display_name": label, "current": *id == active}))
            .collect();
        // Merge configured providers' models (multi-model-per-key, §3). Each is
        // listed as "<provider>/<model>" so the id round-trips back through
        // model.set / the registry. Sorted for a stable menu (the map isn't).
        if let Some(cfg) = self.config_snapshot() {
            let mut provider_ids: Vec<String> = cfg
                .providers
                .iter()
                .flat_map(|(name, spec)| spec.models.iter().map(move |m| format!("{name}/{m}")))
                .collect();
            provider_ids.sort();
            items.extend(provider_ids.into_iter().map(|id| {
                let current = id == active;
                json!({"id": id, "display_name": id, "current": current})
            }));
        }
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

    /// `providers.list` — the configured multi-provider map (ADR-026), each with
    /// whether its API-key env var is currently set (never the key itself).
    /// Sorted by name for stable output.
    pub(super) fn providers_list(&self, req: RpcRequest) {
        let Some(cfg) = self.config_snapshot() else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        let mut rows: Vec<(&String, serde_json::Value)> = cfg
            .providers
            .iter()
            .map(|(name, spec)| {
                let key_present = std::env::var(&spec.api_key_env)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                (
                    name,
                    json!({
                        "name": name,
                        "kind": spec.kind,
                        "base_url": spec.base_url,
                        "api_key_env": spec.api_key_env,
                        "key_present": key_present,
                        "models": spec.models,
                    }),
                )
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(b.0));
        let items: Vec<_> = rows.into_iter().map(|(_, v)| v).collect();
        self.send(ok_response(req.id, json!(items)));
    }

    /// `providers.test` — resolve `name` (a provider name → its first model, or a
    /// `"provider/model"` id) through a registry built from config, then send a
    /// tiny live completion to confirm the key + endpoint actually work. Returns
    /// `{ok, model, error?}` (never a transport error — the failure is the result).
    pub(super) async fn providers_test(&self, req: RpcRequest) {
        let Some(cfg) = self.config_snapshot() else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        let Some(target) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        let registry = ProviderRegistry::from_config(&cfg.providers);
        // "provider/model" id, else a bare provider name → its first model.
        let model_ref = registry.resolve_model_str(target, None).or_else(|| {
            cfg.providers
                .get(target)
                .and_then(|s| s.models.first())
                .map(|m| ModelRef::new(target, m))
        });
        let Some(model_ref) = model_ref else {
            self.send(err_response(
                req.id,
                -32602,
                format!("unknown provider or no models for '{target}'"),
            ));
            return;
        };
        let provider = match registry.provider_for(&model_ref) {
            Ok(p) => p,
            Err(error) => {
                self.send(ok_response(
                    req.id,
                    json!({"ok": false, "model": model_ref.to_string(), "error": error.to_string()}),
                ));
                return;
            }
        };
        let mut request = ChatRequest::new(String::new(), vec![ChatMessage::user("ping")]);
        request.max_tokens = Some(8);
        let result = match provider.complete(&request).await {
            Ok(_) => json!({"ok": true, "model": model_ref.to_string()}),
            Err(error) => {
                json!({"ok": false, "model": model_ref.to_string(), "error": error.to_string()})
            }
        };
        self.send(ok_response(req.id, result));
    }

    /// `mom.run` — run a configured Mixture-of-Models group (§B): resolve its
    /// proposer + aggregator model specs through the registry, fan out the
    /// proposers (advisory), and return the aggregator's synthesis. Unresolvable
    /// proposers are skipped (logged); an unresolvable aggregator is a hard error.
    pub(super) async fn mom_run(&self, req: RpcRequest) {
        let (Some(name), Some(brief)) = (
            req.params
                .get("name")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            req.params
                .get("brief")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
        ) else {
            self.send(err_response(req.id, -32602, "missing name or brief"));
            return;
        };
        // Resolve to an owned MomRunner synchronously, so no borrow of self/config
        // crosses the await below (keeps the future Send for the in-process tool).
        let runner = match self.prepare_mom(&name) {
            Ok(runner) => runner,
            Err((code, message)) => {
                self.send(err_response(req.id, code, message));
                return;
            }
        };
        match runner.run(&brief).await {
            Ok(synthesis) => self.send(ok_response(
                req.id,
                json!({"group": name, "synthesis": synthesis}),
            )),
            Err(error) => self.send(err_response(req.id, -32000, error.to_string())),
        }
    }

    /// Build a `MomRunner` for a configured group, resolving its model specs
    /// through the provider registry. Sync (borrows self only here); typed
    /// `(code, message)` errors map straight to a JSON-RPC error.
    fn prepare_mom(&self, name: &str) -> Result<MomRunner, (i32, String)> {
        let cfg = self
            .config_snapshot()
            .ok_or((-32000, "config not wired".to_owned()))?;
        let group = cfg
            .mom
            .get(name)
            .ok_or((-32602, format!("unknown mom group '{name}'")))?;
        let registry = ProviderRegistry::from_config(&cfg.providers);
        let default = cfg.agents_defaults.primary.as_ref();
        let proposers: Vec<_> = group
            .proposers
            .iter()
            .filter_map(|spec| {
                let provider = registry
                    .resolve_model_str(spec, default)
                    .and_then(|m| registry.provider_for(&m).ok());
                if provider.is_none() {
                    tracing::warn!(spec, "mom proposer unresolved; skipping");
                }
                provider
            })
            .collect();
        let aggregator_ref = registry
            .resolve_model_str(&group.aggregator, default)
            .ok_or((
                -32602,
                format!(
                    "mom aggregator '{}' unresolved — configure a matching provider",
                    group.aggregator
                ),
            ))?;
        let aggregator = registry
            .provider_for(&aggregator_ref)
            .map_err(|e| (-32000, format!("mom aggregator: {e}")))?;
        let mut runner = MomRunner::new(proposers, aggregator);
        if group.max_proposers > 0 {
            runner = runner.with_max_proposers(group.max_proposers);
        }
        Ok(runner)
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

    // ── Persona (DB-backed soul / user profile) ─────────────────────────────

    pub(super) fn persona_get(&self, req: RpcRequest) {
        let key = req
            .params
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("soul");
        match self.sessions.persona_get(key) {
            Ok(content) => self.send(ok_response(
                req.id,
                json!({ "key": key, "content": content }),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn persona_set(&self, req: RpcRequest) {
        let key = req.params.get("key").and_then(|v| v.as_str()).unwrap_or("");
        let content = req
            .params
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !regent_store::is_valid_persona_key(key) {
            self.send(err_response(
                req.id,
                -32602,
                "key must be 'soul', 'about', or 'about.<identity|preferences|habits|constraints|goals>'",
            ));
            return;
        }
        match self.sessions.persona_set(key, content) {
            Ok(()) => self.send(ok_response(req.id, json!({ "ok": true }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    // ── Kanban board ────────────────────────────────────────────────────────

    pub(super) fn kanban_create(&self, req: RpcRequest) {
        let title = req
            .params
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if title.is_empty() {
            self.send(err_response(req.id, -32602, "missing title"));
            return;
        }
        let body = req
            .params
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match self.sessions.kanban_create(title, body) {
            Ok(id) => self.send(ok_response(req.id, json!({ "id": id }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn kanban_list(&self, req: RpcRequest) {
        let status = req.params.get("status").and_then(|v| v.as_str());
        match self.sessions.kanban_list(status) {
            Ok(tasks) => {
                let items: Vec<_> = tasks.iter().map(task_json).collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn kanban_show(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing id"));
            return;
        };
        match self.sessions.kanban_show(id) {
            Ok(Some(task)) => self.send(ok_response(req.id, task_json(&task))),
            Ok(None) => self.send(err_response(req.id, -32004, format!("no task {id}"))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn kanban_assign(&self, req: RpcRequest) {
        let (Some(id), Some(worker)) = (
            req.params.get("id").and_then(|v| v.as_str()),
            req.params.get("worker").and_then(|v| v.as_str()),
        ) else {
            self.send(err_response(req.id, -32602, "missing id or worker"));
            return;
        };
        match self.sessions.kanban_assign(id, worker) {
            Ok(claimed) => self.send(ok_response(req.id, json!({ "claimed": claimed }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn kanban_set_status(&self, req: RpcRequest) {
        let (Some(id), Some(status)) = (
            req.params.get("id").and_then(|v| v.as_str()),
            req.params.get("status").and_then(|v| v.as_str()),
        ) else {
            self.send(err_response(req.id, -32602, "missing id or status"));
            return;
        };
        match self.sessions.kanban_set_status(id, status) {
            Ok(ok) => self.send(ok_response(req.id, json!({ "ok": ok }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
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

    // ── Named agents ──────────────────────────────────────────────────────────

    pub(super) fn agents_list(&self, req: RpcRequest) {
        match self.sessions.agents_list() {
            Ok(agents) => {
                let items: Vec<_> = agents.iter().map(agent_json).collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn agents_show(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        match self.sessions.agents_show(name) {
            Ok(Some(a)) => self.send(ok_response(req.id, agent_json(&a))),
            Ok(None) => self.send(err_response(req.id, -32004, format!("no agent '{name}'"))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn agents_set(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        if name.trim().is_empty() || name.contains(char::is_whitespace) {
            self.send(err_response(
                req.id,
                -32602,
                "name must be a single non-empty word",
            ));
            return;
        }
        let get = |k: &str| req.params.get(k).and_then(|v| v.as_str());
        let description = get("description").unwrap_or("");
        let system_prompt = get("system_prompt").unwrap_or("");
        // Empty model/tools strings mean "unset" (inherit / full catalog).
        let model = get("model").filter(|s| !s.trim().is_empty());
        let tools = get("tools").filter(|s| !s.trim().is_empty());
        match self
            .sessions
            .agents_set(name, description, system_prompt, model, tools)
        {
            Ok(()) => self.send(ok_response(req.id, json!({ "ok": true }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn agents_remove(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        match self.sessions.agents_remove(name) {
            Ok(removed) => self.send(ok_response(req.id, json!({ "removed": removed }))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }
}

/// JSON shape for one named agent (the `agents.*` surface).
fn agent_json(a: &regent_store::AgentRow) -> serde_json::Value {
    json!({
        "name": a.name,
        "description": a.description,
        "system_prompt": a.system_prompt,
        "model": a.model,
        "tools": a.tools,
        "created_at": a.created_at,
        "updated_at": a.updated_at,
    })
}

/// JSON shape for one kanban task (the `kanban.*` surface).
fn task_json(t: &regent_store::KanbanTaskRow) -> serde_json::Value {
    json!({
        "id": t.id,
        "board": t.board,
        "title": t.title,
        "description": t.description,
        "status": t.status,
        "assignee": t.assignee,
        "created_at": t.created_at,
        "updated_at": t.updated_at,
    })
}
