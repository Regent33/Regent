//! `model.*` + `providers.*` handlers — catalog, active-model switch, and the
//! configured multi-provider surface (ADR-026).

use super::providers_ops::split_provider_model;
use super::{Dispatcher, model_catalog};
use crate::domain::config::ProviderKind;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    pub(super) fn model_get(&self, req: RpcRequest) {
        self.send(ok_response(req.id, json!({"model": self.sessions.model()})));
    }

    pub(super) fn model_list(&self, req: RpcRequest) {
        let active = self.sessions.model();
        let snapshot = self.config_snapshot();
        // The static Claude menu is only offered when something can actually
        // serve those ids: an anthropic-kind provider in config, or the legacy
        // no-config boot (single Anthropic provider from REGENT_API_KEY).
        // Otherwise every entry is dead on arrival — a pick fails at call time.
        let claude_live = snapshot.as_ref().is_none_or(|cfg| {
            cfg.providers
                .values()
                .any(|spec| spec.kind == ProviderKind::Anthropic)
        });
        let mut items: Vec<_> = if claude_live {
            model_catalog()
                .iter()
                .map(
                    |(id, label)| json!({"id": id, "display_name": label, "current": *id == active}),
                )
                .collect()
        } else {
            Vec::new()
        };
        // Merge configured providers' models (multi-model-per-key, §3). Each is
        // listed as "<provider>/<model>" so the id round-trips back through
        // model.set / the registry. Sorted for a stable menu (the map isn't).
        if let Some(cfg) = snapshot {
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
}
