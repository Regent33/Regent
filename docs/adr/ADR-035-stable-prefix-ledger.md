# ADR-035 — The Stable-Prefix Ledger (SPL P1)

**Status:** accepted (2026-07-10) · **Context:** `docs/proposal/token-efficiency-architecture-v1.md`

## Context

~14k tokens of every turn's prompt are byte-identical to the previous turn.
Providers serve a byte-stable prefix from cache at ~2–10% of the price — but
nothing measured that stability, so one per-turn `format!` in the wrong place
would silently double the bill forever.

## Decision

A `Ledger` (deacon `domain/ledger.rs`) is the ONLY code path that concatenates
the system prompt. Each segment carries a stability tier (Tier 0 PROCESS /
Tier 1 SESSION); render is byte-identical to the historical assembly. The
build-time render + sealed tool-definitions serialization form the baseline;
each turn re-hashes what the agent actually sends (frozen prompt string,
re-serialized defs — never live store reads) and a mismatch logs a `cache_bust`
warning naming tier + segment, fail-open. Build-time tier hashes ride
`turn.complete` as additive fields. CI gates: 80k-char prefix ceiling, 50-turn
hash stability, injected-timestamp trip.

## Consequences

New features must not inject per-turn content into Tier 0/1 — the hash test
fails until it's budgeted or moved to the volatile tail. Env-derived lines
(`now_line` etc.) stay read-once-at-spawn. Tool-schema serialization must stay
deterministic. P2 (cache_control adapters) places breakpoints at tier
boundaries; cadence study (`docs/audits/2026-07-10-cadence-study.md`) gates it.
