//! Best-effort context-window lookup by model family. Feeds compaction
//! preflight and the context meter so the math follows whoever actually serves
//! the turn (ADR-038 P0a). Deliberately conservative: only families whose
//! documented base window we are confident of are listed. An unknown model
//! returns `None`, and the caller falls back to its configured default rather
//! than to a guess.
//!
//! This table WILL go stale — windows only grow. It is the middle rung, not
//! the authority: the user's per-model `context.windows` config override is
//! checked first (see `Agent::effective_max_context`), and stale-LOW is safe
//! by construction (early compaction, never an overflowed request).

/// The context window (in tokens) for a known model family, or `None` when the
/// family is unrecognized. Matching tolerates a `provider/` prefix
/// (`anthropic/claude-…`, `openai/gpt-4o`) and is case-insensitive. Values are
/// conservative documented base windows — a model's opt-in long-context beta is
/// never assumed (e.g. Claude reports 200k, not its 1M beta).
#[must_use]
pub fn window_for_model(model_id: &str) -> Option<u32> {
    // Drop any `provider/` prefix and lower-case for stable matching.
    let id = model_id
        .rsplit('/')
        .next()
        .unwrap_or(model_id)
        .to_ascii_lowercase();

    // Order matters: more specific ids come first so a broad substring never
    // shadows a narrower one (gpt-4.1's 1M window before gpt-4o's 128k;
    // gemini-1.5-pro's 2M before gemini-1.5-flash's 1M).
    if id.contains("claude") {
        // Per Anthropic's models catalog (2026-06): the Claude 5 generation
        // (Fable/Mythos/Sonnet 5) and the 4.6+ wave run a 1M window at
        // standard pricing; Haiku 4.5 and every older family are 200k.
        // Matches both id spellings ("opus-4-8" first-party, "opus-4.8"
        // OpenRouter) and suffixed variants ("-fast").
        const MILLION: &[&str] = &[
            "fable",
            "mythos",
            "sonnet-5",
            "opus-4-8",
            "opus-4.8",
            "opus-4-7",
            "opus-4.7",
            "opus-4-6",
            "opus-4.6",
            "sonnet-4-6",
            "sonnet-4.6",
        ];
        if MILLION.iter().any(|m| id.contains(m)) {
            return Some(1_000_000);
        }
        // Haiku tiers + older opus/sonnet — the conservative floor.
        return Some(200_000);
    }
    // GPT-5.6 (Sol/Terra/Luna) ships a 1.05M window; GPT-5.5 is 1M (both per
    // OpenAI's model pages, verified 2026-07-17). The original GPT-5 line is
    // 400k total with 272k of it INPUT — preflight sizes the input, so the
    // input limit is the honest number there. Specific families first.
    if id.contains("gpt-5.6") {
        return Some(1_050_000);
    }
    if id.contains("gpt-5.5") {
        return Some(1_000_000);
    }
    if id.contains("gpt-5") {
        return Some(272_000);
    }
    if id.contains("gpt-4.1") {
        return Some(1_000_000);
    }
    if id.contains("gpt-4o") || id.contains("gpt-4-turbo") {
        return Some(128_000);
    }
    // o-series reasoning models. `starts_with` (after prefix strip) avoids
    // matching an `o1`/`o3` that appears inside an unrelated id.
    if id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4") {
        return Some(200_000);
    }
    if id.contains("gemini-1.5-pro") {
        return Some(2_000_000);
    }
    if id.contains("gemini-1.5-flash") {
        return Some(1_000_000);
    }
    if id.contains("gemini-3.5") || id.contains("gemini-3") || id.contains("gemini-2") {
        return Some(1_048_576);
    }
    // Verified 2026-07-17 from the providers' model pages: DeepSeek V4
    // (pro + flash) is 1M by default; GLM-5.2 is 1M; Kimi K2.7 is 256k.
    if id.contains("deepseek-v4") {
        return Some(1_048_576);
    }
    // DeepSeek's own API: V3.x-served `deepseek-chat` is 128k; the reasoner
    // line is documented at 64k — the lowest documented figure wins (a host
    // serving more is recovered by discovery or `context.windows`).
    if id.contains("deepseek-r1") || id.contains("deepseek-reasoner") {
        return Some(65_536);
    }
    if id.contains("deepseek") {
        return Some(131_072);
    }
    if id.contains("glm-5.2") {
        return Some(1_000_000);
    }
    // Zhipu docs: GLM-4.6 is 200k; the rest of the GLM-4 line is 128k.
    if id.contains("glm-4.6") {
        return Some(200_000);
    }
    if id.contains("glm-4") || id.contains("glm-5") {
        return Some(131_072);
    }
    // Kimi K3 (released 2026-07-16): 1M context — "moonshotai/kimi-k3" on
    // OpenRouter, bare "k3" on Moonshot's own platform. The bare id gets an
    // exact match: "k3" as a substring would false-match unrelated ids.
    if id == "k3" || id.contains("kimi-k3") {
        return Some(1_048_576);
    }
    if id.contains("kimi-k2.7") {
        return Some(256_000);
    }
    // Moonshot's original K2 line launched at 128k — the floor for every
    // other k2 variant (0905/k2.5/k2.6 grew to 256k on some hosts; discovery
    // or the config override recovers the difference).
    if id.contains("kimi") || id.starts_with("k2") {
        return Some(131_072);
    }
    // xAI (docs.x.ai, verified 2026-07-17): Grok 4.5 is 500k; Grok 4.3 is 1M.
    if id.contains("grok-4.5") {
        return Some(500_000);
    }
    if id.contains("grok-4.3") {
        return Some(1_000_000);
    }
    // Grok 4 base is 256k; Grok 3 is 131k (docs.x.ai).
    if id.contains("grok-4") {
        return Some(256_000);
    }
    if id.contains("grok-3") {
        return Some(131_072);
    }
    // Qwen3.7-Max: a 1M window with 991.8k of it INPUT — preflight sizes the
    // input. MiniMax-M3: 1M. (Both verified 2026-07-17.)
    if id.contains("qwen3.7-max") {
        return Some(991_800);
    }
    // Qwen3 (non-Max, incl. coder/thinking variants): 256k native.
    if id.contains("qwen3") {
        return Some(262_144);
    }
    if id.contains("minimax-m3") {
        return Some(1_000_000);
    }
    // MiniMax M1 documented 1M; M2 documented 204,800.
    if id.contains("minimax-m1") {
        return Some(1_000_000);
    }
    if id.contains("minimax-m2") {
        return Some(204_800);
    }
    // Meta: Llama 4 (Maverick's documented 1M is the family floor — Scout's
    // 10M is never assumed); Llama 3.x is 128k.
    if id.contains("llama-4") || id.contains("llama4") {
        return Some(1_000_000);
    }
    if id.contains("llama-3") || id.contains("llama3") {
        return Some(131_072);
    }
    // Mistral's current lines are all 128k except Codestral's 256k.
    if id.contains("codestral") {
        return Some(256_000);
    }
    if id.contains("mistral-large")
        || id.contains("mistral-medium")
        || id.contains("mistral-small")
        || id.contains("magistral")
        || id.contains("devstral")
    {
        return Some(131_072);
    }
    // Cohere: Command A is 256k; Command R / R+ are 128k.
    if id.contains("command-a") {
        return Some(256_000);
    }
    if id.contains("command-r") {
        return Some(131_072);
    }
    // Google Gemma 3+ and NVIDIA Nemotron: 128k documented floors.
    // OpenAI's open-weight gpt-oss line: 131k.
    // Both spellings ship: "gemma-3-27b" (hyphenated orgs) and "gemma4"
    // (ollama's tag style).
    if id.contains("gemma-3") || id.contains("gemma-4") || id.contains("gemma3") || id.contains("gemma4") {
        return Some(131_072);
    }
    if id.contains("nemotron") {
        return Some(131_072);
    }
    if id.contains("gpt-oss") {
        return Some(131_072);
    }
    // Still unlisted on purpose: local/Ollama-served models (the window is a
    // SERVER setting, not a model fact — `context.windows` is the only honest
    // source) and families without a documented figure. Unknown → None → the
    // configured fallback.
    None
}

#[cfg(test)]
#[path = "tests/model_windows_tests.rs"]
mod tests;
