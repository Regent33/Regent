use or_core::TokenUsage;
use regent_kernel::{ChatMessage, ToolDefinition};

/// Explicit prompt-cache TTL for providers that offer it (Anthropic). The
/// cadence study (`docs/audits/2026-07-10-cadence-study.md`) picks per surface:
/// 5m for tight internal loops, 1h for human-paced chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheTtl {
    /// Anthropic's default ephemeral TTL — write costs 1.25×, no `ttl` field.
    FiveMinutes,
    /// Extended TTL — write costs 2×, emitted as `{"ttl":"1h"}`.
    OneHour,
}

/// SPL P2 (`docs/proposal/token-efficiency-architecture-v1.md` §3.2): the
/// opt-in cache policy a request carries. `ChatRequest.cache = Some(_)` asks a
/// caching provider (Anthropic) to place `cache_control` breakpoints on the
/// stable prefix; `None` (the default) sends today's request with no
/// breakpoints. Automatic-caching providers ignore it — byte-stability, which
/// P1 guarantees, is all they need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachePolicy {
    pub ttl: CacheTtl,
}

/// One model call. `system` travels separately from the transcript so the
/// cached prefix stays byte-stable for the life of a conversation (the
/// prompt-caching invariant) — callers pass the same `system` string
/// every turn and append only transcript messages.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    /// Extended-thinking budget in tokens. `Some(n)` enables thinking (the
    /// model reasons before answering, up to `n` tokens); `None` disables it.
    /// Providers that don't support it ignore this.
    pub thinking_budget: Option<u32>,
    /// Optional prompt-cache policy (SPL P2). `None` (default) = no explicit
    /// cache breakpoints, today's behavior. `Some(_)` opts the request into
    /// Anthropic `cache_control` breakpoints at the chosen TTL; other providers
    /// ignore it.
    pub cache: Option<CachePolicy>,
}

impl ChatRequest {
    #[must_use]
    pub fn new(system: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            system: system.into(),
            messages,
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            thinking_budget: None,
            cache: None,
        }
    }

    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Opts the request into explicit prompt-cache breakpoints (SPL P2). No-op
    /// on providers without an explicit cache mechanism.
    #[must_use]
    pub fn with_cache(mut self, policy: CachePolicy) -> Self {
        self.cache = Some(policy);
        self
    }

    /// Enables extended thinking with the given token budget.
    #[must_use]
    pub fn with_thinking(mut self, budget_tokens: u32) -> Self {
        self.thinking_budget = Some(budget_tokens);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// Always an assistant message; `tool_calls` non-empty when the model
    /// requested tools.
    pub message: ChatMessage,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
}

impl ChatResponse {
    #[must_use]
    pub fn wants_tools(&self) -> bool {
        !self.message.tool_calls.is_empty()
    }

    /// No user-actionable output: whitespace-only (or absent) visible content
    /// and no tool calls. This is the AGENT's notion — the turn loop repairs it
    /// in place (reveal-on-stuck). It is deliberately NOT the chain's failover
    /// test; see [`ChatResponse::produced_nothing`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.message.tool_calls.is_empty()
            && self
                .message
                .content
                .as_deref()
                .is_none_or(|c| c.trim().is_empty())
    }

    /// The provider produced NOTHING AT ALL — no tool calls, no visible text,
    /// and no reasoning either — or it hung up before saying it was done.
    /// This is the chain's failover test.
    ///
    /// A reasoning model that thinks and then stops short of visible text is
    /// `is_empty` but demonstrably alive: the provider answered, streamed
    /// tokens, and billed for them. Failing over there switched the user's
    /// model mid-session — and stickily, for the rest of it — while nothing was
    /// wrong with either provider, which is exactly what a healthy model
    /// reaching for a deferred tool looks like. That case belongs to the
    /// agent's own repair, on the SAME provider; only silence is a fault.
    ///
    /// What proves it stopped short *on purpose* is the terminal chunk. A
    /// stream severed mid-thought looks identical — thinking, no answer — but
    /// carries no `finish_reason`, and it is a fault: nothing is coming, and
    /// the same call will be cut at the same place again. `integrate.api.
    /// nvidia.com` ends a stream at ~303s, so a long completion (rewriting a
    /// 584-line file) could never land; the chain read the wreckage as a
    /// healthy model thinking, stayed put, and the turn loop burned a second
    /// 303s call on it before erroring "empty response ... twice".
    ///
    /// Gated on `is_empty`, so a response that DID carry text or tool calls is
    /// never discarded for a missing `finish_reason` — only wreckage reroutes.
    #[must_use]
    pub fn produced_nothing(&self) -> bool {
        self.is_empty()
            && (self.finish_reason.is_none()
                || self
                    .message
                    .reasoning
                    .as_deref()
                    .is_none_or(|r| r.trim().is_empty()))
    }
}
