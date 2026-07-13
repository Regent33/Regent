use serde::{Deserialize, Serialize};

/// A concrete model on a concrete configured provider. Cheap to clone; used as
/// a map key in the provider registry so a `(provider, model, key_slot)`
/// triple builds once.
///
/// `provider` is a **configured provider name** (a key in `config.providers`),
/// e.g. `"openrouter"` — not a wire protocol. `model` is the provider's own
/// model id, e.g. `"anthropic/claude-opus-4-8"` (which may itself contain `/`).
/// Parsing a `"provider/model"` string is provider-aware and lives in the
/// registry, which knows the configured names — this type stays pure.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
    /// Pins this ref to a specific stored key slot (`<API_KEY_ENV>_<N>`, slot 1
    /// = the base var) instead of the provider's active key. `None` = today's
    /// behavior. Lets a fallback be the SAME provider+model on a DIFFERENT key
    /// (multi-key failover). Omitted from serialized form when unset, so
    /// existing configs read and write byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_slot: Option<u8>,
}

impl ModelRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            key_slot: None,
        }
    }

    /// The same ref pinned to stored key slot `n` (1 = the base env var).
    #[must_use]
    pub fn with_key_slot(mut self, n: u8) -> Self {
        self.key_slot = Some(n);
        self
    }
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.provider, self.model)?;
        // The slot is part of identity — surface it in logs ("#2"), but only
        // when pinned so existing log lines stay unchanged.
        if let Some(n) = self.key_slot {
            write!(f, "#{n}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_provider_slash_model() {
        let m = ModelRef::new("openrouter", "anthropic/claude-opus-4-8");
        assert_eq!(m.to_string(), "openrouter/anthropic/claude-opus-4-8");
        assert_eq!(
            m.with_key_slot(2).to_string(),
            "openrouter/anthropic/claude-opus-4-8#2"
        );
    }

    #[test]
    fn round_trips_through_json() {
        let m = ModelRef::new("groq", "llama-3.3-70b");
        let back: ModelRef = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn key_slot_is_additive_and_omitted_when_unset() {
        // Old wire form (no key_slot) still parses…
        let old: ModelRef = serde_json::from_str(r#"{"provider":"groq","model":"m"}"#).unwrap();
        assert_eq!(old.key_slot, None);
        // …and an unset slot never appears in the serialized form.
        assert_eq!(
            serde_json::to_string(&old).unwrap(),
            r#"{"provider":"groq","model":"m"}"#
        );
        // A pinned slot round-trips and differentiates identity.
        let pinned = old.clone().with_key_slot(2);
        let back: ModelRef =
            serde_json::from_str(&serde_json::to_string(&pinned).unwrap()).unwrap();
        assert_eq!(back.key_slot, Some(2));
        assert_ne!(old, pinned);
    }
}
