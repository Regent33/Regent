#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Hard ceiling on model calls per turn (the default).
    pub max_iterations: u32,
    /// Session source tag persisted to the store (`cli`, `gateway`, …).
    pub source: String,
    /// Model context window used for compression preflight estimates.
    pub max_context_tokens: u32,
    pub compression: CompressionConfig,
    /// Extended-thinking budget in tokens. `Some(n)` makes the model reason
    /// before answering (passed through to providers that support it, with the
    /// thinking block replayed across tool-use turns); `None` (default) is off.
    pub thinking_budget: Option<u32>,
    /// Per-turn spend ceiling in total tokens (prompt + completion, summed
    /// across the turn's model calls). When the running total reaches this, the
    /// agent loop halts the turn (like `max_iterations`) instead of spending
    /// more. `None` (default) = no ceiling. Bounds the cost of a single message
    /// so a runaway or abusive turn can't run up unbounded API spend (W2.4).
    pub max_turn_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CompressionConfig {
    pub enabled: bool,
    /// Compress when the estimated prompt exceeds this fraction of
    /// `max_context_tokens` (preflight default: 0.5).
    pub trigger_fraction: f32,
    /// Newest messages kept verbatim through compression (default 20).
    pub protect_last_n: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 90,
            source: "cli".to_owned(),
            max_context_tokens: 128_000,
            compression: CompressionConfig::default(),
            thinking_budget: None,
            max_turn_tokens: None,
        }
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_fraction: 0.5,
            protect_last_n: 20,
        }
    }
}
