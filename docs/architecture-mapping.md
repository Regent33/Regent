# Architecture Mapping — canonical tree ↔ Regent workspace

The project's canonical layout is the feature-based clean architecture tree from the master
prompt (`src/app · shared · agents · features`). That tree is TypeScript-shaped; the Rust core is
a cargo workspace, and per the master prompt's own legacy policy ("conform to the repo — do not
restructure"), the workspace keeps `crates/` while **enforcing the same layers**. This table is
the contract; new code must land in the layer its crate maps to.

| Canonical location | Regent home | Notes |
|---|---|---|
| `shared/kernel/types` + `contracts` | `crates/regent-kernel` | The ONLY freely-importable crate. Branded IDs, typed `RegentError` (base Failure), message/transcript types, `ToolDefinition` contract. No I/O. |
| `shared/infrastructure/storage` | `crates/regent-store` | Pure persistence (SQLite WAL/FTS5). Zero domain logic — graph *semantics* are explicitly kept out. |
| `shared/infrastructure/http` (LLM client) | `crates/regent-providers` | Provider adapters + retry/backoff (via `or-core`). Wire formats only. |
| `shared/infrastructure/logger` | `tracing` + subscriber at the app edge | Structured logging adapter; subscriber installed only in binaries. |
| `agents/orchestrator` | `crates/regent-agent` | The harness loop, budgets, stop conditions, compression lifecycle. Planner/router arrive with delegation (M4). |
| `agents/memory` (semantic · episodic · procedural · prospective) | `crates/regent-graph` (semantic+episodic), `crates/regent-skills` (procedural), `regent-store` sessions (episodic transcript), cron at M4 (prospective) | Tier taxonomy per master prompt §10.1. |
| `agents/tools/[tool]/definition` | `ToolDefinition` values in `regent-tools` (`*_definition()` fns) | What the model sees — schema only. |
| `agents/tools/[tool]/executor` | `ToolExecutor` impls in `crates/regent-tools` | Validates input, never bypasses domain logic. |
| `agents/prompts` | `crates/regent-agent/src/compression.rs` consts + `regent-skills` prompts (review prompt) | Versioned with the crate; prompt changes gate on evals. |
| `agents/evals` | `crates/regent-graph/tests/golden_retrieval.rs` (+ future per-agent golden sets) | Regression gates for schema/scoring/prompt changes. |
| `agents/middleware` (output-validator · safety-guard · rate-limiter · tracer) | `regent-tools` guard + catalog error-wrapping (safety/validator), `or-core` RetryPolicy + provider retry (rate), `tracing`/or-lens (tracer) | Consolidated middleware layer is on the roadmap as the gateway lands. |
| `app/` (root shell, di, config) | `crates/regent-agent/src/bin/repl.rs` today → `regent-daemon` + Go CLI later | Composition root: builds catalog, graph, provider chain, snapshot prompt. All DI is constructor injection. |
| `features/[feature]/…` | Future Rust crates / TS packages per feature | Gateway platforms, dashboard, desktop (M5+ — TS surfaces apply the canonical tree **literally**). |

Rules carried over regardless of language (this is **feature-based clean architecture** — the
layering contract, not just folders):
1. Dependencies point inward: `presentation → domain ← data`. Everything may import
   `regent-kernel`; `regent-store` imports nothing domain-shaped; domain code never imports
   binaries or infra.
2. Domain owns entities, **repository interfaces**, and use cases — zero framework/infra
   imports. Implementations live in infra/data.
3. Two-file tool contract: definition (model-facing) is separate from executor (validating,
   use-case-calling) even when both live in one Rust module.
4. Composition roots (`app/di`) are the only place handles are wired together — constructor
   injection only.
5. **Crate-internal layout**: new Regent crates use the `domain/` + `application/` + `infra/`
   module structure (the same convention the Orchustr workspace uses: `domain/contracts·
   entities·errors`, `application/orchestrators`, `infra/adapters·implementations`).
   `regent-skills` (M3) is the reference example; pre-M3 crates migrate opportunistically.
