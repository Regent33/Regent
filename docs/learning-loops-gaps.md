# Learning Loops — Hermes vs Regent, gaps & improvements

**Status: ANALYSIS (2026-06-21).** How Hermes auto-learns, how Regent does, and the concrete gaps.

## How Hermes auto-learns (two loops)

1. **Per-turn background review** (`agent/background_review.py`) — after a turn, a forked agent
   (daemon thread, **memory + skill tools only**, parent's cached prompt untouched) replays the
   conversation and asks *"should any skill/memory be saved or updated?"* — writes straight to the
   memory + skill stores.
2. **Inactivity curator** (`agent/curator.py`) — when the agent is idle and the last run was >
   `interval_hours` ago, `maybe_run_curator()` forks an auxiliary agent to maintain **agent-created
   skills**: auto-transition lifecycle (stale→archived), pin/consolidate/patch. Never deletes
   (archive is recoverable); pinned skills are exempt; never touches the main prompt cache.

## How Regent learns today

- **Post-turn review fork** (`regent-agent/.../review.rs` + `REVIEW_SYSTEM_PROMPT`) — ✅ mirrors
  Hermes: whitelisted to memory + skill tools, replays the snapshot, persists learning. The prompt
  already captures user identity/preferences (`memory` tool, `user` scope) → **graph memory**
  (`regent-graph`), rendered into the system prompt via `graph.render_prompt_block()`.
- **Curator** (`regent-skills/.../curator.rs`) — ✅ implemented (stale→archive over usage telemetry,
  agent-created only, pinned-exempt).
- **Persona** (new, this session) — DB `persona` table (`soul` + `about`), user-editable + an
  `update_persona` tool + a base-prompt directive telling the main agent to proactively append
  learned user prefs to `about`.

## Gaps (highest-impact first)

1. **🔴 The curator never runs.** `curator.rs` exists but **nothing in the daemon triggers it** — no
   inactivity timer, no cron job. So skills accumulate and are never archived/consolidated. Hermes
   runs it inactivity-triggered. **Fix:** add a daemon trigger — either an idle-timer (last-activity
   + `interval_hours`, like Hermes) or a seeded internal cron job calling `curator.run`. (The
   parity plan's B5.3 `curator run` exposes a manual trigger; auto-run is the missing half.)

2. **🟠 Two stores for "what we know about the user."** The review fork writes user prefs to the
   **graph** (`user` nodes); the new **persona `about` table** is a *separate* path (user-editable +
   the proactive `update_persona` directive). Same facts can land in both → drift. **Fix (decide):**
   either (a) `about` = a small *user-curated* summary and the graph stays the auto-learned detail
   (then `persona_block()` could also surface top graph user-facts), or (b) the review fork writes
   user prefs via `update_persona` instead of the graph. Recommend (a) — keep the graph as the
   learning substrate; treat `about` as the human-editable override.

3. **🟡 Review fork can't touch the persona.** Its whitelist is memory + skill tools only, so the
   dedicated learning pass can't update `soul`/`about`. Consistent with picking 2(a); if 2(b),
   add `register_persona_tool` to the review catalog + a line in `REVIEW_SYSTEM_PROMPT`.

4. **🟡 Review only fires on success.** `spawn_review_if_configured` runs after `result.is_ok()`;
   a corrected/aborted turn (often the richest "don't do that again" signal) is skipped. Hermes
   reviews every turn. **Fix:** also review on interrupt/error (the snapshot is still valid).

5. **🟢 No memory hygiene loop for the graph.** The curator maintains *skills*; there's no
   equivalent TTL/decay/consolidation pass for *graph memory* (semantic). Hermes' memory has write
   policy + review triggers (§10.5). **Fix:** a periodic graph-memory review (dedupe, decay, pin).

## Recommended order

1. **Wire the curator to auto-run** (gap 1) — biggest, self-contained; skills are already curated,
   they just need a trigger.
2. **Decide the user-facts model** (gap 2) — recommend 2(a); cheap once decided.
3. **Review on failure too** (gap 4) — one-line change in `run_turn`.
4. Later: graph-memory hygiene pass (gap 5).

## Verification (per fix)

`cargo test --workspace` + `clippy -D warnings`; for the curator trigger, a unit/integration test
that an idle tick archives a stale agent-created skill and leaves pinned/user skills untouched.
