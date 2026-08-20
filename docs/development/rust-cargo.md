# Building the Rust core (cargo)

The Rust side is a cargo **workspace** (`Cargo.toml` at the repo root, members
under `src/crates/`). All commands run from the repo root.

## Binaries
| Binary | Crate | What it is |
|---|---|---|
| `regent-deacon` | `regent-deacon` | the JSON-RPC core the CLI talks to (sessions, agent turns, memory, voice). `regent-deacon mcp` also serves the MCP surface — what `regent mcp serve` spawns |
| `regent-gateway` | `regent-gateway` | the messaging gateway (Telegram, …) — separate long-running process |
| `regent-voice-server` | `regent-voice-server` | local ASR/TTS (whisper + Kokoro) — what the mic and Butler Mode spawn. **Links sherpa-onnx dynamically**, so `sherpa-onnx-c-api` and `onnxruntime` must sit beside it; its `build.rs` sets an `$ORIGIN`/`@loader_path` rpath so a copied-anywhere binary finds them |

Built binaries land in `target/debug/<name>(.exe)` (or `target/release/…` with `--release`).

All three ship in the release archive, along with the shared libraries the
voice server loads. Every one of those used to be missing, which is why this
list is worth keeping accurate: `regent-voice-server` was never packaged (voice
failed on every install), neither was `regent-gateway` (`regent gateway start`
reported "not found"), and when the voice server finally did ship it still
could not start, because its libraries did not come with it — exit 0xC0000135
on Windows, with no message. The MCP server was a fourth binary with the same
packaging gap; it is a deacon subcommand now because it lives in that crate
anyway and cost 49MB to ship separately.

The release job now starts the packaged voice server and fails if it exits,
which is the only check in this repo that would have caught the library gap:
existence checks all passed while the binary could not load.

## Everyday commands
```bash
cargo build -p regent-deacon      # build just the daemon (what the CLI needs)
cargo build --workspace           # build everything (all crates + binaries)
cargo build --release             # optimized build → target/release/
cargo build -p regent-gateway     # build the gateway binary

cargo test --workspace            # run all tests
cargo test -p regent-speech       # test one crate
cargo test -p regent-embed -- --ignored   # run the #[ignore] tests (real model download)

cargo clippy --workspace --all-targets     # lint (warnings are not allowed in CI)
cargo fmt                                   # format
cargo fmt --check                           # CI-style format check

cargo check --workspace           # fast type-check without producing binaries
```

## Clean / rebuild
```bash
cargo clean                       # delete target/ entirely (full rebuild next time)
cargo clean -p regent-deacon      # clean just one crate's artifacts
```
> After `cargo clean`, `regent` will report "regent-deacon not found" until you
> `cargo build -p regent-deacon` again — the CLI loads the compiled binary.

## Running the binaries directly
```bash
cargo run -p regent-deacon                    # run the daemon in the foreground
cargo run -p regent-gateway                    # run the Telegram gateway (needs env, see voice-and-api-calls.md)
./target/debug/regent-deacon                   # run the built binary directly
```
Normally you don't run the daemon by hand — `regent` (the CLI) spawns it. Run it
directly only for debugging.

## Crate map (where things live)
```
regent-kernel     contracts + shared types (the only freely-imported layer)
regent-store      SQLite (sessions/messages/FTS5)        regent-graph    graph memory
regent-embed      local ONNX embeddings                  regent-speech   ASR/TTS stack
regent-providers  LLM chat providers (OpenAI-compat/Anthropic)
regent-tools      agent tools                            regent-skills   skills loader
regent-cron       scheduler                              regent-agent    the agent loop
regent-gateway    messaging surface (+ regent-gateway bin)
regent-deacon     JSON-RPC core (+ regent-deacon bin, which also serves `mcp`)
```

## CI gate (what "green" means)
Run each in turn — separate lines so it also works in PowerShell (no `&&`):
```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
```
