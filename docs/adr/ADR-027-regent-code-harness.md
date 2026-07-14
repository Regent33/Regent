# ADR-027 — `regent-code` coding harness (plan-mode → verify → revert)

**Status:** accepted · **Date:** 2026-07-01 · §F P1. Builds on the editing tools (file_edit/apply_patch/glob/search_files/ls) and the sandbox already shipped.

## Context
The editing *craft* already exists as tools the agent uses today. What was missing
is the coding-specialized *harness* — the disciplined loop the best coding agents get right:
a read-only plan gate, per-step verify, and revert-to-last-green on failure. The
constraint was to add this without re-implementing the turn loop (`regent-agent`
owns budgets/interrupts/compression) and without destabilizing the live daemon.

## Decision
- **New `regent-code` crate** (domain/application/infra). Domain is pure + unit-tested:
  `detect_build_tool` (repo manifests → Cargo/Npm/Make/Pytest), `plan_toolset`
  (read-only subset `read_file`/`glob`/`search_files`/`ls`), `parse_verify`.
- **Plan-mode is enforced structurally, not by prompt:** the plan turn runs with a
  catalog *restricted* to the read-only subset (`ToolCatalog::restrict_to`), so
  write/terminal tools are physically absent. Phase prompts adopt a strict
  plan-mode discipline (read-only supersedes; explore + reuse; structured plan).
- **Verify + revert are infra ports** (`Verifier`, `Checkpoint`) so the loop is
  testable without real builds. `GitCheckpoint` snapshots before execute and, on a
  failed verify, restores tracked files + removes newly-created ones; outside a git
  repo it degrades to report-only (surfaced, never silent).
- **Surface = the daemon's existing session path (approach A):** `code.plan` /
  `code.start` run the plan/execute turns as normal sessions, so approval prompts,
  streaming, and interrupt are reused rather than re-plumbed. `CodeHarness` (the
  standalone struct) is the tested embodiment of the same flow for embedders.

## Consequences
- Additive: no turn-loop change; the live chat path is untouched. The plan/execute
  phases are isolated sessions (sub-agent isolation) — the plan text is the contract.
- The harness orchestration is expressed twice (the tested `CodeHarness` struct and
  the daemon's thin `code_start`); accepted to keep the daemon on its richer
  streaming/approval path. Revert's known ceiling: a tool that `git add`s a new file
  escapes the untracked-diff cleanup (built-in edit tools don't stage).
- P2 (deferred): auto-routing arbitrary coding chat through the harness, worktree
  isolation, and code-context RAG — built on this proven core, not before it.
