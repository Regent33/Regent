# ADR-015: Gateway control from the CLI — process lifecycle + auth file, not IPC

**Status:** Accepted — 2026-06-18

**Context:** `regent-gateway` is a separate long-running binary (its own composition root) with no
JSON-RPC/IPC surface back to the daemon or CLI. It reads credentials from the environment, persists
pairing state to `$REGENT_HOME/gateway-auth.json`, and runs a platform poll loop. The CLI needs
`gateway` and `auth` commands (Hermes parity) without inventing a new control protocol.

**Decision:**
1. **Process lifecycle, CLI-managed.** `gateway start` spawns the located `regent-gateway` binary
   detached (stdio → `$REGENT_HOME/logs/gateway.log`), recording its PID in
   `$REGENT_HOME/gateway.pid`; `gateway stop` signals that PID; `gateway status` checks liveness
   (cleaning a stale pid file). Mirrors how `mcp serve` locates/spawns `regent-mcp`. No daemon
   round-trip.
2. **Config via `.env`.** `gateway setup` writes `REGENT_TELEGRAM_TOKEN` / `REGENT_TELEGRAM_ALLOW_ALL`
   / `REGENT_TELEGRAM_ALLOWED_USERS` into `$REGENT_HOME/.env` (atomic, 0600), merged into the child
   env on start (the gateway reads env, not config.yaml).
3. **Auth via the snapshot file.** `auth status` reads `gateway-auth.json` (allow_all, operators,
   paired); `auth revoke <user>` prunes the allowlist/paired sets there. Effective on gateway restart
   (the running gateway re-persists its snapshot every 60s — concurrent edits are last-write-wins).

**Consequences:** `gateway`/`auth` are pure CLI (filesystem + process) — no new daemon methods,
additive. Interactive pairing (codes issued over chat) and message delivery (`send` + adapter config)
require a running gateway and are deferred to a later B2 increment. If the gateway later grows a
control socket, `status`/`auth` can move onto it without changing the CLI surface.
