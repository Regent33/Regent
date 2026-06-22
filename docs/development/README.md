# Development & Setup Guide

How to build, run, and configure Regent locally. Regent is two planes:

- a **Rust/Tokio core** (a cargo workspace under `src/crates/`) — the daemon,
  gateway, agent, speech, store, etc.;
- a **TypeScript/Ink CLI** (`src/regent-cli/`, run with Bun) — a thin JSON-RPC
  client that talks to the Rust daemon.

The `regent` command you run is the **CLI**; it spawns/loads the **`regent-daemon`
binary** to do the actual work. So you must build the Rust binaries before the
CLI is useful.

## Contents
- [Building the Rust core (cargo)](rust-cargo.md) — build/clean/test/clippy, the binaries, where they land.
- [Building the TypeScript CLI (bun)](typescript-cli.md) — install/dev/compile/test/lint.
- [Voice & API calls](voice-and-api-calls.md) — how the chat model, ASR/TTS, and vision are configured and called.

## Prerequisites
- **Rust** — toolchain is pinned by [`rust-toolchain.toml`](../../rust-toolchain.toml) (edition 2024, ≥ 1.96). `rustup` installs it automatically on first `cargo` run.
- **Bun** — for the CLI (`src/regent-cli/`). https://bun.sh
- **ffmpeg** — optional, only if a future local speech path needs decoding (the current OpenAI-compatible voice path does **not** require it).

## "regent-daemon not found" — the first thing to fix
```
$ regent
✗ regent-daemon not found (set REGENT_DAEMON_PATH or build with `cargo build -p regent-daemon`)
```
The CLI couldn't locate the daemon binary. It searches, in order
([`locate.ts`](../../src/regent-cli/src/shared/infrastructure/daemon/locate.ts)):

1. `$REGENT_DAEMON_PATH` (explicit override),
2. a sibling of the `regent` executable,
3. `PATH`,
4. walking up from both the exe's dir and the **current directory** for
   `target/release/` then `target/debug/`.

It failed because the binary was never compiled. Fix:

```bash
cargo build -p regent-daemon          # → target/debug/regent-daemon.exe
regent                                 # run from inside the repo: step 4 finds target/debug
```

Running `regent` from outside the repo? Either build `--release` and run from a
repo subdir, or point at the binary explicitly:

```bash
# PowerShell
$env:REGENT_DAEMON_PATH = "D:\1-1@k\@ServeAI\Regent\target\debug\regent-daemon.exe"
```

## One-time full setup
```bash
cargo build --workspace                      # all Rust binaries (daemon, gateway, mcp, repl)
cd src/regent-cli && bun install && bun run compile   # the CLI single-binary → dist/regent-cli(.exe)
regent setup                                  # pick provider + model + API key
regent doctor                                 # sanity check
regent chat                                   # go
```
