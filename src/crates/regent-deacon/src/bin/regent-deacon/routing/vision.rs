//! Vision/document env routing. Explicit `speech.vision` config wins; Auto
//! preserves user env overrides and otherwise follows the main chat route.

use super::Routing;
use std::ffi::OsString;
use std::sync::Mutex;

const MARKER: &str = "REGENT_VISION_AUTO";
const VARS: [&str; 3] = [
    "REGENT_VISION_BASE_URL",
    "REGENT_VISION_API_KEY",
    "REGENT_VISION_MODEL",
];
type Export = (&'static str, String);

#[derive(Default)]
struct ExplicitState {
    active: bool,
    previous: Vec<(&'static str, Option<OsString>)>,
}

static EXPLICIT_STATE: Mutex<ExplicitState> = Mutex::new(ExplicitState {
    active: false,
    previous: Vec::new(),
});

fn marker_owners() -> Vec<String> {
    std::env::var(MARKER)
        .unwrap_or_default()
        .split(',')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn remove(vars: impl IntoIterator<Item = impl AsRef<str>>) {
    for var in vars {
        // SAFETY: the deacon owns these process-local routing variables.
        unsafe { std::env::remove_var(var.as_ref()) };
    }
}

fn set_marker(exports: &[Export]) {
    if exports.is_empty() {
        unsafe { std::env::remove_var(MARKER) };
    } else {
        let names = exports
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(",");
        unsafe { std::env::set_var(MARKER, names) };
    }
}

fn apply_explicit(ours: &[String], exports: &[Export]) {
    let mut state = EXPLICIT_STATE.lock().unwrap();
    if !state.active {
        state.previous = VARS
            .into_iter()
            .map(|var| {
                let prior = (!ours.iter().any(|owned| owned == var))
                    .then(|| std::env::var_os(var))
                    .flatten();
                (var, prior)
            })
            .collect();
        state.active = true;
    }
    remove(VARS);
    for (var, value) in exports {
        unsafe { std::env::set_var(var, value) };
    }
    set_marker(exports);
}

fn restore_before_auto() -> bool {
    let mut state = EXPLICIT_STATE.lock().unwrap();
    if !state.active {
        return false;
    }
    remove(VARS);
    for (var, value) in &state.previous {
        if let Some(value) = value {
            unsafe { std::env::set_var(var, value) };
        }
    }
    unsafe { std::env::remove_var(MARKER) };
    *state = ExplicitState::default();
    true
}

fn explicit_exports(cfg: &regent_deacon::DeaconConfig) -> Result<Option<Vec<Export>>, String> {
    let provider = cfg.speech.vision.provider.trim();
    if provider.is_empty() || provider.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    let model = cfg.speech.vision.model.trim();
    if model.is_empty() {
        return Err(format!("vision provider '{provider}' has no model"));
    }
    let spec = cfg
        .providers
        .get(provider)
        .ok_or_else(|| format!("vision provider '{provider}' is not configured"))?;
    let base = regent_deacon::openai_style_base(spec.kind, spec.base_url.as_deref())
        .ok_or_else(|| format!("vision provider '{provider}' is not OpenAI-compatible"))?;
    let mut exports = vec![
        ("REGENT_VISION_BASE_URL", base),
        ("REGENT_VISION_MODEL", model.to_owned()),
    ];
    let key_var = spec.api_key_env.trim();
    if !key_var.is_empty() {
        let key = std::env::var(key_var)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("vision provider '{provider}' is missing key {key_var}"))?;
        exports.push(("REGENT_VISION_API_KEY", key));
    }
    Ok(Some(exports))
}

/// Export the route consumed by vision/video/document tools. Called at boot
/// and on every config/env reload, so the next tool call sees the new choice.
pub(super) fn export(routing: &Routing, cfg: &regent_deacon::DeaconConfig) {
    let mut ours = marker_owners();
    match explicit_exports(cfg) {
        Ok(Some(exports)) => {
            apply_explicit(&ours, &exports);
            return;
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "explicit vision route unavailable; using Auto"),
    }

    if restore_before_auto() {
        ours.clear();
    }
    let Some(base) = regent_deacon::openai_style_base(routing.kind, routing.base_url.as_deref())
    else {
        remove(&ours);
        unsafe { std::env::remove_var(MARKER) };
        return;
    };
    let key = routing.kind.resolve_key();
    if key.is_empty() {
        remove(&ours);
        unsafe { std::env::remove_var(MARKER) };
        return;
    }
    let mut candidates = vec![
        ("REGENT_VISION_BASE_URL", base),
        ("REGENT_VISION_API_KEY", key),
    ];
    if let Some(primary) = &routing.primary {
        candidates.push(("REGENT_VISION_MODEL", primary.model.clone()));
    }

    // Refresh only marker-owned values. Non-marker env values are manual and
    // keep precedence in Auto mode.
    remove(&ours);
    let exports: Vec<Export> = candidates
        .into_iter()
        .filter(|(var, _)| match std::env::var(var) {
            Ok(value) => value.trim().is_empty(),
            Err(_) => true,
        })
        .collect();
    for (var, value) in &exports {
        unsafe { std::env::set_var(var, value) };
    }
    set_marker(&exports);
}

#[cfg(test)]
#[path = "vision_tests.rs"]
mod tests;
