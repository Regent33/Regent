# Desktop app (experimental)

The Tauri + React desktop app lives at `src/regent-app/Desktop`. It is
**experimental and source-only** — not part of the release download — and it
needs a locally built deacon to talk to.

```bash
cargo build --release -p regent-deacon   # from the repo root, once
cd src/regent-app/Desktop
npm install
npm run tauri dev                        # dev shell (or `npm run build` for the web bundle)
```

The app finds the deacon the same way the CLI does (`REGENT_DEACON_PATH`,
sibling binary, `PATH`, cargo `target/`). Vite stack notes: ADR-034.
