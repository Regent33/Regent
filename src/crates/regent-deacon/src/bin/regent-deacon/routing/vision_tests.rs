use super::*;
use regent_deacon::{DeaconConfig, ProviderKind};
use serde_json::json;

static ENV_LOCK: Mutex<()> = Mutex::new(());
const TEST_VARS: [&str; 9] = [
    MARKER,
    "REGENT_VISION_BASE_URL",
    "REGENT_VISION_API_KEY",
    "REGENT_VISION_MODEL",
    "REGENT_PROVIDER",
    "REGENT_BASE_URL",
    "REGENT_API_KEY",
    "OPENROUTER_API_KEY",
    "VISION_TEST_KEY",
];

struct EnvSnapshot(Vec<(&'static str, Option<OsString>)>);

impl EnvSnapshot {
    fn isolated() -> Self {
        let saved = TEST_VARS
            .into_iter()
            .map(|var| (var, std::env::var_os(var)))
            .collect();
        remove(TEST_VARS);
        *EXPLICIT_STATE.lock().unwrap() = ExplicitState::default();
        Self(saved)
    }
}

impl Drop for EnvSnapshot {
    fn drop(&mut self) {
        remove(TEST_VARS);
        for (var, value) in &self.0 {
            if let Some(value) = value {
                unsafe { std::env::set_var(var, value) };
            }
        }
        *EXPLICIT_STATE.lock().unwrap() = ExplicitState::default();
    }
}

fn config() -> DeaconConfig {
    serde_json::from_value(json!({
        "model": { "provider": "openrouter" },
        "providers": {
            "eyes": {
                "kind": "nvidia",
                "api_key_env": "VISION_TEST_KEY",
                "models": ["vision-model"]
            }
        },
        "agents_defaults": {
            "primary": { "provider": "main", "model": "main-model" }
        }
    }))
    .unwrap()
}

#[test]
fn explicit_provider_exports_base_key_and_model_over_manual_env() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvSnapshot::isolated();
    unsafe {
        std::env::set_var("OPENROUTER_API_KEY", "main-key");
        std::env::set_var("VISION_TEST_KEY", "vision-key");
        std::env::set_var("REGENT_VISION_MODEL", "manual-model");
    }
    let mut cfg = config();
    cfg.speech.vision.provider = "eyes".into();
    cfg.speech.vision.model = "vision-model".into();

    super::super::routing_from(&cfg);

    assert_eq!(
        std::env::var("REGENT_VISION_BASE_URL").unwrap(),
        "https://integrate.api.nvidia.com/v1"
    );
    assert_eq!(
        std::env::var("REGENT_VISION_API_KEY").unwrap(),
        "vision-key"
    );
    assert_eq!(
        std::env::var("REGENT_VISION_MODEL").unwrap(),
        "vision-model"
    );
}

#[test]
fn auto_derives_the_main_route() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvSnapshot::isolated();
    unsafe { std::env::set_var("OPENROUTER_API_KEY", "main-key") };
    let cfg = config();

    super::super::routing_from(&cfg);

    assert_eq!(
        std::env::var("REGENT_VISION_BASE_URL").unwrap(),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(std::env::var("REGENT_VISION_API_KEY").unwrap(), "main-key");
    assert_eq!(std::env::var("REGENT_VISION_MODEL").unwrap(), "main-model");
}

#[test]
fn explicit_provider_without_its_key_falls_back_to_auto() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvSnapshot::isolated();
    unsafe { std::env::set_var("OPENROUTER_API_KEY", "main-key") };
    let mut cfg = config();
    cfg.speech.vision.provider = "eyes".into();
    cfg.speech.vision.model = "vision-model".into();

    let route = super::super::routing_from(&cfg);

    assert_eq!(route.kind, ProviderKind::OpenRouter);
    assert_eq!(std::env::var("REGENT_VISION_MODEL").unwrap(), "main-model");
    assert_eq!(std::env::var("REGENT_VISION_API_KEY").unwrap(), "main-key");
}

#[test]
fn switching_explicit_to_auto_restores_manual_env() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvSnapshot::isolated();
    unsafe {
        std::env::set_var("OPENROUTER_API_KEY", "main-key");
        std::env::set_var("VISION_TEST_KEY", "vision-key");
        std::env::set_var("REGENT_VISION_BASE_URL", "https://manual.example/v1");
        std::env::set_var("REGENT_VISION_API_KEY", "manual-key");
        std::env::set_var("REGENT_VISION_MODEL", "manual-model");
    }
    let mut cfg = config();
    cfg.speech.vision.provider = "eyes".into();
    cfg.speech.vision.model = "vision-model".into();
    super::super::routing_from(&cfg);

    cfg.speech.vision.provider = "auto".into();
    cfg.speech.vision.model.clear();
    super::super::routing_from(&cfg);

    assert_eq!(
        std::env::var("REGENT_VISION_BASE_URL").unwrap(),
        "https://manual.example/v1"
    );
    assert_eq!(
        std::env::var("REGENT_VISION_API_KEY").unwrap(),
        "manual-key"
    );
    assert_eq!(
        std::env::var("REGENT_VISION_MODEL").unwrap(),
        "manual-model"
    );
    assert!(std::env::var(MARKER).is_err());
}
