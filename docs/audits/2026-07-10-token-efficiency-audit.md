# Token-efficiency audit — why every turn cost ≥30k input tokens

**Date:** 2026-07-10 · **Trigger:** user report ("input is at least 30k per
turn — Regent is not token efficient") · **Method:** live JSON-RPC probes
against the real store + source reading, no guesswork.

## Measured breakdown (this machine, before the fix)

Every chat turn re-sends the assembled system prompt + tool schemas + history.
The fixed (history-independent) spend was:

| Block | Chars | ~Tokens | Bounded? |
|---|---|---|---|
| `soul` persona row | 47,647 | ~11.9k | **NO — root cause** |
| `about` persona row | 15,291 | ~3.8k | **NO — root cause** |
| `constitution` (opt-in, ADR-028) | 9,056 | ~2.3k | by the user's choice |
| SYSTEM_PROMPT + CAPABILITIES (static) | ~17,000 | ~4.3k | yes (source constant) |
| Tool schemas (30 tools) | 13,254 | ~3.3k | grows only with new tools |
| Graph memory block (MEMORY + USER) | ≤3,575 | ~0.9k | yes (2,200 + 1,375 budgets) |
| Skills index (16 skills) | ~1,800 | ~0.5k | grows with each skill |

Fixed total ≈ **27k tokens** before a single message of history — matching the
report exactly.

## Root cause

Graph memory was budgeted from day one (over-budget writes error with the
current entries so the agent consolidates). **Persona rows never were.** The
`update_persona` tool's `append` action let every call session accrete
episodic notes into `soul` — 98 bullets by audit day, many duplicated verbatim
2–3×, several stale and later self-corrected. The entire block rides every
turn's system prompt, so the cost compounded silently.

## Fix (shipped 2026-07-10)

1. **`persona_budget(key)` in regent-store** — soul 8,000 chars, about 6,000,
   `about.<facet>` 2,000 each, constitution 12,000. Enforced in
   `Store::set_persona` (one seam: covers the agent tool, the `persona.set`
   RPC, and the CLI). Over-budget writes fail with consolidation guidance —
   the graph-entries pattern applied to persona.
2. **One-time consolidation** of this machine's rows (originals backed up at
   `~/.regent/persona-backup-2026-07-10/`): soul 47,647 → 6,540 chars, about
   15,291 → 3,522. Every distinct durable rule kept; duplicates merged; stale
   facts (pre-Qwen3 voice stack, "CLI is Rust") dropped.

**Expected result: ~30k → ~17-18k input tokens per turn**, no change to the
system prompt's structure or content design.

## Will Regent stay token-efficient over time?

Per growth vector, after this fix:

- **Persona (was the runaway):** now hard-bounded at 36k chars absolute
  worst-case (~9k tok) if every row is filled to its cap; ~4k tok as
  currently written. The budget error forces consolidation instead of
  accretion. **Stays flat.**
- **Graph memory:** always bounded (3,575 chars). **Stays flat.**
- **Conversation history:** grows within a session but compression
  (`context.trigger_fraction`, `protect_last_n`) compacts it against
  `context.max_tokens`. **Bounded per session.**
- **Tool schemas (~3.3k tok):** grows only when new tools ship, not with
  usage. The `tools.deferred` config (ADR-031) can withhold rare tools'
  schemas until loaded — an available lever if the catalog doubles.
- **Skills index (~0.5k tok):** the one remaining *usage-driven* growth
  vector — every learned skill adds its index line (~50–100 chars). At the
  current pace this is years from mattering; if the library reaches hundreds
  of skills, cap the rendered index (most-recently-used N + `skills_list`
  for the rest) the same way persona got capped.

**Answer: yes — by design, now.** The architecture's stated principle
(bounded prompt stores, consolidate-don't-accrete) was applied everywhere
except persona; that hole is closed. The remaining growth vectors are either
build-time (tools) or slow enough to flag long before they hurt (skills
index). The thing to keep honoring in future features: **anything injected
into every turn's prompt must have a hard budget and a consolidation path.**

## Startup latency (asked in the same audit)

Live timings: deacon cold boot **34ms**, `session.list` (950 sessions)
**29ms**, `session.create` **6ms**, `memory.list` **2ms**. The backend was
never the bottleneck. The perceived slowness on audit day was the serialized
stdin dispatcher wedged behind an awaited 30-model-call title sweep (fixed
the same day — the sweep is detached now). Remaining startup cost is WebView2
window creation plus the desktop's 300ms splash fade; the entry chunk was
already cut 5.8× (1.87MB → 322kB) in the Vite migration.
