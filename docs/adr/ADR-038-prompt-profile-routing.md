# ADR-038 — Prompt-profile routing (light/full) with measure-first gate

Status: accepted · implemented 2026-07-17 (P0–P3; gate ran on real telemetry
→ A. Deviations: P0(a) grew a per-model `context.windows` config override +
live window discovery (OpenRouter/Anthropic metadata endpoints) after the
static table proved stale on arrival; the light Tier-0 header is light-only
so full stays byte-identical; drift-probe EVALS remain the open follow-up —
the re-anchor itself shipped.)

**Context:** Post-SPL, a chat turn still carries ~9k of fixed prefix before
history — protected blocks (SYSTEM_PROMPT + constitution, ~6.5k, owner
constraint: verbatim every turn) plus the P4-gated catalog (≤2.5k). The <10k
per-turn target is unreachable by trimming; it requires routing. Research
plan v3 (`docs/proposal/token-efficiency-dynamic-routing-v1.md`, gitignored)
answered the design questions: heuristics-first routing is accepted practice
but needs escalation telemetry; tool-selection degrades past ~15–20 visible
tools (~6 is comfortably safe; Anthropic measured accuracy GAINS from
deferred loading); a verbatim identity block does not by itself prevent
persona drift (attention decay) — re-anchoring at compaction does.

**Decision:** route between two byte-stable prompt profiles at session
boundaries, gated by a measurement phase that decides the final shape on
real numbers. Phased:

- **P0 — measure, no behavior change.** (a) `max_context_tokens` derives
  from the ACTIVE model's catalog entry, failover-aware — compaction math
  follows whoever serves the turn, not the static 128k default. (b)
  `profile.estimate`: `fixed_prefix()` renders BOTH candidates (light =
  minimal pinned set; full = today's) and reports token sizes through
  `context.budget`. (c) A session-mix report over existing telemetry
  (turns/session by `SessionKind`, per-turn `input_tokens`, tool-use mix)
  computes the analytic A-vs-B comparison per provider cache model:
  A = two profiles (one reset per escalation) vs B = one prefix +
  mode-by-message (Claude Code pattern, §3.1 of the plan). **Gate: B wins
  on billed tokens for the real session mix → P1/P2 are cancelled and B's
  message-injection (already the Tier-3/code_skill seam) ships instead.**
- **P1 — the `light` profile.** Resolved at `create_session_keyed` from
  `SessionKind`: Chat → light; CodePlan/CodeExecute/voice/delegate → full.
  Light keeps the protected blocks verbatim and pins ~6 tools
  (`memory_search`, `session_search`, `current_time`, `load_tools`,
  `skill_view`, `code_task` — the escalation trigger must stay visible);
  everything else rides the deferred index. A one-line Tier-0 profile
  header leads the prompt so implicit-cache providers (OpenAI ~256-token
  routing hash) keep two independent warm caches.
- **P2 — one-way escalation.** Mid-session `load_tools`/`code_task`/
  `delegate_task` call → next prompt build is full, tagged
  `cache_reset: profile`; never a downgrade (oscillation busts caches).
  Escalation-rate report ships WITH the feature, not after — the
  documented production failure mode is silent escalation drift.
- **P3 — identity guards.** Persona-drift probes join the eval suite
  (character-source question at turn ~1, ~50, post-compaction, both
  profiles); compaction injects a one-line identity re-anchor (free:
  compaction resets the cache there anyway).

Acceptance (plan §6): light ≤9k / full ≤15k in `context.budget`; ≤2 resets
in a 20-turn mixed session; deferred-recovery eval ≥90% on light; memory-
precision probes pass on both profiles; the <10k claim is a queryable
report, not an estimate.

**Consequences:** chat turns drop ~5k of catalog+extras; agentic turns are
untouched. Storage/recall paths are untouched by construction (plan §9).
Risk: a misrouted first turn pays one escalation reset — accepted, cheap,
and measured. The P0 gate means we may ship Alternative B and never build
profiles at all; that outcome is a success, not a failure of this ADR.
