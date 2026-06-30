# ADR-011: regent-deacon — JSON-RPC 2.0 IPC; single config.yaml loader; daemon-hosted loops

**Status:** Accepted

**Context:** P1 needs a long-lived core process to replace the `regent-repl` scaffold and serve
as the IPC hub for all surfaces (Go CLI now; TS dashboard/desktop/ACP in P8). Two interaction
patterns are required: child-process (stdio pipe, owned by Go CLI) and attach mode (named pipe on
Windows, Unix socket on Linux/macOS, for TUI/multi-client). Hermes used three separate config
loaders (.env, config.yaml fragments, runtime globals) — a deliberate wart to design away.

**Decision:**
1. **`regent-deacon` (Rust crate, canonical `app/` shell)** speaks **JSON-RPC 2.0** in both
   modes — same protocol, different transport; no HTTP/REST overhead in the hot path. Methods:
   `session.{create,resume,list,search}`, `prompt.submit`, `model.{get,set}`,
   `config.{get,set}`, `skills.list`, `commands.list`, `cron.{list,add,remove}`, `health`.
   Notifications (streamed progress, no polling): `turn.{started,complete,interrupted}`,
   `tool.{start,complete}`, `message.complete`, `approval.{request,respond}`,
   `clarify.{request,respond}`. The v1 surface is frozen on P1.3 completion; additions are
   additive-only; breaking changes require a `_protocol_version` bump.
2. **Single config loader**: `$REGENT_HOME/config.yaml` — all behavioral configuration,
   serde-validated schema with `_config_version` + additive field-reconcile (same pattern as
   store v2). `.env` carries secrets *only*; `regent doctor` lints any behavioral key found
   in `.env` and errors. Profiles are named REGENT_HOME directories; `-p <name>` selects them.
3. **Daemon-hosted loops**: per-session `Agent`s (gateway's `AgentConversations` generalized),
   cron tick loop, curator loop (P4 hook point), graph TTL purge loop — all behind a shutdown
   signal that drains in-flight turns gracefully before exit.

**Consequences:** `regent-repl` is retired once `regent chat` reaches round-trip parity (P1.3).
Webhook/REST adapters for gateway land in P5 on the daemon's HTTP listener (ADR-009). The
JSON-RPC protocol is the frozen contract for TS surfaces (P8). `regent doctor` is the first
integration smoke test — it verifies toolchain, DB integrity, provider reachability, and
config.yaml lint on a fresh machine before any session is attempted.
