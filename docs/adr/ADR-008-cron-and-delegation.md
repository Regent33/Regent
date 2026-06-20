# ADR-008: Cron as prospective memory; delegation as bounded ordered fan-out

**Status:** Accepted

**Context:** M4 needs scheduled jobs (Hermes cron) and subagent delegation (Hermes
`delegate_task`). or-colony was evaluated for the fan-out: its parallel mode is fail-fast
`try_join_all` with no concurrency cap and a static roster — incompatible with the Hermes
invariants (max 3 concurrent children, per-child failure isolation, dynamic task lists).

**Decision:** `regent-cron` (clean-arch crate) owns the schedule domain (`30m`/`2h`/`1d`,
`daily HH:MM`, one-shot; full cron expressions deferred) and a scheduler that enforces every
hardening invariant in harness code: file **tick lock** (stale-broken after 10 min), **hard run
timeout** (default 180 s — a stuck job advances the schedule anyway), **catch-up clamp**
(period/2 clamped to [120 s, 2 h]; one-shot grace 120 s; missed-beyond-window = skip forward,
never run), one-shots retire instead of being deleted. Runs use a fresh agent (`source: cron`,
no graph memory, no review — the Hermes `skip_memory` rule) via `AgentJobRunner`.

Delegation lives in `regent-agent` (an orchestrator concern): `DelegateTool` fans tasks out to
leaf sub-agents over `stream::buffered(max_concurrent=3)` — bounded **and** order-preserving —
each child getting only its brief + a leaf catalog (no delegate/memory), its own session and
budget (50), failures isolated per child. Once or-colony gains a concurrency cap and per-member
error isolation upstream, this becomes a ColonyOrchestrator adapter (the ADR-002 path).

**Consequences:** Prospective memory survives restarts as `cron/jobs.json`; the orchestrator
role (children that can themselves delegate, depth 2) is deferred until a real need appears.
