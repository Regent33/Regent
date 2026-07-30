# ADR-044: Interactive terminals live in the deacon

**Date:** 2026-07-30 · **Status:** accepted (phases 1–2 shipped)

## Context

The workspace panel needed VS Code's bottom panel — Terminal, Output, Debug
Console — with a terminal that runs code for real, on Windows, macOS and Linux.

The agent's `terminal` tool cannot become that. It is request/response with a
timeout and a 24k output cap: right for "run this and tell me what happened", and
structurally unable to host a REPL, a progress bar, Ctrl+C, or any program that
asks a question. That needs a pseudo-terminal.

The app is deliberately least-privilege. `src-tauri/capabilities/default.json`
states it: *no shell, fs, or http plugin — every process/disk action goes through
the Rust bridge*. So a `tauri-plugin-shell` terminal was never a candidate.

## Decision

The PTY lives in the **deacon**, behind four additive stdio JSON-RPC methods
(`pty.open`/`write`/`resize`/`close`) plus `pty.data`/`pty.exit` notifications.
`portable-pty` provides it (ConPTY on Windows, `forkpty` on Unix); `xterm.js`
renders it in the webview.

The alternative — a PTY in the Tauri bridge — was rejected because it would make
the app a second place that spawns processes and duplicates the workspace-root
logic. That is the drift that already bit `regent-gateway`, where tools and prompt
were hand-copied from the deacon and silently diverged.

Traffic is base64 in **both** directions: output is arbitrary bytes and a UTF-8
character split across a read boundary cannot live in a JSON string, while input
carries control bytes (Ctrl+C is 0x03).

The terminal **starts** at the session's workspace root and is **not confined**
to it. The path jail exists to contain prompt injection; a human typing is the
user, not injected input, and a shell that cannot leave its folder is not a
terminal.

## Consequences

- **An unconfined shell is reachable from the webview.** `detect_dangerous_command`
  does not apply and cannot — a PTY is a byte stream, not discrete commands.
- Containment is **structural, not a check**: `pty.*` is on the stdio dispatcher;
  the HTTP ingress serves only `/health` and `/v1/chat`; the gateway is a separate
  binary that does not route the dispatcher. Unreachable remotely even with
  `http.enabled: true`.
- **No `pty` tool is registered**, so the agent cannot reach any of it. Agent
  tools stay jailed and unchanged.
- **Standing constraint:** if the dispatcher is ever exposed over a socket or a
  second transport, `pty.*` becomes a remote shell and must be gated at that
  boundary.
- A client MUST answer the cursor-position query (`\x1b[6n`) with
  `\x1b[row;colR`. PowerShell blocks on it before printing anything. xterm.js does
  this automatically; anything else driving `pty.*` has to.
- Output batching must be **timer-driven, never read-driven**. Flushing only when
  the next read arrives deadlocks against any shell that prints and then waits —
  which is every shell, starting with that cursor query.
