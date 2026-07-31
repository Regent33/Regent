//! `env.*` — UI-driven read/write of API keys in `$REGENT_HOME/.env`. Values
//! are NEVER returned (list reports set + a `****last4` mask only); writes go
//! through the same owner-only upsert the `manage_keys` tool uses. The settable
//! surface is bounded (LLM provider keys + a key-suffix rule) with the runtime/
//! model-routing vars hard-blocked, so the UI can't inject PATH / REGENT_HOME /
//! model wiring (that belongs in `config.set`) through here.

use super::Dispatcher;
use super::config_ops::set_config_path;
use crate::domain::config::{MAX_KEY_SLOTS, ProviderKind};
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_tools::{
    MANAGED, env_var_status, extra_key_groups, key_group, remove_env_var, swap_env_vars,
    upsert_env_var,
};
use serde_json::json;

/// The LLM provider key vars the API Keys page surfaces (var name, label),
/// **derived from [`ProviderKind::ALL`]** so a provider can never exist in the
/// model picker while its key row is missing from Settings. This was a
/// hand-written list; it had already drifted, and a duplicate list is how a
/// user ends up able to select a provider they have no way to authenticate.
///
/// `REGENT_API_KEY` leads: it is the generic fallback the deacon uses when a
/// provider-specific var is unset. The user setting it here is legitimate —
/// unlike the agent's `manage_keys` tool, which protects it from
/// self-clobbering. Kinds sharing a var (local + hosted Ollama) collapse to one
/// row, since it is one secret either way.
fn llm_keys() -> Vec<(&'static str, &'static str)> {
    let mut rows = vec![("REGENT_API_KEY", "Default (generic fallback)")];
    for kind in ProviderKind::ALL {
        let var = kind.key_env_var();
        if !rows.iter().any(|(existing, _)| *existing == var) {
            rows.push((var, kind.label()));
        }
    }
    rows
}

/// Never writable through the UI: process runtime + model-routing (use
/// `config.set` for provider/model/base_url so the validated schema applies).
const BLOCKED: &[&str] = &[
    "REGENT_HOME",
    "REGENT_NOW",
    "REGENT_MODEL",
    "REGENT_PROVIDER",
    "REGENT_BASE_URL",
    "PATH",
];

use env_catalog::{auto_provider, env_key_rows, is_settable, numbered_base};

mod env_catalog;

impl Dispatcher {
    /// `env.list` — the managed keys grouped for the UI, each with set-state +
    /// masked tail (no values).
    pub(super) fn env_list(&self, req: RpcRequest) {
        self.send(ok_response(req.id, json!({ "keys": env_key_rows() })));
    }

    /// `env.set {name, value}` — persist a key to `.env` (masked in the reply).
    pub(super) fn env_set(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        let name = name.trim().to_uppercase();
        if !is_settable(&name) {
            self.send(err_response(
                req.id,
                -32602,
                format!("{name} is not a settable key here"),
            ));
            return;
        }
        let value = req
            .params
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if value.is_empty() {
            self.send(err_response(req.id, -32602, "missing or empty value"));
            return;
        }
        match upsert_env_var(&name, value) {
            Ok(()) => {
                // upsert_env_var hot-applied the process env; the reload also
                // rebuilds provider routing so cached providers holding a
                // stale key are dropped.
                self.reapply_config();
                // New key for an unconfigured provider kind → add its minimal
                // `providers:` entry through the validated config path, so the
                // provider shows up in Settings → Model right away. Best-effort:
                // the key save already succeeded; a failure here only warns.
                let mut note = "saved to .env and applied — takes effect from your next message (open sessions re-route too)".to_owned();
                if let (Some(cfg), Ok(home)) =
                    (self.config_snapshot(), std::env::var("REGENT_HOME"))
                    && let Some((path, value)) = auto_provider(&cfg, &name)
                {
                    match set_config_path(std::path::Path::new(&home), &path, &value) {
                        Ok((_, parsed)) => {
                            let provider = path.trim_start_matches("providers.").to_owned();
                            self.apply_config(parsed);
                            note = format!(
                                "saved to .env and applied — provider '{provider}' added to config.yaml; pick its model in Settings → Model"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(%error, key = %name, "provider auto-add skipped");
                        }
                    }
                }
                let (_, masked) = env_var_status(&name);
                self.send(ok_response(
                    req.id,
                    json!({ "name": name, "masked": masked, "note": note }),
                ));
            }
            Err(e) => self.send(err_response(req.id, -32000, e)),
        }
    }

    /// `env.activate {name, slot}` — make numbered slot N the ACTIVE key for a
    /// base var by swapping values (the runtime resolves the base first). Both
    /// keys stay stored; only which one is "key 1" changes.
    pub(super) fn env_activate(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        let name = name.trim().to_uppercase();
        let slot = req.params.get("slot").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        if !is_settable(&name) || numbered_base(&name).is_some() {
            self.send(err_response(
                req.id,
                -32602,
                format!("{name} is not a base key managed here"),
            ));
            return;
        }
        if !(2..=MAX_KEY_SLOTS).contains(&slot) {
            self.send(err_response(
                req.id,
                -32602,
                format!("slot must be 2..={MAX_KEY_SLOTS}"),
            ));
            return;
        }
        match swap_env_vars(&name, &format!("{name}_{slot}")) {
            Ok(()) => {
                self.reapply_config();
                let (_, masked) = env_var_status(&name);
                self.send(ok_response(
                    req.id,
                    json!({ "name": name, "masked": masked, "note": "swapped and applied — takes effect from your next message (open sessions re-route too)" }),
                ));
            }
            Err(e) => self.send(err_response(req.id, -32000, e)),
        }
    }

    /// `env.unset {name}` — remove a key from `.env`.
    pub(super) fn env_unset(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        let name = name.trim().to_uppercase();
        if !is_settable(&name) {
            self.send(err_response(
                req.id,
                -32602,
                format!("{name} is not managed here"),
            ));
            return;
        }
        match remove_env_var(&name) {
            Ok(removed) => {
                self.reapply_config();
                self.send(ok_response(
                    req.id,
                    json!({ "name": name, "removed": removed }),
                ));
            }
            Err(e) => self.send(err_response(req.id, -32000, e)),
        }
    }
}

#[cfg(test)]
#[path = "env_ops_tests.rs"]
mod tests;
