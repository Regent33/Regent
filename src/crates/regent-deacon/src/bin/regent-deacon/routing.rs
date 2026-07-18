//! Live provider routing for the deacon binary: the snapshot type, its
//! construction from config, the per-session provider factory (with the
//! failover-notification chain), and the REGENT_VISION_* export that keeps
//! standalone vision/document calls on the active provider. Split from
//! main.rs (file-size rule).

use regent_deacon::{
    OutboundTx, ProviderFactory, ProviderKind, ProviderRegistry, make_provider_factory,
};
use std::sync::Arc;

mod vision;

/// Live provider-routing state — one snapshot per config/env change. The
/// factory reads it per session build; the reload hook replaces it whole
/// (also dropping the old registry's memoized providers, so rotated keys and
/// edited provider entries take effect).
pub(crate) struct Routing {
    pub(crate) registry: ProviderRegistry,
    pub(crate) primary: Option<regent_kernel::ModelRef>,
    pub(crate) fallbacks: Vec<regent_kernel::ModelRef>,
    pub(crate) kind: ProviderKind,
    pub(crate) base_url: Option<String>,
}

pub(crate) fn routing_from(cfg: &regent_deacon::DeaconConfig) -> Routing {
    let routing = Routing {
        registry: ProviderRegistry::from_config(&cfg.providers),
        primary: cfg.agents_defaults.primary.clone(),
        fallbacks: cfg.agents_defaults.fallbacks.clone(),
        // Env still wins at boot AND on reload (same precedence as before).
        kind: ProviderKind::from_env_or(cfg.model.provider),
        base_url: std::env::var("REGENT_BASE_URL")
            .ok()
            .or_else(|| cfg.model.base_url.clone()),
    };
    vision::export(&routing, cfg);
    routing
}

/// The per-session provider factory: resolves the active model through the
/// registry to a fallback chain (emitting `model.failover` pills), degrading
/// to the single legacy provider when no registry/primary is configured.
pub(crate) fn provider_factory_from(
    routing: &Arc<std::sync::RwLock<Routing>>,
    out_tx: &OutboundTx,
) -> ProviderFactory {
    let routing = Arc::clone(routing);
    let out = out_tx.clone();
    Arc::new(move |model: &str| {
        let r = routing.read().unwrap();
        let single =
            || make_provider_factory(r.kind, r.kind.resolve_key(), r.base_url.clone())(model);
        match &r.primary {
            Some(primary) if !r.registry.is_empty() => {
                let picked = r
                    .registry
                    .resolve_model_str(model, Some(primary))
                    .unwrap_or_else(|| primary.clone());
                let mut chain_fallbacks = Vec::new();
                if picked != *primary {
                    chain_fallbacks.push(primary.clone());
                }
                chain_fallbacks.extend(r.fallbacks.iter().filter(|f| **f != picked).cloned());
                // Auto-fallback: with no explicit `agents_defaults.fallbacks`
                // configured, derive a chain from the user's OTHER configured
                // models, so a dead or reasoning-only primary (the nemotron
                // "empty response … twice" error) self-heals onto a working
                // model instead of erroring. Deduped by (provider, model);
                // chain_for still skips any that can't resolve a key.
                if r.fallbacks.is_empty() {
                    for m in r.registry.auto_fallbacks(&picked) {
                        if !chain_fallbacks
                            .iter()
                            .any(|f| f.provider == m.provider && f.model == m.model)
                        {
                            chain_fallbacks.push(m);
                        }
                    }
                }
                // Emit `model.failover` so the composer pill / status bar can
                // show the model actually answering during a provider outage,
                // and clear it on recovery. Transient — never touches the
                // user's selected model (`model.changed`).
                let out = out.clone();
                let on_change: regent_providers::ActiveChangeFn =
                    std::sync::Arc::new(move |engaged: bool, active: &str| {
                        let note = regent_deacon::RpcNotification::new(
                            "model.failover",
                            serde_json::json!({ "engaged": engaged, "model": active }),
                        );
                        if let Ok(line) = serde_json::to_string(&note) {
                            out.send(line).ok();
                        }
                    });
                r.registry
                    .chain_for(&picked, &chain_fallbacks, Some(on_change))
                    .unwrap_or_else(|e| {
                        tracing::warn!(%e, "fallback chain unresolvable; using single provider");
                        single()
                    })
            }
            _ => single(),
        }
    })
}

/// Boot-time active model: REGENT_MODEL override, else the applied
/// `agents_defaults.primary` ("provider/model"), else legacy `model.default` —
/// without the primary a restart re-pointed chat at the old default.
pub(crate) fn initial_model_from(cfg: &regent_deacon::DeaconConfig) -> String {
    std::env::var("REGENT_MODEL").unwrap_or_else(|_| {
        cfg.agents_defaults
            .primary
            .as_ref()
            .map(|p| format!("{}/{}", p.provider, p.model))
            .unwrap_or_else(|| cfg.model.default.clone())
    })
}

/// Logs the ACTUAL provider the active model resolves to through the registry
/// — not the legacy `kind` (a chain id routes to its own provider).
pub(crate) fn log_selected_provider(
    routing: &Arc<std::sync::RwLock<Routing>>,
    initial_model: &str,
) {
    let r = routing.read().unwrap();
    let selected_provider = r
        .registry
        .resolve_model_str(initial_model, r.primary.as_ref())
        .map_or_else(|| format!("{:?}", r.kind), |m| m.provider);
    tracing::info!(provider = %selected_provider, model = %initial_model, "model provider selected");
}
