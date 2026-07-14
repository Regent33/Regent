# Desktop app (experimental)

The Tauri + React desktop app lives at `src/regent-app/Desktop`. It is
**experimental and source-built** — not part of the prebuilt release download.

## Install (one script, builds everything)

From a checkout, or standalone (it clones one to `~/.regent/src`):

```powershell
# Windows
scripts\install-desktop.ps1            # add --run to launch the installer it builds
```
```bash
# macOS / Linux
sh scripts/install-desktop.sh
```

It checks prerequisites (git, cargo, bun — with install URLs), builds
`regent-deacon` into `~/.regent/bin`, pins `REGENT_DEACON_PATH` so the
installed app finds it, then runs `tauri build` to produce the native
installer (`.msi`/`.exe` on Windows, `.dmg`/`.app` on macOS,
`.deb`/`.AppImage` on Linux) and prints its path. Linux also needs the
WebKitGTK/GTK dev packages (the script lists them).

## Dev loop

```bash
cargo build --release -p regent-deacon   # from the repo root, once
cd src/regent-app/Desktop
bun install
bun run tauri dev                        # dev shell (or `bun run build` for the web bundle)
```

The app finds the deacon the same way the CLI does (`REGENT_DEACON_PATH`,
sibling binary, `PATH`, cargo `target/`) — the installer pins the first of
those. Vite stack notes: ADR-034.
