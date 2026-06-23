# ADR-022: Structured user profile + memory routing

**Status:** Accepted (2026-06-23)

**Context:** The `about` user profile was one free-text blob, and the agent dumped
transient state into it (current downloads, today's task, one-off paths) because
nothing told it what belongs where. The architecture proposal (§5.3) already maps
the 7 memory types to subsystems; the profile is *semantic* memory of kind
`persona`/`preference`, not a scratchpad.

**Decision:**
- Split the profile into **five stable facets** — identity · preferences · habits ·
  constraints · goals — stored as persona keys `about.<facet>` (no schema change; the
  `persona` table is already KV). The bare `about` key stays a back-compat catch-all.
  `persona_block()` renders each non-empty facet as `### Heading`.
- `update_persona` gains a `section` arg (target `user`); its description plus the
  `memory` tool's now state the routing: **profile → the 5 facets (durable only);
  world/work facts → `memory`; what happened → session history (episodic); how-to →
  skills (procedural); future intents → cron (prospective).**
- CLI: `regent about <facet> <show|set|add|edit|clear>` (full CRUD per facet);
  unknown facets error. `regent about` shows all facets.

**Consequences:** Additive + back-compat (legacy blobs still render; existing
`regent about set` still works). No new graph tables — episodic/procedural/prospective
already exist as sessions/skills/cron, so we route to them rather than duplicate them.
Realizing first-class `kind`-tagged graph nodes (episode/intent) remains a later
milestone per the proposal.
