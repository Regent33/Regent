//! Model + provider config: the main model, the multi-provider map, per-agent
//! defaults, MoM groups, and spend limits. `ProviderKind` itself (the wire
//! enum) lives in `config::provider_kind` — adding a provider touches only that.

use super::provider_kind::ProviderKind;
use regent_kernel::ModelRef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    pub default: String,
    /// Wire protocol: "anthropic" (native Messages API, prompt-cache
    /// breakpoints) or "openai" (any OpenAI-compatible endpoint).
    pub provider: ProviderKind,
    /// Overrides the provider base URL (None = provider's own default).
    pub base_url: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            default: "claude-sonnet-4-6".to_owned(),
            provider: ProviderKind::default(),
            base_url: None,
        }
    }
}

/// One configured provider: a wire protocol (`kind`), an optional endpoint
/// override, the env var holding its key, and the model ids it serves. One
/// `api_key_env` serves every model in `models` (multi-model-per-key — §3).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ProviderSpec {
    pub kind: ProviderKind,
    /// Override the wire base URL; `None` = the kind's own default.
    pub base_url: Option<String>,
    /// Env var name holding the API key (read at registry build — never the
    /// key itself, so secrets stay out of config and version control).
    pub api_key_env: String,
    /// Model ids this provider serves — the catalog `model.list` merges in.
    pub models: Vec<String>,
}

/// Per-agent model defaults: the primary model and an ordered fallback chain
/// applied to every named-agent provider built through the registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AgentsDefaults {
    pub primary: Option<ModelRef>,
    pub fallbacks: Vec<ModelRef>,
}

/// One Mixture-of-Models group (§B): proposer model specs answered in parallel,
/// then `aggregator` synthesizes them. Specs are `"provider/model"` (or a bare
/// id resolved against `agents_defaults.primary`) — resolved through the
/// provider registry at run time. `max_proposers` 0 = the runner default (3).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MomGroupConfig {
    pub proposers: Vec<String>,
    pub aggregator: String,
    pub max_proposers: usize,
}

/// Spend/rate limits (W2.4). Currently just a per-turn token ceiling; the
/// inbound rate limiter (Layer A) will slot in here later.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsConfig {
    /// Per-turn spend ceiling in total tokens (prompt + completion, summed over
    /// the turn's model calls). Absent/`null` (default) = no ceiling. Bounds the
    /// cost of a single message; maps to `AgentConfig::max_turn_tokens`.
    pub max_turn_tokens: Option<u32>,
}

/// The constitutional values layer (character + hard boundaries, shipped in
/// `regent-agent`). **On by default** — when enabled the deacon seeds the
/// `constitution` persona row from the shipped document on boot and every
/// session's prompt renders it. Set `constitution: { enabled: false }` to opt out.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstitutionConfig {
    pub enabled: bool,
}

impl Default for ConstitutionConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}
