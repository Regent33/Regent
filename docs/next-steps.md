# Regent — Full Next-Step Plan (post-M6 → complete Hermes re-implementation)

**Status: ACTIVE ROADMAP** (2026-06-12). Source of truth for what gets built next and in what
order. Grounded in the re-study [hermes-study/10-gap-analysis.md](hermes-study/10-gap-analysis.md).

**Mission restated:** recreate the *whole* of Hermes Agent — every capability, none of the warts —
in Regent's own architecture and style: Rust/Tokio core, feature-based clean architecture in every
crate, Go CLI plane, TS surface edge (M5+ amendment), Orchustr-native, SQLite+FTS5, graph memory.
Re-implementation, never a port: Hermes defines *what*; Regent defines *how*.

## Where we stand (M0–M6, all green)

9 crates (kernel, store, graph, skills, cron, gateway, providers, tools, agent) · 87 tests ·
clippy clean · Rust 1.96. Working today: hardened async loop (budgets/interrupts/fallback/
compression+lineage/turn ledger), graph memory (eval-gated hybrid retrieval, recall@5=1.00),
learning loop (skills+curator+background review), cron invariants, bounded delegation, gateway
(auth/pairing//stop bypass/approval-over-chat) + Telegram, MCP via or-mcp, docker/ssh sandboxes,
dispatch hooks. Two runnable bins: `regent-repl`, `regent-gateway`.

## Standing rules (apply to every phase)

1. Hermes invariants ledger (study #09) is binding: cache-stable prompts, narrow waist
   (Footprint Ladder), harness-owned stop conditions, default-deny security, never-delete
   lifecycles, behavior-contract tests.
2. Clean architecture per crate (`domain/application/infra`; kernel `types/contracts`) — ADR-007.
3. Edges before core: new capability lands as MCP server / skill / gated tool before a core tool.
4. Orchustr first: where an or-crate fits, adopt; where it falls short, record the gap (ADR-002,
   -008) and prefer upstreaming to forking. Targets: or-conduit native tool-calls → adopt
   or-sentinel topologies; or-colony concurrency cap → DelegateTool becomes an adapter;
   or-lens/or-prism for traces; or-mcp NexusServer for `regent mcp serve`.
5. Every phase exits with: tests green + clippy clean + CHANGELOG + ADR for constraining
   decisions + golden-eval reruns when prompts/schemas/scoring change.

---

## P1 — The CLI plane (START HERE)

*The user-facing foundation everything else plugs into — Hermes's `hermes` command, our way.*

**P1.1 `regent-daemon` (Rust)** — the long-lived core process (canonical `app/`):
- JSON-RPC 2.0 server over stdio (child-process mode) AND named pipe/unix socket (attach mode) —
  the Hermes `tui_gateway` pattern; one protocol for Go CLI now, TS surfaces later (ADR-001).
- Method/event surface (v1): `session.create/resume/list/search`, `prompt.submit` →
  streamed events `turn.started`, `tool.start/complete`, `message.complete`,
  `approval.request/respond`, `clarify.request/respond`, `turn.interrupt`,
  `model.get/set`, `config.get/set`, `skills.list`, `cron.list/add/remove`, `health`.
- Hosts: per-session agents (gateway's `AgentConversations` generalized), cron tick loop,
  curator loop (P4 hook point), graph TTL purge loop.
- **Single config loader** (kills Hermes's three): `$REGENT_HOME/config.yaml` (all behavior;
  serde+zod-style validated schema, `_config_version` + additive reconcile like store v2) +
  `.env` (secrets ONLY — enforced by lint in `regent doctor`).

**P1.2 `regent` CLI (Go — cobra + bubbletea)** at `apps/cli/` (canonical tree applies literally):
- `regent` / `regent chat` — interactive chat: streaming output, tool activity lines, inline
  y/N approval + clarify prompts, `/` commands from the **shared command registry** (exported by
  the daemon so CLI/gateway/TUI can never drift), skill slash commands, Ctrl-C interrupt.
- Subcommands (Hermes parity set): `sessions` (list/resume/search/export/prune) · `model`
  (list/set, provider catalog) · `tools` (list/enable/disable per surface) · `skills`
  (list/view/create/install/opt-out) · `memory` (show/pending/approve/forget) · `cron`
  (list/add/edit/pause/resume/run/remove) · `gateway` (setup/start/stop/status) · `config`
  (get/set) · `profile` (create/list/delete — `-p` sets REGENT_HOME) · `doctor` (toolchain, db
  integrity, provider reachability, config lint) · `logs` · `completion` · `version`/`update`.
- Long tail (`secrets`, `checkpoints`, `bundles`, `optimize`, `repair`, `bitwarden`, `browse`,
  `computer-use`…) arrives with its owning phase — commands ship with features, never stubs.

**P1.3 retire `regent-repl`** once `regent chat` reaches parity (REPL was always the scaffold).

**Exit criteria:** `regent chat` round-trip with streaming + tool activity + approval prompt over
JSON-RPC; `regent -p work chat` isolates state under a profile; same session resumable from CLI
and gateway; `regent doctor` green on a fresh machine; config.yaml is the only behavior source.

## P2 — Loop & provider completion

Streaming end-to-end (provider SSE → daemon events → CLI render) · `anthropic_messages` mode +
prompt-cache breakpoints (port the caching study) · provider profiles catalog (OpenRouter, Groq,
DeepSeek, Ollama, Anthropic, … named like or-conduit's factories) + `model` metadata (context
lengths → real compression thresholds, pricing → cost in turns ledger) · auxiliary per-task
models (summarizer/title-gen first) · SOUL.md + project context files (.regent.md/AGENTS.md…)
with injection scanning into the stable tier · **Orchustr upstream window #1:** native tool-calls
in or-conduit; on landing, adopt + wrap or-sentinel `LoopTopology` (ADR-002 close-out).
*Exit:* Claude via native mode with cache hits visible; `/model` switch mid-conversation
(cache-aware: new session); compression triggers from real context lengths.

## P3 — Tool parity (the core waist)

`patch` (anchored search/replace) · `todo` · `clarify` (RPC plumbing from P1) · `process`
(background + completion watchers → new daemon turn — the Hermes notify pattern) · `web_search` /
`web_extract` (provider-gated check_fn-style) · `execute_code` sandbox with **tools-via-RPC**
(scripts call Regent tools — the zero-context-cost collapse) · browser toolset (CDP via
chromiumoxide; snapshot/click/type/navigate first) · `vision_analyze` + `image_generate`
(provider-gated) · `tool_search` (only once tool count demands it) · guard completion: full
threat/path/url safety, **allowlist persistence** ("approve permanently" → config.yaml), smart
approval via auxiliary model · per-tool result budgets.
*Exit:* the Hermes core-tool table (study #03) fully green at equal-or-better semantics.

## P4 — Memory & learning completion

Write-approval staging (`memory.write_approval` + `/memory pending|approve|reject`) · curator
daemon loop + tar backups + `pin/unpin/restore` verbs · TTL purge + episode-on-session-end in
daemon · session_search scroll/browse calling shapes · skills: `platforms:` gating, config keys,
**hub install/sync** (agentskills.io), optional-skills set, bundles · evals: golden set ≥50
pairs + trajectory-shaped review evals; re-check the embeddings gate (sqlite-vec only if recall
drops on paraphrase classes).
*Exit:* unattended week-long run curates itself: stale skills archived, memory gated, nothing lost.

## P5 — Gateway & delivery breadth

Webhook + REST adapters on the daemon's HTTP listener (deferred from M5 by design, ADR-009) ·
message queueing + interrupt-redirect (replace the "busy" reply with Hermes semantics) ·
`send_message` tool + delivery targets + home channel + cron delivery to platforms · adapters:
Discord, Slack, WhatsApp, Signal (contract-per-platform; group/thread-aware session keys) ·
voice-note transcription (provider-gated).
*Exit:* cron job posts its morning report to Telegram while a Discord chat runs concurrently.

## P6 — Multi-agent & long horizon

Orchestrator-role delegation (spawn depth 2, child cancel propagation) — **Orchustr upstream
window #2:** concurrency cap + error isolation in or-colony, then DelegateTool → Colony adapter
(ADR-008 close-out) · kanban: SQLite board + dispatcher in daemon + worker toolset + board/tenant
isolation + failure auto-block · cron job fields (skills attach, `script` pre-run, `context_from`
chaining, `workdir`) + `cronjob` agent tool + 5-field cron expressions · goals / deliverable mode ·
batch trajectory runner (ShareGPT export) for the research edge.
*Exit:* dispatcher drives two worker profiles through a shared board under all caps.

## P7 — Ops, security, observability

Structured log files (agent/errors/gateway) + `regent logs --follow` · or-lens/or-prism trace
dashboard wiring · secrets redaction at the logging boundary · checkpoints (file-state snapshot/
rollback) · CI: fmt+clippy+test+`cargo audit`+`cargo deny`+`govulncheck`, toolchain matrix ·
setup wizard (`regent setup` interactive; portal-style key collection) + update channel ·
`secrets`/bitwarden integration.
*Exit:* a failed run is replayable from ledger+logs+traces alone; CI gates supply chain.

## P8 — Ecosystem & rich surfaces

`regent mcp serve` — expose Regent's catalog as an MCP server (or-mcp `NexusServer`; Regent
becomes a tool provider, not just consumer) · MCP catalog + OAuth · **TypeScript returns**
(amendment item 4): web dashboard then desktop app as JSON-RPC clients; optional Ink TUI · ACP
server for IDEs · skins/personality + i18n string layer · TTS/voice mode · computer-use · LSP
tool · long-tail toolsets (x_search, spotify, home-assistant…) shipped as **MCP servers**, never
core tools.

---

## Sequencing & sizing

| Phase | Size | Hard dependency |
|---|---|---|
| P1 CLI plane | L | — (start now) |
| P2 loop/providers | M | P1 events for streaming |
| P3 tool parity | L | P1 clarify/approval RPC |
| P4 memory/learning | M | P1 daemon loops |
| P5 gateway breadth | M | P1 HTTP listener |
| P6 multi-agent | M | P1 daemon; or-colony upstream optional |
| P7 ops/security | M | continuous; CI immediately |
| P8 ecosystem | L | P1 JSON-RPC protocol frozen |

P2–P4 can interleave after P1; P7's CI lands alongside P1. Each phase decomposes into atomic,
tested changes per the operating loop; ADRs accompany anything constraining.

**Next concrete step: P1.1 — `regent-daemon` crate skeleton (JSON-RPC server + session manager +
the event protocol), then the Go workspace at `apps/cli/`.**
