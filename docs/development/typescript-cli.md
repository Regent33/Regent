# Building the TypeScript CLI (bun)

The CLI lives in [`src/regent-cli/`](../../src/regent-cli/) and runs on **Bun**.
It is a thin JSON-RPC client + Ink terminal UI; it does no agent work itself —
it spawns the `regent-deacon` binary (see [rust-cargo.md](rust-cargo.md)).

All commands run from `src/regent-cli/`.

## Scripts (`package.json`)
```bash
bun install            # install deps (first time)

bun run dev            # run from source: `bun run src/main.tsx` (opens chat)
bun run src/main.tsx chat        # dev with args (any subcommand)

bun run typecheck      # tsc --noEmit (no JS emitted; type check only)
bun run test           # bun test (unit tests)
bun run lint           # biome check src
bun run lint:fix       # biome check --write src (auto-fix + sort imports)

bun run compile        # build the single-binary → dist/regent-cli(.exe)
```

## Dev vs. compiled
- **Dev (fast iteration):** `bun run src/main.tsx <cmd>` runs straight from
  TypeScript — your edits take effect immediately, no rebuild.
- **Compiled (`regent`):** `bun run compile` bundles everything into
  `dist/regent-cli.exe`. The `regent` command on your `PATH` is this compiled
  binary — **source edits do NOT appear until you re-run `bun run compile`.**
  (If `regent` seems to ignore a change, you forgot to recompile.)

## Making `regent` available globally
After `bun run compile`, expose `dist/regent-cli(.exe)` as `regent`:
```bash
bun link                       # from src/regent-cli — links the `regent` bin onto PATH (~/.bun/bin)
# or add src/regent-cli/dist to PATH, or copy/rename the exe to `regent`
```
Run `regent` from **inside the repo** so it can find `target/debug/regent-deacon`
(it walks up from the current directory). From elsewhere, set
`REGENT_DEACON_PATH` (see [README](README.md)).

## Tests / lint targets
```bash
bun test src/features/voice         # one folder
bun test src/app/config             # the slash-command picker tests
```
The repo's bar: `bun run typecheck` + `bun run test` + `bun run lint` all clean.

## Layout (feature-based clean architecture)
```
src/
  app/        cli router, config (commands, brand), di, presentation shell
  features/   chat, voice, gateway, cron, memory, … (each: cli/ domain/ data/ presentation/)
  shared/     kernel (Result, contracts), infrastructure (daemon locate/spawn, rpc client), ui
```
