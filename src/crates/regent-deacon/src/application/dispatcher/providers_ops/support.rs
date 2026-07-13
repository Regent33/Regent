//! Helpers for the providers.* handlers. Split for the file-size rule.

use crate::domain::config::{DeaconConfig, ProviderKind, ProviderSpec};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

/// A LOCAL Ollama provider (kind ollama, NOT pointed at ollama.com): its
/// catalog is whatever the user has pulled, fetched live rather than curated.
pub(super) fn is_local_ollama(spec: &ProviderSpec) -> bool {
    spec.kind == ProviderKind::Ollama
        && !spec
            .base_url
            .as_deref()
            .is_some_and(|u| u.contains("ollama.com"))
}

/// Pulled model names from a running Ollama server (`GET /api/tags`). Empty on
/// ANY failure (server down / bad response) — the picker then falls back to the
/// configured list or free-text, never surfacing an error. Short timeouts so a
/// stopped ollama doesn't stall the settings load.
pub(super) async fn fetch_ollama_models(base_url: &str) -> Vec<String> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
    else {
        return Vec::new();
    };
    let Ok(resp) = client.get(&url).send().await else {
        return Vec::new();
    };
    let Ok(body) = resp.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    body.get("models")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Build the `{ provider: [models] }` catalog: live pulled models (local ollama)
/// lead, then the provider's configured `models:`, then the kind's curated
/// defaults — deduped, preserving that order.
pub(super) fn providers_models_map(
    cfg: &DeaconConfig,
    live: &HashMap<String, Vec<String>>,
) -> serde_json::Value {
    fn add(models: &mut Vec<String>, m: &str) {
        if !models.iter().any(|x| x == m) {
            models.push(m.to_owned());
        }
    }
    let mut map = serde_json::Map::new();
    for (name, spec) in &cfg.providers {
        let mut models: Vec<String> = Vec::new();
        if let Some(pulled) = live.get(name) {
            for m in pulled {
                add(&mut models, m);
            }
        }
        for m in &spec.models {
            add(&mut models, m);
        }
        for &d in spec.curated_defaults() {
            add(&mut models, d);
        }
        map.insert(name.clone(), json!(models));
    }
    serde_json::Value::Object(map)
}

/// Splits a `"provider/model"` id against the CONFIGURED provider names —
/// model ids themselves contain slashes ("z-ai/glm-5.2"), so only a prefix
/// matching a real provider name splits. `None` for bare/unknown ids.
pub(crate) fn split_provider_model(
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
