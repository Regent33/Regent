# Learning Loops — Hermes vs Regent, gaps & improvements



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

1. **✅ DONE — the curator now runs.** `spawn_curator` (daemon `background.rs`) runs a periodic
   (6h) pass transitioning stale agent-created skills toward archived; deterministic, idempotent,
   pinned/user skills exempt; wired beside TTL-purge / pending-expiry.

2. **✅ RESOLVED (2a) — `about` is the user-curated override; the graph is the auto-learned
   substrate.** The review fork keeps writing user prefs to the **graph** (`user` nodes); the
   persona `about` table is the small human-editable summary the user (or `update_persona`)
   maintains. No rewrite — they're intentionally different layers. (Future polish: have
   `persona_block()` also surface top graph user-facts so the two layers visibly compose.)

3. **✅ N/A under 2(a).** The review fork stays whitelisted to memory + skill tools (it writes user
   facts to the graph); it deliberately does not touch the curated `about`/`soul`.

4. **✅ DONE — review fires on partial-failure too.** `run_turn` now spawns the review fork on
   success *and* on an interrupted turn that left a partial tool exchange (settling pending tools
   produced rows). A turn that reverted to pre-turn state is still skipped (nothing new to learn).

5. **🟢 No memory hygiene loop for the graph.** The curator maintains *skills*; there's no
   equivalent TTL/decay/consolidation pass for *graph memory* (semantic). Hermes' memory has write
   policy + review triggers (§10.5). **Fix:** a periodic graph-memory review (dedupe, decay, pin).

## Remaining

- **Gap 5 (deferred)** — a periodic graph-memory hygiene pass (dedupe / decay / consolidation),
  the semantic-memory analog of the skill curator. Sizable new feature, not a quick fix.
- **Polish under 2(a)** — surface top graph user-facts inside `persona_block()` so the curated
  `about` and the auto-learned graph visibly compose in the prompt.

## Verification (per fix)

`cargo test --workspace` + `clippy -D warnings`; for the curator trigger, a unit/integration test
that an idle tick archives a stale agent-created skill and leaves pinned/user skills untouched.
