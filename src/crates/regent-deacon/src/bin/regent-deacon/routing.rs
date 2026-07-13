//! Live provider routing for the deacon binary: the snapshot type, its
//! construction from config, the per-session provider factory (with the
//! failover-notification chain), and the REGENT_VISION_* export that keeps
//! standalone vision/document calls on the active provider. Split from
//! main.rs (file-size rule).

use regent_deacon::{
    OutboundTx, ProviderFactory, ProviderKind, ProviderRegistry, make_provider_factory,
};
use std::sync::Arc;

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
    export_vision_route(&routing);
    routing
}

/// Keeps the standalone vision/document calls (`vision_analyze`,
/// `read_document`'s model-direct rung) on the ACTIVE provider: their
/// `REGENT_VISION_*` env fallbacks are exported from the current routing at
/// boot and on every config reload, so switching providers carries them along
/// instead of leaving a stale hardcoded default. A value the USER set always
/// wins — only values this function exported (flagged by the marker var) are
/// ever refreshed. Anthropic routing exports nothing (not OpenAI-compatible);
/// the tools keep their own static fallback there.
fn export_vision_route(routing: &Routing) {
    // The marker holds the comma-joined names THIS function exported, so a
    // reload refreshes exactly those and never clobbers a var the user set —
    // a global "we exported something" flag would treat every var as ours.
    const MARKER: &str = "REGENT_VISION_AUTO";
    let ours: Vec<String> = std::env::var(MARKER)
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    // No exportable route (Anthropic, or a keyless provider): clear what WE
    // exported so documents never keep flowing to a provider the user left.
    let clear_ours = || {
        for var in &ours {
            // SAFETY: single-process env ownership, as below.
            unsafe { std::env::remove_var(var) };
        }
        unsafe { std::env::remove_var(MARKER) };
    };
    let Some(base) = regent_deacon::openai_style_base(routing.kind, routing.base_url.as_deref())
    else {
        clear_ours();
        return;
    };
    let key = routing.kind.resolve_key();
    if key.is_empty() {
        clear_ours();
        return;
    }
    let mut exports = vec![
        ("REGENT_VISION_BASE_URL", base),
        ("REGENT_VISION_API_KEY", key),
    ];
    // The tools' static model id only exists on OpenRouter — when the user
    // routes elsewhere, point them at the active primary model instead (a
    // text-only model fails the call harmlessly; the tools fall back).
    if let Some(primary) = &routing.primary {
        exports.push(("REGENT_VISION_MODEL", primary.model.clone()));
    }
    let mut exported: Vec<&str> = Vec::new();
    for (var, value) in exports {
        let user_set = std::env::var(var).is_ok_and(|v| !v.trim().is_empty())
            && !ours.iter().any(|o| o == var);
        if !user_set {
            // SAFETY: the deacon owns its process env (same pattern as the
            // key manager's env activation).
            unsafe { std::env::set_var(var, value) };
            exported.push(var);
        }
    }
    // SAFETY: as above — single-process env ownership.
    unsafe { std::env::set_var(MARKER, exported.join(",")) };
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
