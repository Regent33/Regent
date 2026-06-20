# P1.1 — regent-daemon Design

**Phase:** P1 · **ADR:** [ADR-011](adr/ADR-011-daemon-json-rpc.md) · **Arch:** [ADR-007](adr/ADR-007-clean-architecture-layout-and-learning-loop.md) + [architecture-mapping.md](architecture-mapping.md)

---

## 1. Crate internal layout (`domain / application / infra`)

```
crates/regent-daemon/
├── domain/
│   ├── entities/    session_handle.rs  daemon_config.rs  rpc_types.rs
│   ├── contracts/   transport.rs (ITransport)  session_manager.rs (ISessionManager)
│   └── errors/      daemon_error.rs
├── application/
│   ├── dispatcher.rs       JSON-RPC method router (transport-agnostic)
│   ├── session_manager.rs  per-session Agent lifecycle
│   └── loops/              cron_loop.rs  curator_loop.rs  ttl_loop.rs
├── infra/
│   ├── transport/          stdio.rs  named_pipe.rs (win) / unix_socket.rs (lin/mac)
│   └── config_loader.rs    serde + config.yaml + _config_version reconcile
└── bin/
    └── regent-daemon.rs    composition root — wires all crates, spawns loops
```

`domain/` imports only `regent-kernel`. `application/` imports domain + regent-{store,agent,…} via contracts. `infra/` implements contracts. `bin/` is the only place handles are wired.

---

## 2. JSON-RPC dispatch (transport-agnostic)

Both transports produce `(ConnId, Request)` and consume `(ConnId, Response | Notification)`:

```
StdioTransport | NamedPipeTransport
        │ frame (newline-delimited JSON)
        ▼
  mpsc tx: (ConnId, JsonRpcRequest)
        │
   Dispatcher (tokio::select! over method handlers)
        │
  session_manager / config / health / skills / cron …
        │
  mpsc rx: (ConnId, JsonRpcResponse)  or  notification fan-out
```

`ITransport` trait: `async fn next(&mut self) -> Option<(ConnId, Bytes)>` + `async fn send(&mut self, ConnId, Bytes)`. `Dispatcher` never imports a transport — it owns two `mpsc` channels. The composition root wires transport → channels → dispatcher.

---

## 3. Session manager

```rust
struct SessionEntry {
    handle:  JoinHandle<()>,
    notif_tx: mpsc::Sender<RpcNotification>,
    cancel:  CancellationToken,
}
type Sessions = Arc<Mutex<HashMap<SessionId, SessionEntry>>>;
```

- `session.create` → spin up `Agent` (graph memory, skills, full catalog, `BackgroundReview` fork).
- `session.resume` → reload from `regent-store`, replay transcript through `Agent::resume`.
- `prompt.submit` → post message to session's agent task; notifications streamed back via `notif_tx`.
- `turn.interrupt` → `cancel.cancel()`; guard releases; next `prompt.submit` re-arms.
- **Graceful shutdown**: cancel all tokens → join all handles with a drain timeout (default 10 s) → exit.

---

## 4. Config loader (`$REGENT_HOME/config.yaml`)

```yaml
_config_version: 1          # additive reconcile on version bump
model:
  default: "claude-sonnet-4-6"
  base_url: ~                # null = Anthropic default
context:
  max_tokens: 200_000
  trigger_fraction: 0.85
  protect_last_n: 10
memory:
  home: "~/.regent"
cron:
  tick_interval_secs: 30
```

Loader: `serde_yaml` deserialize → unknown keys → hard error (strict). Missing keys vs current version → fill defaults (additive). `_config_version` < current → reconcile pass. Secrets (`REGENT_API_KEY`, …) live in `.env` only; `regent doctor` lints behavioral keys found in `.env` and reports them as errors.

---

## 5. Crate wiring at the composition root

| Crate | Role in daemon |
|---|---|
| `regent-kernel` | types throughout |
| `regent-store` | `SessionStore` — session CRUD, turn ledger, FTS search |
| `regent-providers` | `FallbackChat` built from `config.yaml` model section |
| `regent-tools` | `core_catalog_with_terminal()` + MCP registrations at boot |
| `regent-graph` | `GraphMemory` — one instance per session |
| `regent-skills` | `SkillLibrary` — shared, single instance (skills dir under REGENT_HOME) |
| `regent-cron` | `Scheduler` — tick loop spawned at daemon boot |
| `regent-agent` | `Agent` + `BackgroundReview` — one per active session |

`regent-gateway` is a **separate binary** that connects to the daemon over the same JSON-RPC protocol; it is not a library dependency of regent-daemon.

---

## 6. regent-repl feature parity checklist (P1.3 gate)

`regent-repl` today (`crates/regent-agent/src/bin/repl.rs`):

- [ ] Env-var provider config (`REGENT_API_KEY` / `REGENT_MODEL` / `REGENT_BASE_URL`)
- [ ] Graph memory snapshot in frozen prompt; `memory` toolset registered
- [ ] Skills library under `~/.regent/skills`; index in stable-tier prompt
- [ ] Skill slash commands (`/name task` → body injected, `record_use`)
- [ ] Background review fork; review awaited on clean exit
- [ ] Cron scheduler loop (30 s tick, `~/.regent/cron/jobs.json`)
- [ ] `delegate_task` registered in catalog
- [ ] Stdin approval prompt (→ replaced by `approval.request` notification + CLI modal)
- [ ] `RUST_LOG` tracing subscriber (→ replaced by structured log file + `regent logs`)

All items must be reachable via `regent chat` over JSON-RPC before `regent-repl` is retired.
