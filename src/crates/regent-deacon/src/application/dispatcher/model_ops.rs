//! `model.*` + `providers.*` handlers — catalog, active-model switch, and the
//! configured multi-provider surface (ADR-026).

use super::{Dispatcher, model_catalog};
use crate::application::provider_registry::ProviderRegistry;
use crate::domain::config::{OLLAMA_CLOUD_MODELS, ProviderKind};
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_kernel::{ChatMessage, ModelRef};
use regent_providers::ChatRequest;
use serde_json::json;

impl Dispatcher {
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
                "note": "applied — takes effect from your next message (open sessions re-route too)",
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

    /// `providers.models` — the EFFECTIVE pickable model catalog per configured
    /// provider: `{ "<name>": ["model", …] }`. The provider's own `models:`
    /// entries come FIRST (user curation leads the list), then the curated
    /// defaults for its kind fill in (deduped) so the dropdown always offers
    /// the wider catalog. An `ollama`-kind provider pointed at ollama.com gets
    /// the HOSTED catalog instead of the local kind's empty default. Read-only
    /// + additive: these defaults are NEVER persisted (config.set only writes
    /// the path it's handed).
    pub(super) fn providers_models(&self, req: RpcRequest) {
        let Some(cfg) = self.config_snapshot() else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        let mut map = serde_json::Map::new();
        for (name, spec) in &cfg.providers {
            let defaults: &[&str] = if spec.kind == ProviderKind::Ollama
                && spec
                    .base_url
                    .as_deref()
                    .is_some_and(|u| u.contains("ollama.com"))
            {
                OLLAMA_CLOUD_MODELS
            } else {
                spec.kind.default_models()
            };
            let mut models = spec.models.clone();
            for d in defaults {
                if !models.iter().any(|m| m == d) {
                    models.push((*d).to_owned());
                }
            }
            map.insert(name.clone(), json!(models));
        }
        self.send(ok_response(req.id, serde_json::Value::Object(map)));
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
}
