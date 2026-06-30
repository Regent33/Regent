use serde::{Deserialize, Serialize};

/// A concrete model on a concrete configured provider. Cheap to clone; used as
/// a map key in the provider registry so a `(provider, model)` pair builds once.
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
}

impl ModelRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.provider, self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_provider_slash_model() {
        let m = ModelRef::new("openrouter", "anthropic/claude-opus-4-8");
        assert_eq!(m.to_string(), "openrouter/anthropic/claude-opus-4-8");
    }

    #[test]
    fn round_trips_through_json() {
        let m = ModelRef::new("groq", "llama-3.3-70b");
        let back: ModelRef = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(m, back);
    }
}
