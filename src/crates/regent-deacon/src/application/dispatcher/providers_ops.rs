//! `providers.*` handlers — the configured multi-provider surface
//! (ADR-026): list, per-provider model catalog, and connectivity test.
//! Split from `model_ops.rs` (file-size rule).

use super::Dispatcher;
use crate::application::provider_registry::ProviderRegistry;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_kernel::{ChatMessage, ModelRef};
use regent_providers::ChatRequest;
use serde_json::json;
use std::collections::HashMap;

impl Dispatcher {
    /// `providers.catalog` — every SUPPORTED provider kind (not just the
    /// configured ones): wire name, conventional key env var, default host,
    /// and the curated model list. This is what the setup wizard's pickers
    /// render on a fresh install, when `providers.list` is empty.
    pub(super) fn providers_catalog(&self, req: RpcRequest) {
        use crate::domain::config::ProviderKind;
        let kinds: Vec<_> = ProviderKind::ALL
            .iter()
            .map(|k| {
                json!({
                    "name": k.name(),
                    "key_env": k.key_env_var(),
                    "host": k.openai_base_path().0,
                    "needs_key": *k != ProviderKind::Ollama,
                    "models": k.default_models(),
                })
            })
            .collect();
        self.send(ok_response(req.id, json!(kinds)));
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

    /// `providers.models` — the EFFECTIVE pickable model catalog per configured
    /// provider: `{ "<name>": ["model", …] }`. The provider's own `models:`
    /// entries come FIRST (user curation leads the list), then the curated
    /// defaults for its kind fill in (deduped) so the dropdown always offers
    /// the wider catalog. An `ollama`-kind provider pointed at ollama.com gets
    /// the HOSTED catalog instead of the local kind's empty default. Read-only
    /// + additive: these defaults are NEVER persisted (config.set only writes
    ///   the path it's handed).
    pub(super) fn providers_models(&self, req: RpcRequest) {
        let Some(cfg) = self.config_snapshot() else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        // A LOCAL Ollama provider's catalog is whatever the user has PULLED — not
        // a static list — so fetch it live from the running server. When none are
        // local ollama, answer synchronously (no network).
        if !cfg.providers.values().any(is_local_ollama) {
            self.send(ok_response(
                req.id,
                providers_models_map(&cfg, &HashMap::new()),
            ));
            return;
        }
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let mut live: HashMap<String, Vec<String>> = HashMap::new();
            for (name, spec) in &cfg.providers {
                if is_local_ollama(spec) {
                    let base = spec.base_url.as_deref().unwrap_or("http://localhost:11434");
                    live.insert(name.clone(), fetch_ollama_models(base).await);
                }
            }
            let payload = providers_models_map(&cfg, &live);
            if let Ok(line) = serde_json::to_string(&ok_response(req.id, payload)) {
                out_tx.send(line).ok();
            }
        });
    }

    /// `providers.test` — resolve `name` (a provider name → its first model, or a
    /// `"provider/model"` id) through a registry built from config, then send a
    /// tiny live completion to confirm the key + endpoint actually work. Returns
    /// `{ok, model, error?}` (never a transport error — the failure is the result).
    pub(super) fn providers_test(&self, req: RpcRequest) {
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
        // Detached: a dead endpoint can hang for its full transport timeout,
        // and the serial read loop must never block on network I/O.
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let mut request = ChatRequest::new(String::new(), vec![ChatMessage::user("ping")]);
            request.max_tokens = Some(8);
            let result = match provider.complete(&request).await {
                Ok(_) => json!({"ok": true, "model": model_ref.to_string()}),
                Err(error) => {
                    json!({"ok": false, "model": model_ref.to_string(), "error": error.to_string()})
                }
            };
            if let Ok(line) = serde_json::to_string(&ok_response(req.id, result)) {
                out_tx.send(line).ok();
            }
        });
    }
}

pub(super) use support::split_provider_model;
use support::{fetch_ollama_models, is_local_ollama, providers_models_map};

mod support;

#[cfg(test)]
#[path = "providers_ops_tests.rs"]
mod tests;
