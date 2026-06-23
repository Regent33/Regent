# ADR-023: Persistent named agents + board execution

**Status:** Accepted (2026-06-23)

**Context:** Regent had ephemeral, anonymous delegation (`delegate_task`) and a
durable kanban board, but no **named, persistent agents** — the board dispatcher
ran one generic worker and ignored a task's assignee. Hermes's model is the
opposite: tasks are assigned to named worker profiles and the dispatcher spawns
the assigned one. (issue #3)

**Decision:**
- **Registry:** a named agent = `{ name, description, system_prompt, model?, tools? }`
  in a new additive `agents` table (`CREATE TABLE IF NOT EXISTS`, no migration).
  Managed by `regent agents list|create|show|edit|remove`; `agents.*` RPC.
- **Assignment ≠ start.** `kanban assign <task> <agent>` now sets the assignee but
  leaves the task in `todo` (new `assign_task`), so the dispatcher can claim and
  run it. `claim_task` preserves a pre-set assignee via `COALESCE` — only an
  unassigned task takes the claimer's id.
- **Execution:** the board runner resolves `task.assignee` → its agent definition
  and runs with that agent's **system prompt** and **tool allow-list**
  (`ToolCatalog::restrict_to`); an unknown/absent assignee falls back to the
  default worker. The `model` override is stored but not yet applied at the board
  layer (the dispatcher holds a fixed provider) — a follow-up.

**Consequences:** Additive + tested (store CRUD, catalog restrict, 3 runner
resolution cases). `kanban assign` no longer auto-starts a task — a behavior
change, but it separates ownership from progress and matches the board model
(`kanban start` still moves todo→in_progress manually). Delegation-by-name (a
live turn spawning a named sub-agent) is deferred.
