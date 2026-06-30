# ADR-026 — Multi-provider registry & per-agent models

**Status:** accepted · **Date:** 2026-06-30 · supersedes nothing; extends ADR-002 (native tool-calls in `regent-providers`).

## Context
Regent spoke to one provider per session (`model` config + env) with a sticky
`FallbackChat` already in place. The next batch (§A) needs many providers
configured at once, per-agent model selection, and automatic fallback — without
re-architecting the working single-provider path or the prompt-cache invariant.

## Decision
- **`ModelRef { provider, model }`** is a kernel value type (freely importable;
  the only shared piece). Provider-aware parsing of `"provider/model"` strings
  lives in the registry, which knows the configured names — the type stays pure.
- **`config.providers`** is an additive map (`name → {kind, base_url, api_key_env, models}`)
  plus `agents_defaults {primary, fallbacks}`. `deny_unknown_fields` honored;
  empty = today's behavior. One `api_key_env` serves every model (multi-model-per-key).
- **`ProviderRegistry`** lives in `regent-daemon` (not `regent-providers`) because
  the provider *kinds* + `make_provider_factory` already live there; moving them
  would churn working code. It resolves+memoizes `ModelRef → Arc<dyn ChatProvider>`
  and builds per-agent chains by reusing the existing `FallbackChat`.
- Keys are read from env at resolve time, never stored (secrets stay out of config).

## Consequences
- Additive only; the single-provider path and prompt-cache freeze are untouched.
- Deviates from the plan's literal §A.1 (fields on `AgentConfig`): the `Agent`
  receives an injected provider, so model/provider resolution belongs at the
  construction seam (registry + named-agent record), not in `AgentConfig` — no dead fields.
- New providers (OpenRouter/Groq/…) are config, not code. Per-agent wiring lands
  at the board runner (the documented gap); other run paths can adopt it incrementally.
