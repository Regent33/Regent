//! ADR-038 P0(c): the analytic A-vs-B billed-token model — pure arithmetic,
//! no I/O — separated from `profile_ops` (file-size rule) so the math is
//! testable without a `Dispatcher`/`SessionManager` fixture.

/// Anthropic explicit 5-minute cache breakpoint: a write bills at 1.25x the
/// base input rate; a read (hit within the TTL) bills at 0.1x (plan §3, §8).
const ANTHROPIC_5M_WRITE_MULT: f64 = 1.25;
const ANTHROPIC_5M_READ_MULT: f64 = 0.1;
/// Anthropic explicit 1-hour cache breakpoint: writes bill at 2x — the plan
/// v1 correction (§8): a rarely-revisited 1h-tier profile has double the
/// break-even v1 assumed. Reads still bill at 0.1x.
const ANTHROPIC_1H_WRITE_MULT: f64 = 2.0;
const ANTHROPIC_1H_READ_MULT: f64 = 0.1;
/// Implicit-cache providers (DeepSeek-style, §8): writes bill at the base
/// rate (no premium); reads bill at 0.1x.
const IMPLICIT_WRITE_MULT: f64 = 1.0;
const IMPLICIT_READ_MULT: f64 = 0.1;
/// Alternative B's per-escalated-session tax (§3.1): the mode-injection
/// message appended over the one stable `full` prefix, billed uncached (1x)
/// since it rides the live message list, never the cached prefix. A flat
/// estimate pending B's own telemetry, not a measured number.
const B_MODE_INJECTION_TOKENS: f64 = 200.0;

/// Source-name substrings treated as inherently agentic (never `light`) when
/// present in the measured mix — plan §8's "agentic-source session" case.
pub(super) const AGENTIC_SOURCE_HINTS: &[&str] = &["code", "delegate", "review"];

/// The cache model the deacon actually runs today (`cache_policy_for_source`
/// grants `deacon`-source sessions 5m breakpoints) — the one `verdict` in
/// `profile.report` is judged against.
pub(super) const DEACON_CACHE_MODEL: &str = "anthropic_5m";

/// One provider's cache pricing: write/read multipliers vs. the base input
/// rate (see the named constants above for citations).
pub(super) struct CacheModel {
    pub name: &'static str,
    pub write_mult: f64,
    pub read_mult: f64,
}

pub(super) const CACHE_MODELS: &[CacheModel] = &[
    CacheModel {
        name: "anthropic_5m",
        write_mult: ANTHROPIC_5M_WRITE_MULT,
        read_mult: ANTHROPIC_5M_READ_MULT,
    },
    CacheModel {
        name: "anthropic_1h",
        write_mult: ANTHROPIC_1H_WRITE_MULT,
        read_mult: ANTHROPIC_1H_READ_MULT,
    },
    CacheModel {
        name: "implicit",
        write_mult: IMPLICIT_WRITE_MULT,
        read_mult: IMPLICIT_READ_MULT,
    },
];

/// The measured session mix reduced to the five numbers the A-vs-B model
/// needs, plus the two candidate prefix sizes in tokens.
pub(super) struct BilledInputs {
    pub light_tokens: f64,
    pub full_tokens: f64,
    pub escalation_share: f64,
    pub avg_turns: f64,
    pub chat_sessions: f64,
    pub agentic_sessions: f64,
}

/// A (two profiles) vs B (one `full` prefix + mode-injection) billed-token
/// totals for one cache model, given the measured session mix. Model, per
/// the plan:
/// - **A**: a non-escalating chat session = 1 light write + (turns−1) light
///   reads; an escalating session = 1 light write + (esc_turn−1 ≈ half the
///   turns) light reads + 1 full write + the remaining full reads; an
///   agentic-source session = 1 full write + (turns−1) full reads.
/// - **B**: every session = 1 full write + (turns−1) full reads, plus a flat
///   per-escalated-session uncached tax (the mode-injection message, §3.1).
pub(super) fn billed_tokens_a_vs_b(mix: &BilledInputs, model: &CacheModel) -> (f64, f64) {
    let turns = mix.avg_turns.max(1.0);
    let escalating_chat = mix.chat_sessions * mix.escalation_share;
    let non_escalating_chat = mix.chat_sessions - escalating_chat;

    let non_esc_cost = mix.light_tokens * (model.write_mult + (turns - 1.0) * model.read_mult);

    let esc_turn = (turns / 2.0).max(1.0);
    let light_reads = (esc_turn - 1.0).max(0.0);
    let full_reads = (turns - esc_turn - 1.0).max(0.0);
    let esc_cost = mix.light_tokens * (model.write_mult + light_reads * model.read_mult)
        + mix.full_tokens * (model.write_mult + full_reads * model.read_mult);

    let agentic_cost = mix.full_tokens * (model.write_mult + (turns - 1.0) * model.read_mult);

    let a_billed = non_escalating_chat * non_esc_cost
        + escalating_chat * esc_cost
        + mix.agentic_sessions * agentic_cost;

    let b_per_session = mix.full_tokens * (model.write_mult + (turns - 1.0) * model.read_mult);
    let total_sessions = mix.chat_sessions + mix.agentic_sessions;
    let b_billed = total_sessions * b_per_session + escalating_chat * B_MODE_INJECTION_TOKENS;

    (a_billed, b_billed)
}

#[cfg(test)]
#[path = "tests/profile_model_tests.rs"]
mod tests;
