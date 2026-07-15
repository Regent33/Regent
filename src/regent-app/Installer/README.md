# Regent Setup — unified GUI installer

One graphical installer for all of Regent — the agent core (`regent-deacon`),
the CLI (`regent`), and the desktop app — in a single flow. Tauri + Vite +
React, reusing the Desktop app's exact design tokens, Kontes display font, and
icon so Setup reads as the same product.

## Preview the UI as a dev — no native build needed

```bash
bun install
bun run dev        # → http://localhost:3100
```

The browser preview walks the **whole** wizard — welcome → license → location →
progress → finish — with a *simulated* install, so you can iterate on the UI
without compiling the Rust shell. Navigation, motion, dark/light, and every
screen are live.

## Build the frontend

```bash
bun run build      # tsc --noEmit + vite build → out/
```

## Status

- **Phase 1 (done):** frontend scaffold + all six screens, dev-previewable.
- **Phase 2 (next):** Rust backend that places the bundled prebuilt binaries,
  emits real staged progress, and wires PATH / shortcuts / uninstall (Option A —
  runs the bundled `install.ps1`/`.sh` in a local/offline mode).
- **Phase 3+:** emil polish pass, per-OS packaging (`.exe` / `.dmg` /
  `.AppImage`), and code-signing.
