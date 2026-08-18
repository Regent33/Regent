//! Every provider Regent can *use* must be a provider the user can
//! *authenticate* — from any surface, not just the one that happens to derive
//! its list.
//!
//! The deacon builds its API Keys rows from `ProviderKind::ALL` and
//! `regent_speech::SPEECH_PROVIDERS`, so a new provider reaches the app for
//! free. Two other surfaces derive nothing:
//!
//!   * the agent's own `manage_keys` tool gates on `regent_tools::MANAGED`
//!     (this is Regent setting a key on the user's behalf), and
//!   * `regent keys set` gates on the TypeScript mirror of that same table
//!     (scripts/tests/verify-key-catalog.py pins the mirror to it).
//!
//! So a kind missing from `MANAGED` is not a cosmetic gap: the app offers the
//! provider and stores its key, while the CLI and the agent both answer "not in
//! Regent's managed key catalog" for the identical variable. Deriving the check
//! from the enum is the only version that stays true when someone adds the
//! thirty-fourth variant.

use regent_deacon::domain::config::ProviderKind;
use regent_tools::{MANAGED, key_group};

fn managed(var: &str) -> bool {
    MANAGED.iter().any(|(name, _)| *name == var)
}

#[test]
fn every_llm_provider_key_is_in_the_shared_managed_catalog() {
    for kind in ProviderKind::ALL {
        let var = kind.key_env_var();
        assert!(
            managed(var),
            "{kind:?}: {var} is offered by the model picker but missing from \
             regent_tools::MANAGED — `regent keys set {var}` and the agent's \
             manage_keys tool would both refuse a key the app accepts"
        );
        // The page draws one section per group; a provider key filed anywhere
        // but its own section is found by nobody looking for it.
        // Hosted Ollama is not a local kind, but it bills to the same
        // account and shares the local daemon's variable — one secret, one row,
        // filed where the app already draws it.
        let expected = if kind.is_local() || var == "OLLAMA_API_KEY" {
            "local"
        } else {
            "llm"
        };
        assert_eq!(key_group(var), expected, "{kind:?}: {var} groups wrong");
    }
}

#[test]
fn every_speech_backend_key_is_in_the_shared_managed_catalog() {
    for provider in regent_speech::SPEECH_PROVIDERS {
        let Some(var) = provider.key_var else {
            continue;
        };
        assert!(
            managed(var),
            "speech backend {}: {var} is offered by the voice picker but \
             missing from regent_tools::MANAGED — the backend would be \
             selectable with no supported way to authenticate it",
            provider.label
        );
    }
}

// A key shared with an LLM provider (Groq, OpenAI, DashScope…) is one secret
// listed in two places, and it must keep its LLM group: `key_group` is what the
// CLI mirror is checked against, and moving a model key into "speech" would
// take it out of the section every model user looks in.
#[test]
fn speech_keys_shared_with_a_model_provider_stay_filed_under_the_model() {
    let llm_vars: Vec<&str> = ProviderKind::ALL.iter().map(|k| k.key_env_var()).collect();
    for provider in regent_speech::SPEECH_PROVIDERS {
        let Some(var) = provider.key_var else {
            continue;
        };
        if llm_vars.contains(&var) {
            let expected = if var == "OLLAMA_API_KEY" {
                "local"
            } else {
                "llm"
            };
            assert_eq!(
                key_group(var),
                expected,
                "{var} is a model provider key reused by the {} speech backend",
                provider.label
            );
        }
    }
}
