# Sandboxing & Tool-Execution Security

Regent's agent has terminal and filesystem access, and — since the messaging gateway landed —
**untrusted external input** (a Slack message, an SMS, a Discord slash command) can drive an agent
turn that invokes tools. This document describes the layered sandbox that contains that execution,
how it maps onto Regent's architecture, and how it compares to the two systems it draws from:
**Claude Code** (`@anthropic-ai/sandbox-runtime` — bubblewrap/Seatbelt) and the **Hermes Agent**
(Docker/Modal/Daytona terminal backends).

It is a *re-implementation in Regent's style*, not a port: the ideas are replicated as small,
unit-tested Rust seams in the clean-architecture layers, not as a dependency on either system's
runtime.

## Threat model

The primary threat is **prompt injection → exfiltration / damage through the shell**: a malicious
message (or a compromised web page a tool fetched) convinces the model to run a command that deletes
files, reads secrets, or phones home. The secondary threat is **honest-but-overreaching** tool calls
that wander outside the workspace.

The trust boundary is the **tool-execution layer** (`regent-tools`). Everything below the model is
enforced in code — never by prompt text — because the model itself is the untrusted component once
it is processing third-party input.

We defend in depth so that a bypass of any single layer is not a full compromise:

| # | Layer | What it stops | Where it lives |
|---|-------|---------------|----------------|
| 1 | **Approval gate** | Destructive commands run without a human's say-so | `regent-tools` `domain/guard.rs` + `ApprovalHandler` |
| 2 | **Filesystem jail** | File tools / `cwd` escaping the workspace (`..`, symlinks, absolute paths) | `domain/entities.rs` `ToolContext` |
| 3 | **Isolated command backend** | A shell command touching the host (network, FS, processes) | `infra/backends.rs` + `infra/sandbox.rs` |
| 4 | **Secret-env stripping** | Credentials leaking into a spawned command's environment | `infra/sandbox.rs` + `infra/backends.rs` |
| 5 | **Network egress off** | A command in the sandbox reaching the internet | `sandbox:<image>` (`--network none`) |

## The layers

### 1. Approval gate (always on)

`detect_dangerous_command` matches a `RegexSet` of destructive patterns (`rm -rf`, `mkfs`, `dd
of=/dev/…`, `curl … | sh`, `DROP TABLE`, `git push --force`, fork bombs, shutdown, …). A match does
**not** block — it routes the command through the injected `ApprovalHandler`, which is **deny-by-
default** on non-response. This mirrors Hermes's `tools/approval.py` and is the one layer that is
unconditional (it runs even with no sandbox configured).

### 2. Filesystem jail (in-process file tools)

The file tools (`read_file`, `write_file`, `search_files`) operate through the Rust process's own
`std::fs` — they do **not** go through a terminal backend, so a container alone cannot contain them.
`ToolContext` therefore carries an optional **sandbox root**, and `ToolContext::resolve` enforces it:

- `..` traversal is rejected outright.
- The longest existing path prefix is **canonicalized** (symlink-safe, like Hermes's
  `os.path.realpath()`), and the not-yet-created tail is re-appended, so writes to new files are
  still contained.
- Absolute paths outside the root are rejected.

`resolve` returns `Result`; on violation the tool returns an error JSON rather than touching disk.
Because the jail root is the **workspace** and `$REGENT_HOME` (the `.env` and `config.yaml`) lives
outside it, a sandboxed turn cannot read or rewrite Regent's own secrets or config — the same
"deny writes to settings" property Claude Code enforces explicitly.

### 3. Isolated command backend

`REGENT_TERMINAL_BACKEND` selects where shell commands run (parsed in `infra/backends.rs`):

| Value | Isolation |
|-------|-----------|
| `local` (default) | None — host shell. |
| `docker:<container>[:workdir]` | `docker exec` into a standing container. |
| `sandbox:<image>` | A fresh `docker run` **per command** (see below). |
| `ssh:<user@host>` | A remote host (key-based, `BatchMode=yes`). |

The `sandbox:<image>` backend (`infra/sandbox.rs`) is the hardened target, matching Hermes's
"container hardening" row and Claude Code's filesystem/network restrictions:

```
docker run --rm --network none --read-only --cap-drop ALL \
  --security-opt no-new-privileges --memory 512m --pids-limit 256 \
  --tmpfs /tmp:rw,exec,nosuid -v <workspace>:/work:rw -w /work <image> sh -c '<command>'
```

No network, read-only root filesystem, all Linux capabilities dropped, no privilege escalation,
memory and PID caps, and only the workspace (`/work`) plus a tmpfs `/tmp` writable.

### 4. Secret-env stripping

Every spawned command (all backends, `infra/backends.rs::run_argv`) has credential-shaped
environment variables removed before exec — `is_secret_env_var` strips names containing `SECRET`,
`TOKEN`, `PASSWORD`, `CREDENTIAL`, `API_KEY`/`APIKEY`, `ACCESS_KEY`, `PRIVATE_KEY`, or ending in
`_KEY`. This is Hermes's "API keys stripped from the child env": Regent loads provider keys and
platform tokens into its own process environment, and without this they would be inherited by every
shell command the agent runs.

### 5. Enforce mode (fail loud)

`REGENT_SANDBOX=1` turns layers 2–5 on together and makes them mandatory:

- the session `ToolContext` is built with `new_sandboxed` (the jail);
- `terminal_backend_from_env` **refuses the `local` backend** — it returns a hard config error and
  the daemon does not start unless the backend is `sandbox:`/`docker:`/`ssh:`.

It never silently degrades to unsandboxed execution. This is Claude Code's `failIfUnavailable` /
`getSandboxUnavailableReason` lesson (their issue #34044): a security setting that quietly does
nothing is worse than none.

## Architecture mapping

Everything is a constructor-injected seam in `regent-tools`, so it is unit-testable without spawning
anything:

- `domain/guard.rs` — pure dangerous-command detection.
- `domain/entities.rs` — `ToolContext` + the `resolve` jail (`contained` is pure; tested with
  allow/`..`/absolute-escape cases).
- `domain/contracts.rs` — `TerminalBackend`, `ApprovalHandler`.
- `infra/backends.rs` — backend parsing, the shared `run_argv` (timeout + kill-on-drop + env
  scrub), and `terminal_backend_from_env` enforcement.
- `infra/sandbox.rs` — `SandboxBackend`, `build_sandbox_args` (pure, argv asserted), `sandbox_enabled`,
  `is_secret_env_var`, `enforce_backend`.
- `application/registry.rs` — `core_catalog_from_env()` wires the env-selected backend into the tool
  catalog; the daemon session manager builds the jailed `ToolContext`.

## How Regent compares

| Capability | Claude Code | Hermes Agent | Regent |
|---|---|---|---|
| Dangerous-command approval | ✅ | ✅ `approval.py` | ✅ `guard.rs` |
| Filesystem containment | allow/deny lists (bwrap/Seatbelt) | `realpath()` within-root + write deny-list | ✅ canonical jail in `ToolContext` |
| Symlink-safe path checks | ✅ | ✅ `realpath()` | ✅ canonicalize prefix |
| Isolated command execution | OS sandbox (bwrap/Seatbelt) | Docker/Modal/Daytona/SSH | ✅ `docker exec` / ephemeral `docker run` / SSH |
| Container hardening | n/a (OS sandbox) | caps-drop, no-priv, pids, tmpfs | ✅ same flags |
| Network egress control | domain allow/deny + proxy | egress proxy / network seg | `--network none` (allowlist = future) |
| Secrets out of child env | network-off blocks exfil | ✅ keys stripped | ✅ `is_secret_env_var` strip |
| Fail-loud when unavailable | ✅ `failIfUnavailable` | (config-driven) | ✅ enforce-mode hard error |
| Config/secrets unwritable | deny settings paths | write deny-list | ✅ `$REGENT_HOME` outside jail |

## Enabling it

```bash
# Strongest posture for an externally-reachable daemon:
export REGENT_SANDBOX=1                       # jail + enforce (no host backend)
export REGENT_TERMINAL_BACKEND=sandbox:alpine # ephemeral, locked-down container per command
```

See also the **Sandboxing** section of [QUICKSTART.md](QUICKSTART.md).

## Deliberate non-goals (today) & future work

- **OS-level sandbox for `local`** (seccomp/Landlock on Linux, Seatbelt on macOS, Job Objects on
  Windows). Today host isolation is delegated to the container/SSH backend; `local` is unrestricted
  by design (trusted local dev). A `landlock:`-style backend is the natural next step on Linux.
- **Network domain allowlist** instead of all-or-nothing `--network none` — Claude Code's egress
  proxy model (allow `api.anthropic.com`, deny the rest).
- **Write deny-list in non-sandbox mode** — protect `~/.ssh`, `/etc/shadow`, etc. even when no jail
  root is set (the jail already covers sandbox mode).
- **Per-platform tool scoping** — Hermes restricts which tools a given channel may call (e.g.
  Discord → read-only). Regent's gateway could attach a capability set per platform so external
  chats never reach the `terminal` tool at all.
