# payload/

What Regent Setup carries inside itself. Everything here except this file is a
build artifact — generated, gitignored, never committed.

Fill it with:

```sh
pwsh scripts/build-payload.ps1     # Windows
sh   scripts/build-payload.sh      # macOS / Linux
```

That produces:

| File | What it is |
| --- | --- |
| `regent-<os>-<arch>.zip` / `.tar.gz` | `regent-deacon` + `regent-cli`, same layout as the GitHub release asset |
| `install.ps1` / `install.sh` | the one-line installer, run offline via `REGENT_LOCAL_ARCHIVE` |
| `app/Regent.exe` (or `app/Regent`) | the desktop app, copied to `<install_dir>/app` |

`tauri.conf.json` bundles `payload/**/*` as resources, so the built Setup binary
needs no network at install time.

This README is committed on purpose: the resource glob fails the build if the
directory has no matching files, so an unpopulated `payload/` would break
`bun run tauri dev` for anyone who hasn't run the build script yet.
