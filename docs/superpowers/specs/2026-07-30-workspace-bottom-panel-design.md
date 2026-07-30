# Workspace bottom panel: Terminal, Output, Debug Console

**Date:** 2026-07-30
**Status:** shipped — all four phases (2026-07-31)
**Requested:** "For workspace kindly add a terminal, Output, Debug Console, too like VS code" ·
"Ensure that we can use terminal too like a normal terminal and running code" ·
"Make sure I can use them normally like in the VSCode" · cross-platform (Windows, macOS, Linux)

## Why this shape

Four findings from the existing codebase constrained the design before any option was on the table.

**The app is deliberately least-privilege.** `src-tauri/capabilities/default.json` says it outright:
*"No shell, fs, or http plugin — every process/disk action goes through the Rust bridge."* Even the
folder picker only returns a path **string**. A `tauri-plugin-shell` terminal would break that
posture, so it was never a candidate.

**Nothing to reuse for a real terminal.** No PTY crate anywhere, no `xterm` in `package.json`. Both
have to be added; neither should be hand-rolled (ANSI parsing with scrollback and selection, or
ConPTY FFI, are the wrong things to write yourself).

**But real execution infrastructure exists.** A `TerminalBackend` trait with local, docker and ssh
implementations; per-OS shell handling already solved (`cmd /C` via `raw_arg` on Windows, because
Rust's default `\"` escaping corrupts quoted commands; `sh -c` elsewhere); `detect_dangerous_command`
and an approval gate. It is request/response, 60s timeout, 24k-char cap — **not** a PTY, and not
convertible into one.

**A streaming channel already runs.** The deacon pumps JSON-RPC notifications through the Tauri
bridge to a process-lifetime listener in `deaconBus.ts`. `tool.start` already flows.

## Architecture

```
webview ── xterm.js ──┐
                      │  pty.write / pty.resize        (base64)
                      │  ◄── pty.data / pty.exit       (notifications)
                      ▼
        Tauri bridge — forwards only, owns no process
                      │  JSON-RPC over stdio  ← the ONLY path in
                      ▼
        deacon ── PtyRegistry ── portable-pty ── shell
                      │
                      └─ cwd seeded from the session's workspace_root
```

**Decision: the PTY lives in the deacon, not the Tauri bridge.** The deacon already owns process
execution, the workspace root, and the session registry, so a terminal there follows `workspace.set`
for free and the CLI gets the same capability. Putting it in the bridge would make the app a second
place that spawns processes — the exact drift that has already bitten `regent-gateway`, where tools
and prompt were hand-copied from the deacon and silently diverged.

**New dependencies:** `portable-pty` (MIT, from wezterm — ConPTY on Windows, `forkpty` on Unix),
`@xterm/xterm`, `@xterm/addon-fit`. `base64` is already a workspace dependency.

## Deacon side

**RPC surface — additive only, four methods.** `pty.open {session_id?, cols, rows}` → `{pty_id}` ·
`pty.write {pty_id, data}` · `pty.resize {pty_id, cols, rows}` · `pty.close {pty_id}`. Two
notifications: `pty.data {pty_id, data}`, `pty.exit {pty_id, code}`.

**Base64 in both directions.** PTY output is arbitrary bytes; a UTF-8 character split across a read
boundary would corrupt if placed in a JSON string. ~33% overhead buys exactness, which for
keystrokes and escape sequences is not optional.

**Shell resolution is native-first**, a pure function with per-platform tests:

| platform | order |
|---|---|
| Windows | `REGENT_PTY_SHELL` → `pwsh` → `powershell.exe` → `%COMSPEC%` |
| macOS | `REGENT_PTY_SHELL` → `$SHELL` → `zsh` → `sh` |
| Linux | `REGENT_PTY_SHELL` → `$SHELL` → `bash` → `sh` |

PowerShell on Windows is the owner's explicit call (2026-07-30), matching VS Code's default. Note
this differs from the agent's `LocalBackend`, which uses `cmd /C` for a specific escaping reason —
that is a non-interactive command runner and stays as it is.

**Backpressure.** The reader task batches on a ~16ms tick instead of notifying per read, so `yes`
cannot flood the bridge. That ceiling is named in a comment rather than left implicit.

## App side

`BottomPanel` inside the workspace column: tab bar (Terminal · Output · Debug Console), vertical
drag via the existing `useDragSize`, show/hide. Output carries a **channel dropdown** — "Agent
tools" / "Deacon log" — mirroring VS Code's own Output channel picker, since both sources were
requested. No tab component exists yet; a minimal one is built rather than adding a UI library.

VS Code parity means, concretely: interactive input, Ctrl+C, ANSI colour, resize that reflows,
scrollback, and standard copy/paste.

## Security consequences

**This adds an unconfined shell reachable from the webview.** Stated plainly because it is a real
privilege expansion.

- The terminal starts at the workspace root but is **not confined** to it — `cd ..` works. The jail
  exists to contain prompt injection; a human typing is the user, not injected input. A shell that
  cannot leave its folder is not a terminal. (Owner decision, 2026-07-30.)
- `detect_dangerous_command` **does not apply**, and cannot: a PTY is a byte stream, not discrete
  commands.
- Containment is **structural, not a check**: `pty.*` lives on the stdio dispatcher. The HTTP ingress
  exposes only `/health` and `/v1/chat`, and the gateway is a separate binary that does not route the
  dispatcher. So `pty.*` is unreachable remotely even with `http.enabled: true`.
- **The agent gets no `pty` tool.** This is a UI surface, not a model capability. Agent tools stay
  jailed and unchanged.
- **Standing constraint:** if the dispatcher is ever exposed over a socket or a second transport,
  `pty.*` becomes a remote shell and must be gated explicitly at that boundary.

## Testing

Pure functions get unit tests: shell resolution per platform, the tab model, base64 round-trip. The
deacon integration test spawns a **real** PTY, writes `echo hi`, and asserts the bytes come back —
that single test fails if any part of the wiring breaks.

## Phases

| phase | scope |
|---|---|
| 1 | Panel shell — tabs, vertical resize, show/hide |
| 2 | Terminal — PTY in the deacon + xterm.js |
| 3 | Output — two channels (agent tool activity, deacon log tail) |
| 4 | Debug Console — JSON-RPC traffic with a filter |

### What phases 3–4 changed against this design

The design said "Output carries a channel dropdown"; it does, and the log channel needed one
addition the design did not anticipate: a `logs.tail {limit?}` RPC, read-only and fixed to
`$REGENT_HOME/logs/`, since the app has no filesystem plugin and every disk action goes through the
deacon. That constraint is the same one that put the PTY there.

The panel also had to stop rendering only the active tab. It unmounted `TerminalTab` on every tab
switch, which closed the ptys and killed whatever was running — `TerminalInstance` is careful never
to unmount across a switch, and the panel above it undid that. All tabs stay mounted now.

Each phase is its own commit, independently testable and installable.

## Files

**Phase 1:** `features/workspace/presentation/BottomPanel.tsx` (new) ·
`features/workspace/domain/panelModel.ts` + `.test.ts` (new) · `WorkspacePanel.tsx` (mod) ·
`shared/i18n/en/workspace.ts` (mod)

**Phase 2:** `regent-deacon/Cargo.toml` (mod) · `application/pty/{mod,shell,pump}.rs` (new) ·
`dispatcher/pty_ops.rs` (new) · `dispatcher/mod.rs` (mod) · `tests/deacon_basics/pty.rs` (new) +
`main.rs` (mod) · Desktop `package.json` (mod) · `viewmodels/usePty.ts` (new) ·
`presentation/TerminalTab.tsx` (new) · `shared/state/deaconBus.ts` (mod)

**Both:** ADR-044 · `docs/changelogs/CHANGELOG.md`

## Out of scope

Multiple concurrent terminals, split terminals, shell-integration decorations, task runners, and a
real debugger to attach the Debug Console to. Phase 4's console shows RPC traffic because there is no
debugger; if one ever exists, that is a separate design.
