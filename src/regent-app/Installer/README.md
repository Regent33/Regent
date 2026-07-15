# Regent Setup — unified GUI installer

One graphical installer for all of Regent — the agent core (`regent-deacon`),
the CLI (`regent`), and the desktop app — in a single flow. Tauri + Vite +
React, reusing the Desktop app's exact design tokens, Kontes display font, and
icon so Setup reads as the same product.

It carries everything it installs, so the machine running it needs no network,
no Rust, and no Bun.

## Preview the UI as a dev — no native build needed

```bash
bun install
bun run dev        # → http://localhost:3100
```

The browser preview walks the **whole** wizard — welcome → license → location →
progress → finish — with a *simulated* install, so you can iterate on the UI
without compiling the Rust shell or staging a payload.

Add `?uninstall` (→ http://localhost:3100/?uninstall) for the uninstall flow:
confirm → progress → removed. Same binary, same design, second mode.

## Run the real thing

```bash
bun run tauri dev                      # real window, real backend, real install
bun run tauri dev -- -- --uninstall    # the uninstall flow
```

The `--uninstall` flag exists only for dev: the shipped uninstaller is routed by
its file name, which `tauri dev` cannot change.

This runs the actual install code. Without a staged payload it fails the `core`
stage with a clear message — that is the failure screen working, not a bug. To
exercise the real path, stage a payload first:

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-payload.ps1
```

A dev run reads `src-tauri/payload/` straight from the source tree.

## Build a shippable installer

See **[docs/BUILDING.md](docs/BUILDING.md)** — what the payload contains, per-OS
artifacts, where things get installed, uninstall, and code signing.

```bash
bun run build        # frontend only: tsc --noEmit + vite build → out/
bun run tauri build  # the installer itself (stage a payload first)
```
