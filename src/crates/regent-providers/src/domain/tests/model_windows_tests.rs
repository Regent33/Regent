//! Unit tests for `model_windows` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::window_for_model;

#[test]
fn known_families_resolve_to_documented_windows() {
    // Claude 5 generation + the 4.6+ wave: 1M standard.
    assert_eq!(window_for_model("claude-fable-5"), Some(1_000_000));
    assert_eq!(window_for_model("claude-sonnet-5"), Some(1_000_000));
    assert_eq!(window_for_model("claude-opus-4-8"), Some(1_000_000));
    assert_eq!(window_for_model("claude-sonnet-4-6"), Some(1_000_000));
    // OpenRouter spelling (dots) + suffixed variant.
    assert_eq!(
        window_for_model("anthropic/claude-opus-4.8-fast"),
        Some(1_000_000)
    );
    // Haiku tiers and older opus/sonnet stay on the 200k floor.
    assert_eq!(window_for_model("claude-haiku-4-5"), Some(200_000));
    assert_eq!(window_for_model("claude-sonnet-4-5"), Some(200_000));
    assert_eq!(window_for_model("claude-3-haiku-20240307"), Some(200_000));
    assert_eq!(window_for_model("gpt-5.6-sol"), Some(1_050_000));
    assert_eq!(window_for_model("openai/gpt-5.6-luna"), Some(1_050_000));
    assert_eq!(window_for_model("gpt-5.5-pro"), Some(1_000_000));
    assert_eq!(window_for_model("gpt-5"), Some(272_000));
    assert_eq!(window_for_model("gpt-5-mini"), Some(272_000));
    assert_eq!(window_for_model("gemini-3.5-flash"), Some(1_048_576));
    assert_eq!(
        window_for_model("deepseek/deepseek-v4-flash"),
        Some(1_048_576)
    );
    assert_eq!(window_for_model("z-ai/glm-5.2"), Some(1_000_000));
    assert_eq!(window_for_model("moonshotai/kimi-k2.7-code"), Some(256_000));
    assert_eq!(window_for_model("moonshotai/kimi-k3"), Some(1_048_576));
    assert_eq!(window_for_model("k3"), Some(1_048_576));
    assert_eq!(window_for_model("x-ai/grok-4.5"), Some(500_000));
    assert_eq!(window_for_model("grok-4.3"), Some(1_000_000));
    assert_eq!(window_for_model("qwen/qwen3.7-max"), Some(991_800));
    assert_eq!(window_for_model("MiniMax-M3"), Some(1_000_000));
    assert_eq!(window_for_model("gpt-4.1"), Some(1_000_000));
    assert_eq!(window_for_model("gpt-4o-mini"), Some(128_000));
    assert_eq!(window_for_model("gpt-4-turbo"), Some(128_000));
    assert_eq!(window_for_model("o1-preview"), Some(200_000));
    assert_eq!(window_for_model("o3-mini"), Some(200_000));
    assert_eq!(window_for_model("o4-mini"), Some(200_000));
    assert_eq!(window_for_model("gemini-1.5-pro-latest"), Some(2_000_000));
    assert_eq!(window_for_model("gemini-1.5-flash"), Some(1_000_000));
    assert_eq!(window_for_model("gemini-2.5-pro"), Some(1_048_576));
}

// The broad-coverage wave (owner ask 2026-07-17: "ALL the other LLM
// providers"): documented family FLOORS — a host serving more is recovered
// by live discovery or `context.windows`; stale-low only compacts early.
#[test]
fn broad_provider_families_resolve_to_documented_floors() {
    // DeepSeek: reasoner line 64k, chat/V3 line 128k (V4 tested above).
    assert_eq!(window_for_model("deepseek-r1"), Some(65_536));
    assert_eq!(window_for_model("deepseek-reasoner"), Some(65_536));
    assert_eq!(window_for_model("deepseek-chat"), Some(131_072));
    assert_eq!(window_for_model("deepseek/deepseek-v3.1"), Some(131_072));
    // GLM: 4.6 is 200k, the rest of the 4/5 lines 128k (5.2 tested above).
    assert_eq!(window_for_model("z-ai/glm-4.6"), Some(200_000));
    assert_eq!(window_for_model("glm-4.5-air"), Some(131_072));
    assert_eq!(window_for_model("glm-5.1"), Some(131_072));
    // Moonshot: every non-k2.7/k3 kimi rides the original 128k floor.
    assert_eq!(window_for_model("kimi-latest"), Some(131_072));
    assert_eq!(window_for_model("moonshotai/kimi-k2.5"), Some(131_072));
    assert_eq!(window_for_model("kimi-k2-thinking"), Some(131_072));
    // xAI: grok-4 base 256k, grok-3 131k (4.5/4.3 tested above).
    assert_eq!(window_for_model("x-ai/grok-4"), Some(256_000));
    assert_eq!(window_for_model("grok-3-mini"), Some(131_072));
    // Qwen3 non-Max: 256k native.
    assert_eq!(window_for_model("qwen/qwen3-coder"), Some(262_144));
    assert_eq!(window_for_model("qwen3-235b-a22b"), Some(262_144));
    // MiniMax M1 (1M) and M2 (204.8k); M3 tested above.
    assert_eq!(window_for_model("minimax-m1"), Some(1_000_000));
    assert_eq!(window_for_model("minimax/minimax-m2"), Some(204_800));
    // Meta Llama: 4 → 1M family floor (Scout's 10M never assumed), 3.x → 128k.
    assert_eq!(
        window_for_model("meta-llama/llama-4-maverick"),
        Some(1_000_000)
    );
    assert_eq!(window_for_model("llama-3.1-70b"), Some(131_072));
    assert_eq!(window_for_model("meta-llama/llama-3.3-70b"), Some(131_072));
    // Mistral: 128k lines + Codestral 256k.
    assert_eq!(
        window_for_model("mistralai/mistral-large-2411"),
        Some(131_072)
    );
    assert_eq!(window_for_model("mistral-small-3.2"), Some(131_072));
    assert_eq!(window_for_model("magistral-medium"), Some(131_072));
    assert_eq!(window_for_model("codestral-2501"), Some(256_000));
    assert_eq!(window_for_model("devstral-small"), Some(131_072));
    // Cohere: Command A 256k, Command R 128k.
    assert_eq!(window_for_model("cohere/command-a"), Some(256_000));
    assert_eq!(window_for_model("command-r-plus"), Some(131_072));
    // Gemma, Nemotron, gpt-oss: 128k floors. Both spellings ("gemma-4" org
    // ids, "gemma4" ollama tags) resolve.
    assert_eq!(window_for_model("google/gemma-3-27b-it"), Some(131_072));
    assert_eq!(window_for_model("google/gemma-4-26b-a4b-it"), Some(131_072));
    assert_eq!(window_for_model("gemma4"), Some(131_072));
    assert_eq!(window_for_model("gemma4:cloud"), Some(131_072));
    // Gemini 3 (flash preview) rides the 1M gemini floor.
    assert_eq!(window_for_model("gemini-3-flash-preview"), Some(1_048_576));
    assert_eq!(
        window_for_model("nvidia/nemotron-3-ultra-550b-a55b"),
        Some(131_072)
    );
    assert_eq!(window_for_model("openai/gpt-oss-120b"), Some(131_072));
}

#[test]
fn provider_prefix_and_case_are_tolerated() {
    assert_eq!(window_for_model("anthropic/Claude-Opus-4"), Some(200_000));
    assert_eq!(window_for_model("openai/gpt-4o"), Some(128_000));
}

#[test]
fn specific_ids_are_not_shadowed_by_broader_ones() {
    // gpt-4.1 must win its 1M window even though it also contains "gpt-4".
    assert_eq!(window_for_model("gpt-4.1-mini"), Some(1_000_000));
    // deepseek-v4 (1M) and -r1 (64k) beat the broad deepseek 128k rung.
    assert_eq!(window_for_model("deepseek-v4-pro"), Some(1_048_576));
    // kimi-k3 (1M) beats the broad kimi 128k floor.
    assert_eq!(window_for_model("kimi-k3-turbo"), Some(1_048_576));
    // grok-4.5 (500k) beats the broad grok-4 256k rung.
    assert_eq!(window_for_model("grok-4.5-fast"), Some(500_000));
}

#[test]
fn unknown_models_return_none() {
    assert_eq!(window_for_model("scripted-model"), None);
    assert_eq!(window_for_model("gpt-4"), None); // ambiguous base — not guessed
    assert_eq!(window_for_model("phi-4"), None); // small local families stay on config
    assert_eq!(window_for_model(""), None);
}
