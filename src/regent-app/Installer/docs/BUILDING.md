# Building Regent Setup

Regent Setup is one graphical executable that installs all of Regent: the agent
core (`regent-deacon`), the `regent` CLI, and the desktop app. It carries them
inside itself, so the machine it runs on needs no network, no Rust, and no Bun.

## What it is

A Tauri 2 app — React frontend in `app/`, Rust backend in `src-tauri/`. It uses
the Regent desktop app's design tokens verbatim, forced to the light theme.

At install time it does three things, streamed to a live log:

| Stage | What happens |
| --- | --- |
| `core` | Runs the bundled `install.ps1` / `install.sh` with `REGENT_LOCAL_ARCHIVE` pointing at the bundled archive. The script extracts `regent-deacon` + `regent-cli` into `<install_dir>/bin`, writes the shim, and adds it to PATH. |
| `app` | Copies the desktop app executable into `<install_dir>/app`. |
| `wire` | Writes the desktop shortcut and the Apps & features uninstall entry. |

The core stage reuses the *same* install script as the `curl \| sh` one-liner —
there is one implementation of "extract and put on PATH", not two.

## Build

Two steps, in order. The payload must exist before the app is built: the
`payload/**/*` resource glob in `tauri.conf.json` fails the build otherwise.

```sh
# 1. Stage what Setup carries (release builds of deacon, CLI, and the app)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-payload.ps1   # Windows
sh scripts/build-payload.sh                                                     # macOS / Linux

# 2. Build Setup itself
bun install
bun run tauri build
```

`build-payload` takes 10–20 minutes cold — it release-builds the deacon, compiles
the CLI to a single binary, and builds the desktop app. Iterate with `-SkipCore`
/ `-SkipApp` (PowerShell) or `SKIP_CORE=1` / `SKIP_APP=1` (sh).

Output lands in `src-tauri/target/release/bundle/`:

| Platform | Artifact |
| --- | --- |
| Windows | `nsis/Regent Setup_<version>_x64-setup.exe` |
| macOS | `dmg/Regent Setup_<version>_<arch>.dmg` |
| Linux | `appimage/Regent Setup_<version>_amd64.AppImage` |

Each is built on its own OS. There is no cross-compilation path — the payload
contains native binaries for the host.

### Developing

```sh
bun run dev          # browser only, http://localhost:3100 — walks a simulated install
bun run tauri dev    # the real window and the real Rust backend
```

`bun run dev` has no Tauri backend, so `runInstall` falls back to a simulation
and every screen is still reachable. `bun run tauri dev` runs the real install
code; with no payload staged it fails the `core` stage with
"no bundled payload — run scripts/build-payload.ps1 first", which is the failure
screen doing its job. Stage a payload to exercise the real path — a dev run
reads `src-tauri/payload/` straight from the source tree.

## What gets installed where

Per-user, always. Nothing needs administrator, so there is no elevation prompt
and no elevated-relaunch path to get wrong.

```
%LOCALAPPDATA%\Programs\Regent\      (default; the user can change it)
  bin\    regent-deacon.exe, regent-cli.exe, regent.cmd
  app\    Regent.exe
  uninstall.ps1
```

The desktop app finds the deacon through a persisted user `REGENT_DEACON_PATH`,
set during the `wire` stage — deliberately not via PATH. Its `find_deacon()`
does fall back to PATH, but PATH is optional here (the checkbox), and a PATH
written during install is invisible to every process that already exists,
including the app launched from the finish screen. On Linux the `.desktop` entry
carries the variable in its `Exec` line for the same reason.

macOS/Linux default to `~/.local/share/Regent`, with the CLI linked into
`~/.local/bin/regent` by `install.sh`.

The user's data — `~/.regent`, holding config, API keys, and memory — is
separate, and the uninstaller deliberately leaves it alone. Uninstalling an app
is not consent to delete someone's data.

## Uninstall

Windows registers under `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Regent`,
so Regent appears in Apps & features like any other program. It runs
`uninstall.ps1`, which stops running processes, removes the PATH entry, deletes
the shortcut, and removes the install directory.

macOS/Linux get an `uninstall.sh` in the install directory — there is no
Add/Remove-Programs equivalent to register with.

## Code signing

Unsigned builds work but warn: Windows shows a SmartScreen "unknown publisher"
prompt, and macOS refuses to open the app at all.

### Windows (Authenticode)

`tauri.conf.json` already sets `digestAlgorithm: "sha256"` and a `timestampUrl`.
The certificate itself is not committed — pass its SHA-1 thumbprint at build
time:

```sh
# Find the thumbprint of an installed cert
powershell -Command "Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Format-List Subject, Thumbprint"

# Build with it
bun run tauri build --config '{\"bundle\":{\"windows\":{\"certificateThumbprint\":\"<THUMBPRINT>\"}}}'
```

Signing uses `signtool.exe` (Windows SDK) by default, so it only runs on
Windows. Two config fields matter if your CA needs them: set `tsp: true` if your
provider uses an RFC-3161 timestamp server (SSL.com does), and use `signCommand`
— a command with a `%1` placeholder for the binary path — to sign with something
other than signtool, e.g. a cloud HSM or `osslsigncode`.

### macOS (Developer ID + notarization)

Requires a paid Apple Developer account. Set `bundle.macOS.signingIdentity` to
your Developer ID Application identity; `hardenedRuntime` is already on by
default, which notarization requires. Add an `entitlements` file if the app ever
needs a capability the hardened runtime blocks, and `providerShortName` if your
Apple ID belongs to more than one team.

Notarization credentials are passed as environment variables to
`bun run tauri build`. Check the Tauri v2 signing docs for the current variable
names before wiring CI — they are read by the bundler binary that ships with
`@tauri-apps/cli`, not by anything in this repo, so this file would go stale.

### Linux

AppImages are not signed. Nothing to configure.

## Gotchas

- **`payload/README.md` is committed on purpose.** The `payload/**/*` resource
  glob fails the build if nothing matches, so an empty `payload/` would break
  `tauri dev` for anyone who has not run the build script yet. The README keeps
  the glob satisfied.
- **`src-tauri` opts out of the root workspace** (empty `[workspace]` table),
  same as the desktop app: Tauri wants edition 2021 and a self-contained dep
  graph, and the repo root is edition 2024.
- **The desktop app is bundled as a bare executable**, not as an `.exe`
  installer or `.app`. `build-payload` passes `--no-bundle` — nesting an
  installer inside an installer would be pointless.
- **WebView2** is handled by `webviewInstallMode: embedBootstrapper`, so
  Windows machines without it get it during install.
