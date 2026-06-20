# Regent — Full CLI Command Parity Plan (backend + front-end)

**Status: PLAN (2026-06-18).** Source of truth for bringing Regent's CLI to full
[Hermes CLI](../../1-1%20Hermes%20Agent) command parity by building **real logic** — not stubs.

## Goal

Implement the complete Hermes CLI command surface in Regent. Hermes ships ~40 top-level command
groups; Regent's CLI (`regent-tui`) currently implements 13. The rest require backend that does not
exist yet. This plan covers building that backend (Rust daemon JSON-RPC methods) **and** wiring the
front-end (`regent-tui`) commands, feature by feature.

## Architecture constraint (why "front-end only" can't get us there)

`regent-tui` is a **thin JSON-RPC client** to `regent-daemon` (ADR-011, ADR-014). The real logic lives
in the Rust core. A command like `kanban` or `goals` has nothing to call until the daemon exposes a
method for it. So "create the logic" = **add the daemon method (in the owning crate) first**, then the
CLI command. A handful of commands are pure local/host operations (profile dirs, config file, update)
and are implemented in the CLI against the filesystem with no daemon round-trip.

## Principles (binding)

1. **No stubs.** A command ships only when its backend method exists and works (next-steps.md rule).
2. **Logic in the owning crate**; the daemon `dispatcher` exposes it; the CLI calls it. Three layers,
   same as today.
3. **Additive contract only.** New JSON-RPC methods are added; existing methods never change shape.
4. **Per-feature gate:** crate capability (+ unit tests) → daemon method (dispatcher arm + handler) →
   CLI command → `cargo test --workspace` + `clippy -D warnings` + `bun test` + `tsc` + `biome` +
   live smoke, all green. The existing **87 daemon tests** stay green throughout.
5. **Phase order:** extend existing crates first (low risk), then the large new subsystems, following
   the next-steps.md roadmap (P4–P8). Edges before core.

## Current state (baseline)

- **Daemon callable methods (21):** `health`, `version`, `commands.list`, `config.get`,
  `session.create/resume/list/search`, `prompt.submit`, `model.get/list/set`, `skills.list`,
  `cron.add/list/remove`, `memory.pending/approve/reject`, `approval.respond`, `turn.interrupt`.
  (Plus server→client events: `turn.*`, `message.*`, `tool.*`, `approval.request`.)
- **CLI commands (13):** `chat`, `model`, `skills`, `config`, `sessions list/search`, `cron list/add/
  remove`, `memory pending/approve/reject`, `logs`, `doctor`, `mcp serve`, `setup`, `version`, `help`.
- **Gap vs current daemon:** only `session.resume` is unwired in the CLI.

## Command inventory → backend mapping

Legend — **Status:** ✅ done · 🟡 backable now (method/fs exists) · 🔵🟣🟠🟤🔶⚫⚪ new backend, by
batch (B1–B6). **Crate:** where the logic lives.

| Hermes command group | Status | Owning crate | New daemon method(s) | Phase |
|---|---|---|---|---|
| chat / model / skills / config(get) / sessions(list,search) / cron(list,add,remove) / memory(pending,approve,reject) / logs / doctor / mcp serve / setup / version | ✅ | — | — (exist) | done |
| `sessions resume` | 🟡 | regent-daemon | uses existing `session.resume` | B0 |
| `profile list/create/delete` | 🟡 | CLI (filesystem) | none (`~/.regent-profiles/`) | B0 |
| `config set` | 🟡 | CLI (filesystem) | none (writes `config.yaml`) | B0 |
| `status` | 🟡 | regent-daemon | `status.get` (aggregate health/cron/gateway) | B0 |
| `cron pause/resume/run/edit` | 🔵 | regent-cron | `cron.set_enabled`, `cron.run`, `cron.edit` | B1 |
| `memory pin/unpin/restore/forget` | 🔵 | regent-graph | `memory.pin`, `memory.unpin`, `memory.restore`, `memory.forget` | B1 |
| `skills view/create/install/opt-out` | 🔵 | regent-skills | `skills.view`, `skills.create`, `skills.install`, `skills.opt_out` | B1 |
| `tools list/enable/disable` | 🔵 | regent-tools + config | `tools.list`, `tools.set_enabled` | B1 |
| `auth/login/logout/pairing/revoke` | 🟣 | regent-gateway | `auth.status`, `pairing.start/complete`, `auth.revoke` | B2 (P5) |
| `gateway setup/start/stop/status` | 🟣 | regent-gateway | `gateway.status`; start/stop via CLI spawning `regent-gateway` | B2 (P5) |
| `webhook / slack / whatsapp / send` | 🟣 | regent-gateway | `delivery.list`, `message.send`, adapter config methods | B2 (P5) |
| `kanban` (add/assign/block/comment/complete/decompose/list/show/…) | 🟠 | **new** regent-kanban (or regent-store board) | `kanban.*` (board CRUD + dispatch) | B3 (P6) |
| `goals` | 🟠 | regent-agent | `goals.*` (deliverable mode) | B3 (P6) |
| `checkpoints` | 🟤 | **new** regent-checkpoints | `checkpoint.snapshot/list/restore` | B4 (P7) |
| `security / insights / debug / dump / hooks` | 🟤 | regent-daemon + store | `security.audit`, `insights.get`, `debug.dump`, `hooks.*` | B4 (P7) |
| `bundles / backup / curator(trigger)` | 🔶 | regent-skills + store | `bundles.*`, `backup.create/restore`, `curator.run` | B5 (P4) |
| `dashboard / gui / acp / mcp catalog,install / plugins / import / portal / claw` | ⚫ | regent-gateway / or-mcp | HTTP/MCP surfaces | B6 (P8) |
| `update / uninstall / postinstall` | ⚪ | CLI (host) | none — host/install ops; CLI-local or out of scope | B6 |

## Sequencing (batches)

- **B0 — Quick real wins (no/low backend):** `sessions resume`, `profile`, `config set`, `status`.
  Mostly CLI + one small daemon method. Lands fast, all real.
- **B1 — Extend existing crates:** cron lifecycle, memory pin/unpin/restore/forget, skills
  view/create/install/opt-out, tools list/enable/disable. Each is a contained method on a crate that
  already exists.
- **B2 — P5 gateway/delivery:** auth/pairing, gateway control, message delivery + platform adapters.
  Reuses the existing `regent-gateway` (18 webhook adapters already built); exposes its state/control
  through the daemon.
- **B3 — P6 multi-agent:** kanban board (largest single item — new crate + dispatcher + worker
  toolset) and goals/deliverable mode.
- **B4 — P7 ops/observability:** checkpoints (file-state snapshot/rollback), security audit, insights,
  debug/dump, hooks.
- **B5 — P4 memory/learning:** bundles, tar backups, manual curator trigger.
- **B6 — P8 ecosystem:** dashboard/gui/acp/mcp-catalog/plugins, plus host ops (update/uninstall) —
  evaluated case by case; some are CLI-local, some may stay out of scope (e.g. `gui`).

## Per-feature detail (B0–B1, the near-term work)

**B0.1 `sessions resume <id>`** — CLI calls existing `session.resume {session_id}`, then opens the
chat surface bound to that session. No backend change.

**B0.2 `profile list|create|delete [name]`** — CLI over `~/.regent-profiles/`: `list` enumerates
dirs, `create` mkdir (0700), `delete` removes (with confirm). No daemon.

**B0.3 `config set <key> <value>`** — CLI edits `$REGENT_HOME/config.yaml` (dotted key path), atomic
write, `_config_version` preserved. No daemon (mirrors `setup`'s writer). `config get` already exists.

**B0.4 `status`** — new daemon method `status.get` → `{ uptime, model, active_sessions, cron: {jobs,
next_run}, gateway: {running, platforms} }`. CLI prints a compact status block.

**B1.1 cron lifecycle** — `regent-cron`: `pause/resume` = toggle `CronJob.enabled` + recompute
`next_run_at`; `run` = enqueue an immediate tick for one job; `edit` = update schedule/prompt/name.
Daemon: `cron.set_enabled {id, enabled}`, `cron.run {id}`, `cron.edit {id, …}`. CLI: `cron
pause/resume/run/edit`.

**B1.2 memory lifecycle** — `regent-graph`: `pin/unpin` set a node flag that exempts it from TTL
purge; `restore` un-archives; `forget` soft-deletes with provenance. Daemon: `memory.pin/unpin/
restore/forget {id}`. CLI: `memory pin/unpin/restore/forget`.

**B1.3 skills authoring** — `regent-skills`: `view` returns one SKILL.md; `create` writes a new skill
file; `install` pulls from a hub URL/path (provenance-tagged, untrusted); `opt_out` disables one.
Daemon: `skills.view/create/install/opt_out`. CLI: `skills view/create/install/opt-out`.

**B1.4 tools** — `regent-tools` already holds the registry. Add `tools.list` (name, toolset, enabled
per surface) and `tools.set_enabled {tool, surface, enabled}` (persisted in `config.yaml`). CLI:
`tools list/enable/disable`.

## Per-feature detail (B2 — P5 gateway & delivery)

Reuses the existing `regent-gateway` (auth/pairing from M5; 18 webhook adapters already built). The
daemon exposes gateway **state/control**; the gateway process itself is spawned by the CLI like
`mcp serve` spawns `regent-mcp`.

**B2.1 auth / pairing** — daemon: `auth.status` → `{paired, devices:[{id,name,last_seen}]}`;
`pairing.start` → `{code, expires_at}`; `pairing.complete {code}` → `{paired, device}`;
`auth.revoke {device_id}` → `{revoked}`. CLI: `login` (pairing.start → show code → poll), `logout`
(revoke this device), `auth status`, `pairing` (manual code flow), `auth revoke <id>`.

**B2.2 gateway control** — daemon: `gateway.status` → `{running, listen_addr, platforms:[…]}`.
`gateway start/stop` spawn/terminate the `regent-gateway` binary (located like the daemon/mcp).
`gateway setup` writes the gateway block of `config.yaml` (port, allowed platforms). CLI:
`gateway setup/start/stop/status`.

**B2.3 message delivery** — daemon: `delivery.list` → configured targets + home channel;
`message.send {target, text}` → routes through the `send_message` tool/adapter. Platform adapters
(slack, whatsapp, webhook, …) are configured in `config.yaml`. CLI: `send <target> <text>`, and
`webhook/slack/whatsapp` subcommands for adapter config (token, secret, home channel).

## Per-feature detail (B3 — P6 multi-agent)

**B3.1 kanban** — *the largest single item; gets its own ADR (board schema).* New crate
`regent-kanban`: a SQLite board (`tasks`: id, title, body, status ∈ {backlog, ready, in_progress,
blocked, done, archived}, assignee, deps[], comments[], tenant) + a dispatcher in the daemon that
drives worker profiles (P6: spawn depth, board/tenant isolation, failure auto-block). Daemon methods:
`kanban.create {title, body?}`, `kanban.list {status?, tenant?}`, `kanban.show {id}`,
`kanban.assign {id, worker}`, `kanban.block {id, reason}` / `kanban.unblock {id}`,
`kanban.comment {id, text}`, `kanban.complete {id}`, `kanban.decompose {id}` (LLM splits a task into
ordered subtasks), `kanban.archive {id}` / `kanban.list_archived`. CLI: `kanban
create/list/show/assign/block/unblock/comment/complete/decompose/archive`.

**B3.2 goals / deliverable mode** — `regent-agent`: a goal is a long-horizon objective with a
deliverable definition; the agent works it across turns (prospective memory). Daemon: `goals.add
{text, deliverable?}`, `goals.list`, `goals.show {id}`, `goals.complete {id}`. CLI: `goals
add/list/show/complete`.

## Per-feature detail (B4 — P7 ops & observability)

**B4.1 checkpoints** — new `regent-checkpoints`: snapshot file/workspace state and roll back. Daemon:
`checkpoint.snapshot {label?}` → `{id}`, `checkpoint.list`, `checkpoint.restore {id}`. CLI:
`checkpoints create/list/restore`.

**B4.2 security** — daemon: `security.audit` → checks `$REGENT_HOME` perms (0700), `.env` not
world-readable, provider key not in `config.yaml`, tool allowlist sanity. CLI: `security audit`.

**B4.3 insights** — daemon: `insights.get {session?}` → turns-ledger rollup (tokens in/out, cost,
tool-call counts, fallbacks). CLI: `insights`.

**B4.4 debug / dump** — daemon: `debug.dump {session?}` → bundles redacted logs + config + turns
ledger into a tar for a bug report (secrets stripped at the boundary). CLI: `debug`, `dump <session>`.

**B4.5 hooks** — the `DispatchHook` observer seam (ADR-010). Daemon: `hooks.list`, `hooks.add
{event, command}`, `hooks.remove {id}`. CLI: `hooks list/add/remove`.

## Per-feature detail (B5 — P4 memory & learning)

**B5.1 bundles** — `regent-skills`: a bundle is a named set of skills installed together. Daemon:
`bundles.list`, `bundles.install {name}` (provenance-tagged). CLI: `bundles list/install`.

**B5.2 backup** — `regent-store` + `regent-graph`: tar backups of `state.db` + graph + skills. Daemon:
`backup.create` → `{path}`, `backup.list`, `backup.restore {path}` (with confirm). CLI: `backup
create/list/restore`.

**B5.3 curator (manual trigger)** — the curator loop already runs in the daemon; add `curator.run` →
`{archived, reviewed, kept}` to trigger a pass on demand. CLI: `curator run`.

## Per-feature detail (B6 — P8 ecosystem & host ops)

**B6.1 mcp catalog/install** — `or-mcp` NexusServer catalog (`mcp serve` already done). Daemon:
`mcp.catalog`, `mcp.install {name}`, `mcp.list`, `mcp.link {name}` / `mcp.unlink {name}`. CLI:
`mcp catalog/install/list/link/unlink`.

**B6.2 dashboard** — `regent-gateway` HTTP serves a web dashboard (TS, ADR-014 P8). CLI: `dashboard`
opens/serves it. Depends on the JSON-RPC contract being frozen.

**B6.3 acp** — ACP server for IDE integration. CLI: `acp` spawns the ACP bridge over stdio (like
`mcp serve`).

**B6.4 plugins** — plugin registry. Daemon: `plugins.list`, `plugins.install {name}`. CLI: `plugins
list/install`.

**B6.5 host ops** — `update` (self-update via git/release channel), `uninstall`, `postinstall` are
**CLI-local host operations** (no daemon round-trip): they manage the install, not the agent. `gui`
(desktop app) is **out of scope** for the terminal CLI. Each is decided at B6; some may be deferred or
dropped rather than forced onto the daemon model.

## Verification (every feature)

`cargo test --workspace` + `cargo clippy --all-targets -- -D warnings` (Rust); `bun test` + `tsc
--noEmit` + `biome check` (TS); a live smoke of the new command against the built daemon. CHANGELOG
entry + ADR when a feature constrains future work (e.g. the kanban board schema).

## Risks / open questions

- **Build time:** the workspace is large; `cargo test --workspace` is the slow gate. Mitigate by
  testing the touched crate first, full workspace before reporting a batch done.
- **Big subsystems:** kanban, goals, checkpoints are new crates/modules (days each), not quick adds —
  each gets its own ADR and may land across multiple turns.
- **Host ops:** `update`, `uninstall`, `postinstall`, `gui` are installer/desktop concerns that don't
  map cleanly onto the daemon; decide per item whether CLI-local or out of scope.
- **Gateway control:** `gateway start/stop` — the CLI likely spawns/stops the `regent-gateway` binary
  (like `mcp serve` spawns `regent-mcp`) rather than the daemon doing it; confirm at B2.
</content>
