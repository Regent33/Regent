//! Unit tests for `env_ops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::{auto_provider, env_key_rows, is_settable};
use crate::domain::config::{DeaconConfig, ProviderKind, ProviderSpec};

#[test]
fn a_new_provider_key_yields_a_config_entry_that_survives_the_write_gate() {
    let cfg = DeaconConfig::default();
    // The reported bug: NVIDIA_API_KEY saved, no `nvidia` provider → the
    // Model page (which lists only config.providers) never shows it.
    let (path, value) = auto_provider(&cfg, "NVIDIA_API_KEY").expect("adds nvidia");
    assert_eq!(path, "providers.nvidia");
    // A numbered slot behaves like its base var.
    assert!(auto_provider(&cfg, "GROQ_API_KEY_2").is_some());
    // The generated value must pass the same whole-file validation
    // config.set applies — otherwise the auto-add silently no-ops.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.yaml"), "_config_version: 1\n").unwrap();
    let (_, parsed) = super::super::config_ops::set_config_path(dir.path(), &path, &value).unwrap();
    let spec = parsed.providers.get("nvidia").expect("entry persisted");
    assert_eq!(spec.kind, ProviderKind::Nvidia);
    assert_eq!(spec.api_key_env, "NVIDIA_API_KEY");
}

#[test]
fn auto_provider_never_duplicates_or_clobbers() {
    let mut cfg = DeaconConfig::default();
    // Non-provider / generic keys map to nothing.
    assert!(auto_provider(&cfg, "REGENT_API_KEY").is_none());
    assert!(auto_provider(&cfg, "TAVILY_API_KEY").is_none());
    assert!(auto_provider(&cfg, "SLACK_BOT_TOKEN").is_none());
    // A same-kind entry under ANY name blocks the add (the real config
    // shape: `ollama-cloud` of kind ollama reading OLLAMA_API_KEY).
    cfg.providers.insert(
        "ollama-cloud".to_owned(),
        ProviderSpec {
            kind: ProviderKind::Ollama,
            api_key_env: "OLLAMA_API_KEY".to_owned(),
            ..ProviderSpec::default()
        },
    );
    assert!(auto_provider(&cfg, "OLLAMA_API_KEY").is_none());
    // An entry already reading the var blocks it even under another kind…
    cfg.providers.insert(
        "my-gateway".to_owned(),
        ProviderSpec {
            kind: ProviderKind::Openai,
            api_key_env: "GROQ_API_KEY".to_owned(),
            ..ProviderSpec::default()
        },
    );
    assert!(auto_provider(&cfg, "GROQ_API_KEY").is_none());
    // …and a taken name is never overwritten.
    cfg.providers.insert(
        "mistral".to_owned(),
        ProviderSpec {
            kind: ProviderKind::Openai,
            ..ProviderSpec::default()
        },
    );
    assert!(auto_provider(&cfg, "MISTRAL_API_KEY").is_none());
}

#[test]
fn env_list_surfaces_a_messaging_key_grouped_and_masked() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".env"),
        "REGENT_TELEGRAM_TOKEN=bot-secret-9876\nOPENROUTER_API_KEY_2=second-key-4321\n",
    )
    .unwrap();
    // SAFETY: single-threaded test; env_var_status reads REGENT_HOME/.env.
    unsafe { std::env::set_var("REGENT_HOME", dir.path()) };

    let rows = env_key_rows();
    let tg = rows
        .iter()
        .find(|r| r["name"] == "REGENT_TELEGRAM_TOKEN")
        .expect("telegram token is in the managed set");
    assert_eq!(tg["group"], "messaging");
    assert_eq!(tg["set"], true);
    assert_eq!(tg["masked"], "****9876");
    // The raw value must never be returned.
    assert!(!tg.to_string().contains("bot-secret-9876"));
    // LLM provider keys stay in the "llm" group (older/flat clients ok).
    let anthropic = rows
        .iter()
        .find(|r| r["name"] == "ANTHROPIC_API_KEY")
        .expect("anthropic key present");
    assert_eq!(anthropic["group"], "llm");
    // A SET numbered slot shows up beside its base with a slot label…
    let second = rows
        .iter()
        .find(|r| r["name"] == "OPENROUTER_API_KEY_2")
        .expect("set _2 slot is listed");
    assert_eq!(second["group"], "llm");
    assert_eq!(second["label"], "OpenRouter (2)");
    assert_eq!(second["masked"], "****4321");
    // …but unset slots are never listed.
    assert!(!rows.iter().any(|r| r["name"] == "ANTHROPIC_API_KEY_2"));
}

#[test]
fn settable_covers_llm_and_credential_suffixes_but_blocks_runtime() {
    assert!(is_settable("OLLAMA_API_KEY"));
    assert!(is_settable("OPENROUTER_API_KEY"));
    assert!(is_settable("REGENT_API_KEY")); // the user's own model key
    assert!(is_settable("TAVILY_API_KEY"));
    assert!(is_settable("SLACK_BOT_TOKEN"));
    // Blocked runtime / model-routing (use config.set for those).
    assert!(!is_settable("REGENT_HOME"));
    assert!(!is_settable("PATH"));
    assert!(!is_settable("REGENT_MODEL"));
    // Numbered multi-key slots: settable iff the base is.
    assert!(is_settable("OPENROUTER_API_KEY_2"));
    assert!(is_settable("SLACK_BOT_TOKEN_3"));
    assert!(!is_settable("OPENROUTER_API_KEY_2X"));
    assert!(!is_settable("REGENT_HOME_2"));
    // Not a credential shape.
    assert!(!is_settable("RANDOM_FLAG"));
    assert!(!is_settable("lowercase_key"));
    assert!(!is_settable(""));
}
