# ADR-012: Go CLI plane — cobra (+ bubbletea later) at regent-cli/; streaming render; shared command registry

**Status:** Accepted — **Amended 2026-06-13**

**Amendments (2026-06-13, P1.2 implementation):**
1. **Location:** the Go module lives at **`regent-cli/`** (repo root), not `apps/cli/` — user
   directive. The internal clean-arch tree (`app/`, `features/`, `shared/`) is unchanged; the
   Go module path stays `regent/cli` (import prefix is independent of the folder name).
2. **bubbletea — adopted (2026-06-13, after P2.2 streaming).** The plain loop was the P1.2
   placeholder; once real `message.delta` streaming landed (P2.2) the chat surface was rewritten
   on bubbletea (Model/Update/View) with `bubbles` (textinput, viewport, spinner): a scrollable
   transcript, a persistent input box, live-typed replies, a thinking spinner, inline y/N
   approval, and Ctrl-C → `turn.interrupt`. Daemon notifications/responses arrive as `tea.Msg`s
   via a re-issued `listen` command over `rpc.Client.Notifications`. Deps: `bubbletea` v1.3.10,
   `bubbles` v1.0.0 (`lipgloss` indirect). The chat feature is split `chat.go` (model + Update)
   / `view.go` (View + commands) to respect the file-size guideline.
3. **Visual identity (user-mandated):** gradient-silver "REGENT" pixel wordmark; a dotted
   (braille) kneeling-king mark rasterised from vector strokes (teal crown, uniform bright-silver
   body); silver rounded panel outline with the title set into the top border. Teal is the accent;
   silver/white is the main tone. Agent persona is kind, thoughtful, warm, with light emoji use.

**Context:** P1 needs a real CLI (`regent` command — Hermes parity, Regent's own architecture)
that connects to `regent-daemon` over JSON-RPC. The CLI must render streaming turn events, inline
approval/clarify prompts, and Ctrl-C interrupt while staying completely outside the Rust core.
TypeScript is formally deferred to P8 (dashboard, desktop, ACP — M1 amendment item 4). Go gives a
fast compile, a single static binary, and a clean process boundary from the Rust daemon.

**Decision:**
1. **Go module at `apps/cli/`** with the canonical clean-arch tree applied literally (per
   architecture contract: "TS surfaces apply it literally" — same for Go). Layout:
   `app/` (cobra root, DI wiring, composition root), `features/[subcommand]/` (one package
   per subcommand group), `shared/` (JSON-RPC client, config reader, render primitives).
   Cobra for subcommand routing; bubbletea for interactive chat render.
2. **Streaming render contract**: CLI subscribes to daemon notifications; `tool.start` emits an
   activity line; `message.complete` flushes the rendered turn; `approval.request` suspends the
   stream and presents an inline y/N modal; Ctrl-C sends `turn.interrupt` over JSON-RPC (never
   kills the daemon process).
3. **Shared command registry**: `/help /new /stop /approve /deny` and skill slash commands are
   resolved from the registry exported by the daemon via `commands.list` — CLI, gateway, and
   future TUI share one source of truth and cannot drift (enforces the Hermes invariant).
4. **Profile isolation**: `regent -p <name>` sets REGENT_HOME; the CLI either attaches to a
   running daemon socket for that profile or spawns a fresh child-process instance over stdio.
5. **Long tail rule**: subcommands (`secrets`, `browse`, `computer-use`, `bundles`, …) ship with
   their owning feature phase — never as empty stubs in P1.

**Consequences:** P1.2 ships the subcommands listed in next-steps.md §P1.2 (sessions, model,
tools, skills, memory, cron, gateway, config, profile, doctor, logs, completion, version/update).
`regent doctor` is the first E2E smoke test across the daemon, store, and provider. Ink TUI
(TypeScript) and the web dashboard are P8 — they attach to the same daemon socket/stdio contract.
