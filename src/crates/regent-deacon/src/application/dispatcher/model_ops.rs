//! `model.*` + `providers.*` handlers — catalog, active-model switch, and the
//! configured multi-provider surface (ADR-026).

use super::{Dispatcher, model_catalog};
use crate::application::provider_registry::ProviderRegistry;
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
        // ONE source of truth: a "provider/model" pick (the composer pill /
        // status-bar menu) also persists as `agents_defaults.primary`, so the
        // Settings Model page and the next boot agree with what chat runs.
        // Bare catalog ids (no configured provider) apply live-only.
        let persisted = self.persist_pick_as_primary(model);
        self.send(ok_response(
            req.id,
            json!({
                "model": model,
                "persisted": persisted,
                "note": if persisted {
                    "applied and saved as your main model — takes effect from your next message"
                } else {
                    "applied — takes effect from your next message (open sessions re-route too)"
                },
            }),
        ));
    }

    /// Writes a resolvable `"provider/model"` pick to `agents_defaults.primary`
    /// through the validated config gate (custom ids also join the provider's
    /// catalog). Fail-open: an unresolvable id or write failure leaves the
    /// live switch in place and returns false.
    fn persist_pick_as_primary(&self, model: &str) -> bool {
        let Some(cfg) = self.config_snapshot() else {
            return false;
        };
        let Some((provider, model_id)) = split_provider_model(&cfg, model) else {
            return false;
        };
        let Ok(home) = std::env::var("REGENT_HOME") else {
            return false;
        };
        let home = std::path::Path::new(&home);
        match super::config_ops::set_config_path(
            home,
            "agents_defaults.primary",
            &json!({"provider": provider, "model": model_id}),
        ) {
            Ok((_, config)) => {
                let config = super::config_ops::adopt_custom_models(home, config);
                self.apply_config(config);
                true
            }
            Err(error) => {
                tracing::warn!(%error, model, "could not persist model pick as primary");
                false
            }
        }
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
            let defaults = spec.curated_defaults();
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

/// Splits a `"provider/model"` id against the CONFIGURED provider names —
/// model ids themselves contain slashes ("z-ai/glm-5.2"), so only a prefix
/// matching a real provider name splits. `None` for bare/unknown ids.
fn split_provider_model(
    cfg: &crate::domain::config::DeaconConfig,
    model: &str,
) -> Option<(String, String)> {
    cfg.providers.keys().find_map(|name| {
        model
            .strip_prefix(&format!("{name}/"))
            .filter(|rest| !rest.is_empty())
            .map(|rest| (name.clone(), rest.to_owned()))
    })
}

#[cfg(test)]
mod tests {
    use super::split_provider_model;
    use crate::domain::config::DeaconConfig;

    #[test]
    fn splits_on_configured_provider_names_only() {
        let cfg: DeaconConfig = serde_yaml::from_str(
            "_config_version: 1\nproviders:\n  nvidia:\n    kind: nvidia\n    api_key_env: K\n",
        )
        .unwrap();
        // A model id with its own slashes splits at the PROVIDER boundary.
        assert_eq!(
            split_provider_model(&cfg, "nvidia/z-ai/glm-5.2"),
            Some(("nvidia".into(), "z-ai/glm-5.2".into()))
        );
        // Bare catalog ids and unknown prefixes don't persist.
        assert_eq!(split_provider_model(&cfg, "claude-sonnet-4-6"), None);
        assert_eq!(
            split_provider_model(&cfg, "openrouter/minimax/minimax-m3"),
            None
        );
        assert_eq!(split_provider_model(&cfg, "nvidia/"), None);
    }
}
