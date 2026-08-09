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

// The Settings → API Keys page renders exactly these rows. Every provider the
// model picker can select must therefore have a row here, or the user can pick
// a provider with no way to authenticate it. The list used to be hand-written
// beside the enum and had already drifted; this pins them together.
#[test]
fn every_provider_kind_has_a_settable_api_key_row() {
    // Deliberately does NOT touch REGENT_HOME: this asserts only on which rows
    // EXIST and how they are labelled, never on whether a key is set, so it
    // needs no fixture — and setting the var would race the sibling test that
    // does (both would be mutating the same process-wide env).
    let rows = env_key_rows();
    for kind in ProviderKind::ALL {
        let var = kind.key_env_var();
        let row = rows
            .iter()
            .find(|r| r["name"] == var)
            .unwrap_or_else(|| panic!("{kind:?}: {var} is missing from Settings → API Keys"));
        assert_eq!(
            row["group"],
            if kind.is_local() || var == "OLLAMA_API_KEY" {
                "local"
            } else {
                "llm"
            },
            "{kind:?}"
        );
        assert_eq!(row["label"], kind.label(), "{kind:?}");
        // Present in the list but unwritable would be just as broken.
        assert!(
            is_settable(var),
            "{kind:?}: {var} is listed but not settable"
        );
    }
    // The generic fallback still leads the list.
    assert_eq!(rows[0]["name"], "REGENT_API_KEY");
}

#[test]
fn media_key_rows_match_the_adapters_regent_actually_ships() {
    let rows = env_key_rows();
    assert!(
        rows.iter()
            .any(|row| row["name"] == "REGENT_VISION_API_KEY" && row["group"] == "vision")
    );
    assert!(
        rows.iter()
            .any(|row| row["name"] == "REGENT_IMAGE_API_KEY" && row["group"] == "image")
    );
    for unsupported in ["STABILITY_API_KEY", "RUNWAY_API_KEY", "SUNO_API_KEY"] {
        assert!(
            !rows.iter().any(|row| row["name"] == unsupported),
            "{unsupported} was advertised without a matching adapter"
        );
    }
}

// The same contract one layer down. The voice menu is generated from
// `SPEECH_PROVIDERS`, so every backend there that takes a key needs somewhere
// to put it — and three of them (AI/ML API, Azure OpenAI, RunPod) reached no
// group at all: `key_group` bucketed them "llm" by fallthrough, and the llm
// rows are derived from `ProviderKind`, which has no such kind. They sat in the
// managed table, settable by the agent, rendering nowhere for the user.
#[test]
fn every_speech_provider_that_takes_a_key_has_a_row_in_the_speech_group() {
    let rows = env_key_rows();
    for provider in regent_speech::SPEECH_PROVIDERS {
        let Some(var) = provider.key_var else {
            continue;
        };
        assert!(
            rows.iter()
                .any(|r| r["name"] == var && r["group"] == "speech"),
            "{}: {var} has no row under Settings → API Keys → Speech",
            provider.id
        );
        // Listed but unwritable would be just as broken.
        assert!(
            is_settable(var),
            "{}: {var} is listed but not settable",
            provider.id
        );
    }
    // A key shared with an LLM provider keeps BOTH rows: it is one secret, and
    // it belongs in each place the user would go looking for it.
    let groq: Vec<_> = rows
        .iter()
        .filter(|r| r["name"] == "GROQ_API_KEY")
        .collect();
    assert!(groq.iter().any(|r| r["group"] == "llm"), "{groq:?}");
    let speech = groq
        .iter()
        .find(|r| r["group"] == "speech")
        .expect("groq is a speech backend too");
    // Named per provider, not "speech API key" — the whole point of the row.
    assert!(
        speech["label"]
            .as_str()
            .unwrap_or_default()
            .contains("Groq"),
        "{speech}"
    );
}

#[test]
fn settable_covers_catalogued_provider_and_integration_keys_but_blocks_runtime() {
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

#[test]
fn arbitrary_credential_shaped_environment_variables_are_not_settable() {
    for name in ["DATABASE_URL", "ATTACKER_API_KEY", "AWS_SECRET_ACCESS_KEY"] {
        assert!(
            !is_settable(name),
            "uncatalogued variable was accepted: {name}"
        );
    }
}

#[test]
fn catalogued_rows_and_only_canonical_numbered_slots_are_settable() {
    for row in env_key_rows() {
        let base = row["name"].as_str().expect("catalog row has a name");
        if base
            .rsplit_once('_')
            .is_some_and(|(_, suffix)| suffix.parse::<u8>().is_ok())
        {
            continue;
        }
        assert!(
            is_settable(base),
            "catalogued variable was rejected: {base}"
        );
        assert!(
            is_settable(&format!("{base}_2")),
            "slot 2 rejected for {base}"
        );
        assert!(
            is_settable(&format!("{base}_8")),
            "slot 8 rejected for {base}"
        );
        for invalid in [
            format!("{base}_1"),
            format!("{base}_9"),
            format!("{base}_02"),
        ] {
            assert!(
                !is_settable(&invalid),
                "invalid slot was accepted: {invalid}"
            );
        }
    }
}
